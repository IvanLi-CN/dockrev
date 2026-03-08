#!/usr/bin/env bun
/**
 * codex-testbox regression: version inference SSE stream availability
 *
 * This script runs Dockrev + a fixture compose project on `codex-testbox` in an isolated
 * remote workspace, then validates:
 *  - `/api/version-inference/events` streams `version_inference_event` with monotonic event id
 *  - triggering service refresh emits `task_enqueued`
 *  - reconnect with `afterId=<lastEventId>` can continue receiving newer events
 *
 * Design notes:
 *  - No npm deps; only Bun + system tools (ssh, rsync, git).
 *  - Keeps all remote writes under /srv/codex/**.
 *  - Runs Dockrev as a host process on the testbox for better LXC reliability.
 */
import { createHash } from "crypto";
import fs from "fs";
import net from "net";
import path from "path";

type JsonValue = any;

type ParsedSseEvent = {
  id: number | null;
  event: string;
  data: string;
};

class SseStream {
  private readonly controller: AbortController;
  private readonly reader: ReadableStreamDefaultReader<Uint8Array>;
  private readonly decoder = new TextDecoder();
  private buffer = "";
  private queue: ParsedSseEvent[] = [];

  static async open(url: string, headers: Record<string, string>): Promise<SseStream> {
    const controller = new AbortController();
    const resp = await fetch(url, { headers, signal: controller.signal });
    if (!resp.ok) throw new Error(`SSE connect failed: status=${resp.status} url=${url}`);
    if (!resp.body) throw new Error(`SSE response has no body: ${url}`);
    const stream = new SseStream(resp.body.getReader(), controller);
    return stream;
  }

  private constructor(reader: ReadableStreamDefaultReader<Uint8Array>, controller: AbortController) {
    this.reader = reader;
    this.controller = controller;
  }

  close() {
    this.controller.abort();
    void this.reader.cancel().catch(() => {});
  }

  async next(timeoutMs: number): Promise<ParsedSseEvent> {
    const deadline = Date.now() + timeoutMs;

    while (Date.now() < deadline) {
      if (this.queue.length > 0) return this.queue.shift()!;

      const remain = Math.max(1, deadline - Date.now());
      const read = this.reader.read();
      const chunk = await Promise.race([
        read,
        new Promise<never>((_, reject) => setTimeout(() => reject(new Error("sse read timeout")), remain)),
      ]);

      if (chunk.done) throw new Error("sse stream closed");
      this.buffer += this.decoder.decode(chunk.value, { stream: true }).replace(/\r\n/g, "\n");
      this.flushBufferToQueue();
    }

    throw new Error(`timeout waiting sse event (${timeoutMs}ms)`);
  }

  private flushBufferToQueue() {
    while (true) {
      const sep = this.buffer.indexOf("\n\n");
      if (sep < 0) return;

      const block = this.buffer.slice(0, sep);
      this.buffer = this.buffer.slice(sep + 2);
      const evt = this.parseBlock(block);
      if (evt) this.queue.push(evt);
    }
  }

  private parseBlock(block: string): ParsedSseEvent | null {
    let id: number | null = null;
    let event = "";
    const dataParts: string[] = [];

    for (const rawLine of block.split("\n")) {
      const line = rawLine.trimEnd();
      if (!line || line.startsWith(":")) continue;

      const idx = line.indexOf(":");
      const field = idx < 0 ? line : line.slice(0, idx);
      const value = idx < 0 ? "" : line.slice(idx + 1).replace(/^ /, "");

      if (field === "id") {
        const n = Number.parseInt(value, 10);
        id = Number.isFinite(n) ? n : null;
      } else if (field === "event") {
        event = value;
      } else if (field === "data") {
        dataParts.push(value);
      }
    }

    if (!event && dataParts.length === 0 && id == null) return null;
    return {
      id,
      event,
      data: dataParts.join("\n"),
    };
  }
}

function env(name: string, fallback?: string): string | undefined {
  const v = process.env[name];
  if (v === undefined || v === "") return fallback;
  return v;
}

function envBool(name: string, fallback: boolean): boolean {
  const v = env(name);
  if (!v) return fallback;
  return v === "1" || v.toLowerCase() === "true" || v.toLowerCase() === "yes";
}

function envInt(name: string, fallback: number): number {
  const v = env(name);
  if (!v) return fallback;
  const n = Number.parseInt(v, 10);
  return Number.isFinite(n) ? n : fallback;
}

function nowRunId(gitSha: string): string {
  const d = new Date();
  const iso = d.toISOString();
  const y = iso.slice(0, 4);
  const m = iso.slice(5, 7);
  const day = iso.slice(8, 10);
  const hh = iso.slice(11, 13);
  const mm = iso.slice(14, 16);
  const ss = iso.slice(17, 19);
  return `${y}${m}${day}_${hh}${mm}${ss}Z_${gitSha || "nogit"}`;
}

function sanitizeComposeProject(raw: string): string {
  const lowered = raw.toLowerCase();
  const cleaned = lowered.replace(/[^a-z0-9_-]+/g, "_").replace(/^_+|_+$/g, "");
  return cleaned.length > 63 ? cleaned.slice(0, 63) : cleaned;
}

async function run(
  cmd: string[],
  opts?: { cwd?: string; env?: Record<string, string>; stdin?: string; allowFailure?: boolean },
): Promise<{ code: number; stdout: string; stderr: string }> {
  const p = Bun.spawn(cmd, {
    cwd: opts?.cwd,
    env: { ...process.env, ...(opts?.env || {}) },
    stdin: opts?.stdin ? "pipe" : "ignore",
    stdout: "pipe",
    stderr: "pipe",
  });
  if (opts?.stdin) {
    p.stdin.write(opts.stdin);
    p.stdin.end();
  }
  const [stdoutBuf, stderrBuf, code] = await Promise.all([p.stdout.text(), p.stderr.text(), p.exited]);
  const out = { code, stdout: stdoutBuf.trimEnd(), stderr: stderrBuf.trimEnd() };
  if (!opts?.allowFailure && code !== 0) {
    throw new Error(
      `command failed (code=${code}): ${cmd.join(" ")}\n--- stdout ---\n${out.stdout}\n--- stderr ---\n${out.stderr}\n`,
    );
  }
  return out;
}

function bashQuote(s: string): string {
  return `'${s.replace(/'/g, `'\"'\"'`)}'`;
}

async function ssh(host: string, sshOpts: string[], script: string, allowFailure?: boolean) {
  const cmd = ["ssh", ...sshOpts, host, "bash", "-s", "--"];
  return await run(cmd, { stdin: script, allowFailure });
}

async function rsyncToRemote(host: string, sshOpts: string[], srcDir: string, remoteDir: string) {
  const sshCmd = ["ssh", ...sshOpts].join(" ");
  const excludes = [
    ".git/",
    "node_modules/",
    "web/node_modules/",
    "target/",
    "dist/",
    ".tmp/",
    "downloads/",
  ];
  const args = [
    "rsync",
    "-az",
    "--delete",
    ...excludes.flatMap((e) => ["--exclude", e]),
    "-e",
    sshCmd,
    `${srcDir.replace(/\/$/, "")}/`,
    `${host}:${remoteDir.replace(/\/$/, "")}/`,
  ];
  await run(args);
}

async function rsyncFileToRemote(host: string, sshOpts: string[], srcFile: string, remoteFile: string) {
  const sshCmd = ["ssh", ...sshOpts].join(" ");
  await run(["rsync", "-az", "-e", sshCmd, srcFile, `${host}:${remoteFile}`]);
}

function resolveArtifactPath(repoRoot: string, value: string): string {
  return path.isAbsolute(value) ? value : path.join(repoRoot, value);
}

function section(title: string) {
  console.log(`\n[${title}]`);
}

function info(msg: string) {
  console.log(msg);
}

function assert(cond: unknown, msg: string) {
  if (!cond) throw new Error(`ASSERTION FAILED: ${msg}`);
}

async function findFreeLocalPort(start: number): Promise<number> {
  for (let p = start; p < start + 2000; p++) {
    const ok = await new Promise<boolean>((resolve) => {
      const s = net.createServer();
      s.once("error", () => resolve(false));
      s.listen(p, "127.0.0.1", () => {
        s.close(() => resolve(true));
      });
    });
    if (ok) return p;
  }
  throw new Error("failed to find a free local port");
}

async function jsonRequest(baseUrl: string, input: { method: string; path: string; body?: JsonValue; headers?: Record<string, string> }) {
  const url = `${baseUrl}${input.path}`;
  const headers: Record<string, string> = {
    ...(input.headers || {}),
  };
  if (input.body !== undefined) headers["content-type"] = "application/json";
  const resp = await fetch(url, {
    method: input.method,
    headers,
    body: input.body === undefined ? undefined : JSON.stringify(input.body),
  });
  const text = await resp.text();
  let data: any = null;
  if (text.trim() !== "") {
    try {
      data = JSON.parse(text);
    } catch {
      data = { raw: text };
    }
  }
  return { status: resp.status, data, raw: text };
}

async function sleep(ms: number) {
  await new Promise((r) => setTimeout(r, ms));
}

async function waitForHealth(baseUrl: string, timeoutSeconds: number) {
  const deadline = Date.now() + timeoutSeconds * 1000;
  while (Date.now() < deadline) {
    try {
      const resp = await fetch(`${baseUrl}/api/health`);
      if (resp.ok) {
        const body = (await resp.text()).trim();
        if (body === "ok") return;
      }
    } catch {
      // ignore
    }
    await sleep(500);
  }
  throw new Error(`timeout waiting for ${baseUrl}/api/health`);
}

async function waitForJob(baseUrl: string, jobId: string, headers: Record<string, string>, timeoutSeconds: number) {
  const deadline = Date.now() + timeoutSeconds * 1000;
  let last: any = null;
  while (Date.now() < deadline) {
    const r = await jsonRequest(baseUrl, { method: "GET", path: `/api/jobs/${encodeURIComponent(jobId)}`, headers });
    if (r.status !== 200) {
      throw new Error(`GET /api/jobs/${jobId} failed: status=${r.status} body=${JSON.stringify(r.data)}`);
    }
    const job = r.data?.job;
    last = job;
    if (job?.finishedAt) return job;
    await sleep(800);
  }
  throw new Error(`timeout waiting for job ${jobId}; last=${JSON.stringify(last)}`);
}

async function triggerDiscovery(baseUrl: string, headers: Record<string, string>): Promise<string> {
  const r = await jsonRequest(baseUrl, { method: "POST", path: "/api/discovery/scan", body: {}, headers });
  if (r.status !== 200) {
    throw new Error(`POST /api/discovery/scan failed: status=${r.status} body=${JSON.stringify(r.data)}`);
  }
  const jobId = r.data?.jobId || r.data?.job_id;
  if (!jobId) throw new Error(`discovery response missing jobId: ${JSON.stringify(r.data)}`);
  return jobId;
}

async function triggerCheckAll(baseUrl: string, headers: Record<string, string>) {
  const r = await jsonRequest(baseUrl, { method: "POST", path: "/api/checks", body: { scope: "all", reason: "ui" }, headers });
  if (r.status === 200) {
    const checkId = r.data?.checkId || r.data?.check_id;
    if (!checkId) throw new Error(`check response missing checkId: ${JSON.stringify(r.data)}`);
    return { ok: true as const, checkId };
  }
  if (r.status === 409) {
    const existingJobId = r.data?.error?.details?.existingJobId;
    return { ok: false as const, status: 409, existingJobId, body: r.data };
  }
  return { ok: false as const, status: r.status, body: r.data };
}

async function waitForDiscoveryStack(baseUrl: string, headers: Record<string, string>, project: string, timeoutSeconds: number) {
  const deadline = Date.now() + timeoutSeconds * 1000;
  while (Date.now() < deadline) {
    const r = await jsonRequest(baseUrl, { method: "GET", path: "/api/discovery/projects", headers });
    if (r.status === 200 && Array.isArray(r.data?.projects)) {
      const hit = r.data.projects.find((p: any) => p?.project === project && typeof p?.stackId === "string" && p.stackId);
      if (hit) return hit.stackId as string;
    }
    await sleep(700);
  }
  throw new Error(`timeout waiting stack id for discovery project=${project}`);
}

async function triggerVersionInferenceRefresh(baseUrl: string, serviceId: string, headers: Record<string, string>) {
  const r = await jsonRequest(baseUrl, {
    method: "POST",
    path: `/api/services/${encodeURIComponent(serviceId)}/version-inference/refresh`,
    body: {},
    headers,
  });
  if (r.status === 400 && r.data?.error?.code === "invalid_argument") {
    return null;
  }
  if (r.status !== 202) {
    throw new Error(
      `POST /api/services/${serviceId}/version-inference/refresh failed: status=${r.status} body=${JSON.stringify(r.data)}`,
    );
  }
  return r.data as { serviceId: string; imageRepo: string; reason: string; status: string };
}

async function waitForWorkerIdle(baseUrl: string, headers: Record<string, string>, timeoutSeconds: number) {
  const deadline = Date.now() + timeoutSeconds * 1000;
  while (Date.now() < deadline) {
    const r = await jsonRequest(baseUrl, { method: "GET", path: "/api/version-inference/overview?page=1&perPage=1", headers });
    if (r.status === 200) {
      const inFlight = Number(r.data?.worker?.inFlight ?? 0);
      if (inFlight <= 0) return;
    }
    await sleep(1000);
  }
  throw new Error(`timeout waiting version inference worker idle (${timeoutSeconds}s)`);
}

function parseJsonOrThrow(raw: string, context: string) {
  try {
    return JSON.parse(raw) as any;
  } catch (e) {
    throw new Error(`${context}: invalid json payload: ${raw}; err=${(e as Error).message}`);
  }
}

async function waitForVersionInferenceEvent(
  stream: SseStream,
  timeoutMs: number,
  predicate: (evt: ParsedSseEvent, payload: any) => boolean,
): Promise<{ evt: ParsedSseEvent; payload: any }> {
  const deadline = Date.now() + timeoutMs;
  let maxSeenId = 0;

  while (Date.now() < deadline) {
    const evt = await stream.next(Math.max(1, deadline - Date.now()));
    if (evt.id != null) {
      assert(evt.id >= maxSeenId, `non-monotonic SSE id: id=${evt.id} prev=${maxSeenId}`);
      maxSeenId = evt.id;
    }

    if (evt.event !== "version_inference_event") continue;
    const payload = parseJsonOrThrow(evt.data, "version_inference_event");
    if (predicate(evt, payload)) return { evt, payload };
  }

  throw new Error(`timeout waiting version_inference_event (${timeoutMs}ms)`);
}

async function main() {
  const testboxHost = env("TESTBOX_HOST", "codex-testbox")!;
  const sshOpts = (env("TESTBOX_SSH_OPTS", "-o BatchMode=yes") || "")
    .split(" ")
    .map((s) => s.trim())
    .filter(Boolean);

  const keepOnFailure = envBool("DOCKREV_TEST_KEEP", false);
  const overallTimeoutSeconds = envInt("DOCKREV_TEST_TIMEOUT_SECONDS", 180);
  const jobWaitSeconds = envInt("DOCKREV_JOB_WAIT_SECONDS", 60);
  const sseWaitMs = envInt("DOCKREV_SSE_WAIT_MS", 45_000);
  const smokeBinSetting = env("DOCKREV_SMOKE_BIN", "dist/ci/docker/amd64/dockrev")!;

  const authHeaderName = env("DOCKREV_AUTH_HEADER_NAME", "X-Forwarded-User")!;
  const authHeaderValue = env("DOCKREV_AUTH_HEADER_VALUE", "test")!;
  const apiHeaders = { [authHeaderName]: authHeaderValue };

  let deadline = 0;
  const startOverallTimer = () => {
    deadline = Date.now() + overallTimeoutSeconds * 1000;
  };
  const checkDeadline = () => {
    if (deadline !== 0 && Date.now() > deadline) throw new Error("overall timeout exceeded");
  };

  section("SETUP");
  const repoRoot = (await run(["git", "rev-parse", "--show-toplevel"])).stdout.trim();
  const repoRootReal = fs.realpathSync(repoRoot);
  const repoName = path.basename(repoRootReal);
  const gitSha = (await run(["git", "-C", repoRootReal, "rev-parse", "--short", "HEAD"], { allowFailure: true })).stdout.trim() || "nogit";
  const pathHash8 = createHash("sha256").update(repoRootReal).digest("hex").slice(0, 8);
  const runId = `${nowRunId(gitSha)}_sse`;
  const smokeBinLocal = resolveArtifactPath(repoRootReal, smokeBinSetting);
  const usePrebuiltSmokeBin = fs.existsSync(smokeBinLocal);

  const remoteUser = (await ssh(testboxHost, sshOpts, "whoami")).stdout.trim();
  assert(remoteUser.length > 0, "remote user is empty");
  const remoteWorkspace = `/srv/codex/workspaces/${remoteUser}/${repoName}__${pathHash8}`;
  const remoteRun = `${remoteWorkspace}/runs/${runId}`;
  const remoteSmokeBin = usePrebuiltSmokeBin ? `${remoteRun}/bin/dockrev` : `${remoteWorkspace}/target/testbox/release/dockrev`;

  const composeProjectRaw = `codex_${repoName}__${pathHash8}_${runId}`;
  const composeProject = sanitizeComposeProject(composeProjectRaw);
  const fixturesProject = sanitizeComposeProject(`${composeProject}_fixtures`);

  const forcedRemotePort = env("REMOTE_HTTP_PORT");
  let remoteHttpPort: string;
  if (forcedRemotePort) {
    remoteHttpPort = forcedRemotePort;
  } else {
    const probe = await ssh(
      testboxHost,
      sshOpts,
      `
set -euo pipefail
PY=python3
command -v "$PY" >/dev/null 2>&1 || PY=python
"$PY" - <<'PY'
import socket
start = 55000
for p in range(start, start + 2000):
    s = socket.socket()
    try:
        s.bind(("127.0.0.1", p))
    except OSError:
        try:
            s.close()
        except Exception:
            pass
        continue
    try:
        s.close()
    except Exception:
        pass
    print(p)
    raise SystemExit(0)
raise SystemExit(2)
PY
`,
    );
    remoteHttpPort = probe.stdout.trim();
    assert(remoteHttpPort.length > 0, "failed to probe a free remote port");
  }

  info(`repoRoot=${repoRootReal}`);
  info(`remoteUser=${remoteUser}`);
  info(`REMOTE_RUN=${remoteRun}`);
  info(`COMPOSE_PROJECT=${composeProject}`);
  info(`FIXTURES_PROJECT=${fixturesProject}`);
  info(`REMOTE_HTTP_PORT=${remoteHttpPort}`);
  info(`DOCKREV_SMOKE_BIN=${usePrebuiltSmokeBin ? smokeBinLocal : "fallback:remote-build"}`);
  info(`REMOTE_DOCKREV_BIN=${remoteSmokeBin}`);

  let remoteRunReady = false;
  const cleanupRemote = async (ok: boolean) => {
    if (!remoteRunReady) return;
    if (!ok && keepOnFailure) {
      section("CLEANUP");
      info("DOCKREV_TEST_KEEP=1 set; keeping remote run for debugging.");
      info(`REMOTE_RUN=${remoteRun}`);
      info(`FIXTURES_PROJECT=${fixturesProject}`);
      return;
    }
    section("CLEANUP");
    await ssh(
      testboxHost,
      sshOpts,
      `
set -euo pipefail
cd ${bashQuote(remoteRun)}
if [[ -f dockrev.pid ]]; then
  pid="$(cat dockrev.pid || true)"
  if [[ -n "$pid" ]]; then
    kill "$pid" >/dev/null 2>&1 || true
    sleep 0.2
    kill -9 "$pid" >/dev/null 2>&1 || true
  fi
fi
docker compose -p ${bashQuote(fixturesProject)} -f scripts/testbox/fixtures.compose.yml -f .codex.caps-compat.fixtures.yml down -v --remove-orphans || true
rm -rf ${bashQuote(remoteRun)} || true
`,
      true,
    );
  };

  let forwardProc: Bun.Subprocess | null = null;
  const cleanupForward = () => {
    if (!forwardProc) return;
    try {
      forwardProc.kill();
    } catch {
      // ignore
    }
    forwardProc = null;
  };

  let success = false;
  let streamA: SseStream | null = null;
  let streamB: SseStream | null = null;
  try {
    await ssh(
      testboxHost,
      sshOpts,
      `
set -euo pipefail
mkdir -p ${bashQuote(remoteRun)}
mkdir -p ${bashQuote(remoteWorkspace)}
mkdir -p ${bashQuote(`${remoteRun}/data`)}
cat > ${bashQuote(`${remoteWorkspace}/workspace.txt`)} <<'TXT'
local_repo_root=${repoRootReal}
git_sha=${gitSha}
run_id=${runId}
created_utc=${new Date().toISOString()}
TXT
`,
    );
    remoteRunReady = true;
    await rsyncToRemote(testboxHost, sshOpts, repoRootReal, remoteRun);

    checkDeadline();
    await ssh(
      testboxHost,
      sshOpts,
      `
set -euo pipefail
cd ${bashQuote(remoteRun)}

gen_caps() {
  local services="$1"
  local out="$2"
  {
    echo "services:"
    for s in $services; do
      cat <<YAML
  $s:
    cap_drop:
      - ALL
    cap_add:
      - CHOWN
      - DAC_OVERRIDE
      - FSETID
      - FOWNER
      - MKNOD
      - NET_RAW
      - SETGID
      - SETUID
      - SETPCAP
      - NET_BIND_SERVICE
      - SYS_CHROOT
      - KILL
      - AUDIT_WRITE
YAML
    done
  } > "$out"
}

services_fixtures="$(docker compose -f scripts/testbox/fixtures.compose.yml config --services)"
gen_caps "$services_fixtures" .codex.caps-compat.fixtures.yml

docker compose -p ${bashQuote(fixturesProject)} \\
  -f scripts/testbox/fixtures.compose.yml \\
  -f .codex.caps-compat.fixtures.yml \\
  up -d
`,
    );

    const buildStarted = Date.now();
    if (usePrebuiltSmokeBin) {
      section("STAGE PREBUILT");
      await ssh(
        testboxHost,
        sshOpts,
        `
set -euo pipefail
mkdir -p ${bashQuote(path.posix.dirname(remoteSmokeBin))}
`,
      );
      await rsyncFileToRemote(testboxHost, sshOpts, smokeBinLocal, remoteSmokeBin);
      await ssh(
        testboxHost,
        sshOpts,
        `
set -euo pipefail
chmod 0755 ${bashQuote(remoteSmokeBin)}
ls -la ${bashQuote(remoteSmokeBin)}
`,
      );
    } else {
      section("BUILD (remote)");
      await ssh(
        testboxHost,
        sshOpts,
        `
set -euo pipefail
cd ${bashQuote(remoteRun)}

uid="$(id -u)"
gid="$(id -g)"
export CARGO_HOME=${bashQuote(`${remoteWorkspace}/.cargo-home`)}
export CARGO_TARGET_DIR=${bashQuote(`${remoteWorkspace}/target/testbox`)}
mkdir -p "$CARGO_HOME" "$CARGO_TARGET_DIR"

caps=(
  --cap-drop=ALL
  --cap-add=CHOWN
  --cap-add=DAC_OVERRIDE
  --cap-add=FSETID
  --cap-add=FOWNER
  --cap-add=MKNOD
  --cap-add=NET_RAW
  --cap-add=SETGID
  --cap-add=SETUID
  --cap-add=SETPCAP
  --cap-add=NET_BIND_SERVICE
  --cap-add=SYS_CHROOT
  --cap-add=KILL
  --cap-add=AUDIT_WRITE
)

docker run --rm \
  --user "$uid:$gid" \
  "\${caps[@]}" \
  -e CARGO_HOME \
  -e CARGO_TARGET_DIR \
  -v ${bashQuote(remoteWorkspace)}:${bashQuote(remoteWorkspace)} \
  -w ${bashQuote(remoteRun)} \
  rust:1.91-bookworm \
  bash -c ${bashQuote("set -euo pipefail; export PATH=/usr/local/cargo/bin:$PATH; cargo build -p dockrev-api --bin dockrev --release --locked")}

ls -la ${bashQuote(remoteSmokeBin)}
`,
      );
    }
    const buildDuration = Math.round((Date.now() - buildStarted) / 1000);
    if (!usePrebuiltSmokeBin && buildDuration > buildTimeoutSeconds) {
      throw new Error(`remote build exceeded timeout: duration=${buildDuration}s timeout=${buildTimeoutSeconds}s`);
    }

    section("START (remote)");
    await ssh(
      testboxHost,
      sshOpts,
      `
set -euo pipefail
cd ${bashQuote(remoteRun)}

bin=${bashQuote(remoteSmokeBin)}
rm -f dockrev.pid dockrev.log
touch dockrev.log

export DOCKREV_HTTP_ADDR="127.0.0.1:${remoteHttpPort}"
export DOCKREV_DB_PATH=${bashQuote(`${remoteRun}/data/dockrev.sqlite3`)}
export DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV="true"

nohup "$bin" >> dockrev.log 2>&1 &
echo "$!" > dockrev.pid
`,
    );

    const localPort = await findFreeLocalPort(55000);
    const forwardArgs = [
      "ssh",
      ...sshOpts,
      "-o",
      "ExitOnForwardFailure=yes",
      "-N",
      "-L",
      `${localPort}:127.0.0.1:${remoteHttpPort}`,
      testboxHost,
    ];
    info(`localPortForward=127.0.0.1:${localPort} -> ${testboxHost}:127.0.0.1:${remoteHttpPort}`);
    forwardProc = Bun.spawn(forwardArgs, { stdout: "pipe", stderr: "pipe" });

    const baseUrl = `http://127.0.0.1:${localPort}`;
    startOverallTimer();
    await waitForHealth(baseUrl, Math.max(20, Math.min(60, overallTimeoutSeconds)));

    section("DISCOVERY");
    const discoveryJobId = await triggerDiscovery(baseUrl, apiHeaders);
    info(`discoveryJobId=${discoveryJobId}`);
    await waitForJob(baseUrl, discoveryJobId, apiHeaders, jobWaitSeconds);
    const stackId = await waitForDiscoveryStack(baseUrl, apiHeaders, fixturesProject, Math.min(60, jobWaitSeconds));
    info(`stackId=${stackId}`);

    section("CHECK");
    const checkResp = await triggerCheckAll(baseUrl, apiHeaders);
    const checkId = checkResp.ok ? checkResp.checkId : checkResp.existingJobId;
    assert(!!checkId, `failed to start check(all): ${JSON.stringify(checkResp)}`);
    info(`checkId=${checkId}`);
    await waitForJob(baseUrl, checkId!, apiHeaders, jobWaitSeconds);

    const stackResp = await jsonRequest(baseUrl, { method: "GET", path: `/api/stacks/${encodeURIComponent(stackId)}`, headers: apiHeaders });
    assert(stackResp.status === 200, `GET /api/stacks/${stackId} failed status=${stackResp.status}`);
    const services: any[] = Array.isArray(stackResp.data?.stack?.services) ? stackResp.data.stack.services : [];
    assert(services.length >= 2, `expected >=2 services in fixture stack, got ${services.length}`);
    const serviceIds = services
      .filter((svc: any) => typeof svc?.id === "string" && typeof svc?.image?.digest === "string" && svc.image.digest)
      .map((svc: any) => svc.id as string);
    assert(serviceIds.length >= 2, `expected >=2 valid service ids with digest, got ${serviceIds.length}`);
    info(`serviceIds=${serviceIds.slice(0, 4).join(",")}`);

    // get_stack() may enqueue cache-miss tasks. Wait until idle to avoid "reason=running" refresh responses.
    await waitForWorkerIdle(baseUrl, apiHeaders, Math.max(180, jobWaitSeconds * 3));

    section("CASE A: live SSE receives task_enqueued + monotonic ids");
    streamA = await SseStream.open(`${baseUrl}/api/version-inference/events`, apiHeaders);
    let refreshA: { serviceId: string; imageRepo: string; reason: string; status: string } | null = null;
    let serviceA = "";
    for (const candidate of serviceIds) {
      const next = await triggerVersionInferenceRefresh(baseUrl, candidate, apiHeaders);
      if (!next) continue;
      if (next.reason === "force") {
        refreshA = next;
        serviceA = candidate;
        break;
      }
    }
    assert(!!refreshA, "failed to trigger a force refresh for case A");
    info(`refreshA imageRepo=${refreshA.imageRepo} reason=${refreshA.reason}`);

    const enqueuedA = await waitForVersionInferenceEvent(
      streamA,
      sseWaitMs,
      (_evt, payload) => payload?.type === "task_enqueued" && payload?.imageRepo === refreshA.imageRepo,
    );
    assert(enqueuedA.evt.id != null, "task_enqueued event id should exist");
    const lastIdA = enqueuedA.evt.id!;
    info(`task_enqueued id=${lastIdA}`);

    const lifecycleA = await waitForVersionInferenceEvent(
      streamA,
      sseWaitMs,
      (_evt, payload) =>
        payload?.imageRepo === refreshA.imageRepo &&
        (payload?.type === "task_started" || payload?.type === "task_progress" || payload?.type === "task_finished"),
    );
    assert((lifecycleA.evt.id || 0) >= lastIdA, `expected lifecycle event id >= ${lastIdA}`);
    info(`lifecycle type=${lifecycleA.payload.type} id=${lifecycleA.evt.id}`);

    const afterId = lifecycleA.evt.id || lastIdA;
    streamA.close();
    streamA = null;

    section("CASE B: reconnect with afterId receives newer events");
    streamB = await SseStream.open(`${baseUrl}/api/version-inference/events?afterId=${afterId}`, apiHeaders);
    let refreshB: { serviceId: string; imageRepo: string; reason: string; status: string } | null = null;
    for (const candidate of serviceIds) {
      if (candidate === serviceA) continue;
      const next = await triggerVersionInferenceRefresh(baseUrl, candidate, apiHeaders);
      if (!next) continue;
      if (next.reason === "force") {
        refreshB = next;
        break;
      }
    }
    assert(!!refreshB, "failed to trigger a force refresh for case B");
    info(`refreshB imageRepo=${refreshB.imageRepo} reason=${refreshB.reason}`);

    const enqueuedB = await waitForVersionInferenceEvent(
      streamB,
      sseWaitMs,
      (evt, payload) =>
        (evt.id || 0) > afterId && payload?.type === "task_enqueued" && payload?.imageRepo === refreshB.imageRepo,
    );
    assert((enqueuedB.evt.id || 0) > afterId, `expected resumed stream id > ${afterId}`);
    info(`resumed task_enqueued id=${enqueuedB.evt.id}`);

    section("CASE C: overview endpoint reflects worker/rows");
    const overview = await jsonRequest(baseUrl, { method: "GET", path: "/api/version-inference/overview?page=1&perPage=50", headers: apiHeaders });
    assert(overview.status === 200, `GET /api/version-inference/overview failed: status=${overview.status}`);
    const rows = Array.isArray(overview.data?.rows) ? overview.data.rows : [];
    assert(rows.some((r: any) => r?.imageRepo === refreshB.imageRepo), `overview rows missing imageRepo=${refreshB.imageRepo}`);
    assert(typeof overview.data?.worker?.inFlight === "number", "overview.worker.inFlight missing");

    section("RESULT");
    info("PASS");
    success = true;
    streamB?.close();
    cleanupForward();
    await cleanupRemote(success);
  } catch (e) {
    streamA?.close();
    streamB?.close();
    cleanupForward();
    await cleanupRemote(false);
    throw e;
  }
}

main().catch((e) => {
  console.error(`\n[RESULT]\nFAIL: ${(e as Error).message || e}`);
  process.exit(1);
});
