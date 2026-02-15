#!/usr/bin/env bun
/**
 * codex-testbox regression: check job concurrency guard + restart recovery
 *
 * This script runs Dockrev + a fixture compose project on `codex-testbox` in an isolated
 * remote workspace, then validates:
 *  - parallel `POST /api/checks` returns 409 conflict
 *  - restarting dockrev terminates orphaned jobs (failed + summary.terminated + audit log)
 *
 * Design notes:
 *  - No npm deps; only uses Bun + system tools (ssh, rsync, git).
  *  - Keeps all remote writes under /srv/codex/**.
 *  - The shared testbox runs Docker inside LXC where CAP_SETFCAP is not available, which can
 *    break docker builds. For reliability, Dockrev is built+run as a host process on the testbox
 *    (via cargo), while Docker is used only for the fixture compose project.
 */
import { createHash } from "crypto";
import fs from "fs";
import net from "net";
import path from "path";

type JsonValue = any;

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
  // YYYYMMDD_HHMMSSZ
  const iso = d.toISOString(); // 2026-02-15T05:01:03.000Z
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
    const w = p.stdin.getWriter();
    await w.write(new TextEncoder().encode(opts.stdin));
    await w.close();
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
  // Wrap in single quotes, escaping internal single quotes: ' -> '"'"'
  return `'${s.replace(/'/g, `'\"'\"'`)}'`;
}

async function ssh(host: string, sshOpts: string[], script: string, allowFailure?: boolean) {
  const cmd = ["ssh", ...sshOpts, host, "bash", "-lc", script];
  return await run(cmd, { allowFailure });
}

async function rsyncToRemote(host: string, sshOpts: string[], srcDir: string, remoteDir: string) {
  // Convert ssh opts to an ssh command for rsync -e.
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

async function waitForJob(
  baseUrl: string,
  jobId: string,
  headers: Record<string, string>,
  predicate: (job: any) => boolean,
  timeoutSeconds: number,
) {
  const deadline = Date.now() + timeoutSeconds * 1000;
  let last: any = null;
  while (Date.now() < deadline) {
    const r = await jsonRequest(baseUrl, { method: "GET", path: `/api/jobs/${encodeURIComponent(jobId)}`, headers });
    if (r.status !== 200) {
      throw new Error(`GET /api/jobs/${jobId} failed: status=${r.status} body=${JSON.stringify(r.data)}`);
    }
    const job = r.data?.job;
    last = job;
    if (predicate(job)) return job;
    await sleep(800);
  }
  throw new Error(`timeout waiting for job ${jobId}; last=${JSON.stringify(last)}`);
}

async function listJobs(baseUrl: string, headers: Record<string, string>) {
  const r = await jsonRequest(baseUrl, { method: "GET", path: "/api/jobs", headers });
  if (r.status !== 200) throw new Error(`GET /api/jobs failed: status=${r.status} body=${JSON.stringify(r.data)}`);
  return r.data?.jobs || [];
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
    const code = r.data?.error?.code;
    const existingJobId = r.data?.error?.details?.existingJobId;
    return { ok: false as const, status: 409, code, existingJobId, body: r.data };
  }
  return { ok: false as const, status: r.status, body: r.data };
}

async function main() {
  const testboxHost = env("TESTBOX_HOST", "codex-testbox")!;
  const sshOpts = (env("TESTBOX_SSH_OPTS", "-o BatchMode=yes") || "")
    .split(" ")
    .map((s) => s.trim())
    .filter(Boolean);

  const keepOnFailure = envBool("DOCKREV_TEST_KEEP", false);
  const buildTimeoutSeconds = envInt("DOCKREV_BUILD_TIMEOUT_SECONDS", 900);
  // This timeout is for the test portion (excluding build).
  const overallTimeoutSeconds = envInt("DOCKREV_TEST_TIMEOUT_SECONDS", 180);
  const jobWaitSeconds = envInt("DOCKREV_JOB_WAIT_SECONDS", 60);
  const restartGraceSeconds = envInt("DOCKREV_RESTART_GRACE_SECONDS", 1);
  const restartMode = (env("DOCKREV_RESTART_MODE", "hard") || "hard").toLowerCase();

  const authHeaderName = env("DOCKREV_AUTH_HEADER_NAME", "X-Forwarded-User")!;
  const authHeaderValue = env("DOCKREV_AUTH_HEADER_VALUE", "test")!;
  const apiHeaders = { [authHeaderName]: authHeaderValue };

  // Overall timeout applies to the *test portion* (after remote build completed).
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
  const runId = nowRunId(gitSha);

  const remoteUser = (await ssh(testboxHost, sshOpts, "whoami")).stdout.trim();
  assert(remoteUser.length > 0, "remote user is empty");
  const remoteWorkspace = `/srv/codex/workspaces/${remoteUser}/${repoName}__${pathHash8}`;
  const remoteRun = `${remoteWorkspace}/runs/${runId}`;

  const composeProjectRaw = `codex_${repoName}__${pathHash8}_${runId}`;
  const composeProject = sanitizeComposeProject(composeProjectRaw);
  const fixturesProject = sanitizeComposeProject(`${composeProject}_fixtures`);

  // Select remote port (avoid conflicts on shared host).
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
START=55000
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

  // Ensure remote dirs exist and attach a small metadata file.
  // We'll try to clean up even if setup fails.
  let remoteRunReady = false;
  const cleanupRemote = async (ok: boolean) => {
    // Clean up remote resources unless we keep on failure.
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

    // Sync repo to remote run dir.
    await rsyncToRemote(testboxHost, sshOpts, repoRootReal, remoteRun);

    // Start fixture compose project (caps compat).
    checkDeadline();
    await ssh(
      testboxHost,
      sshOpts,
      `
set -euo pipefail
cd ${bashQuote(remoteRun)}

# LXC quirk: CAP_SETFCAP is not available. Drop all and add back a known-good set.
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

docker compose -p ${bashQuote(fixturesProject)} \\
  -f scripts/testbox/fixtures.compose.yml \\
  -f .codex.caps-compat.fixtures.yml \\
  ps
`,
    );

    // Build Dockrev binary on the testbox in a container (LXC quirk: host lacks pkg-config/openssl dev).
    // Keep caches under /srv/codex/** and run the container as the remote user to avoid root-owned outputs.
    section("BUILD (remote)");
    const buildStarted = Date.now();
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

echo "[build] building dockrev-api (release) via rust:1.91-bookworm..."
docker run --rm \\
  --user "$uid:$gid" \\
  "\${caps[@]}" \\
  -e CARGO_HOME \\
  -e CARGO_TARGET_DIR \\
  -v ${bashQuote(remoteWorkspace)}:${bashQuote(remoteWorkspace)} \\
  -w ${bashQuote(remoteRun)} \\
  rust:1.91-bookworm \\
  bash -c ${bashQuote("set -euo pipefail; export PATH=/usr/local/cargo/bin:$PATH; cargo build -p dockrev-api --bin dockrev --release --locked")}

ls -la "$CARGO_TARGET_DIR/release/dockrev"
`,
    );
    const buildDuration = Math.round((Date.now() - buildStarted) / 1000);
    if (buildDuration > buildTimeoutSeconds) {
      throw new Error(`remote build exceeded timeout: duration=${buildDuration}s timeout=${buildTimeoutSeconds}s`);
    }

    // Start Dockrev as a host process (bind to remote loopback).
    checkDeadline();
    section("START (remote)");
    await ssh(
      testboxHost,
      sshOpts,
      `
set -euo pipefail
cd ${bashQuote(remoteRun)}

export CARGO_TARGET_DIR=${bashQuote(`${remoteWorkspace}/target/testbox`)}
bin="$CARGO_TARGET_DIR/release/dockrev"

rm -f dockrev.pid dockrev.log
touch dockrev.log

export DOCKREV_HTTP_ADDR="127.0.0.1:${remoteHttpPort}"
export DOCKREV_DB_PATH=${bashQuote(`${remoteRun}/data/dockrev.sqlite3`)}
export DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV="true"

echo "[start] $bin (addr=$DOCKREV_HTTP_ADDR)" | tee -a dockrev.log
nohup "$bin" >> dockrev.log 2>&1 &
echo "$!" > dockrev.pid
`,
    );

    // Start local port-forward to the remote dockrev process (remote binds to 127.0.0.1 only).
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

    section("CASE A: 409 conflict on parallel checks");
    checkDeadline();

    // Ensure discovery has a chance to register the fixture project.
    const discoveryJobId = await triggerDiscovery(baseUrl, apiHeaders);
    info(`discoveryJobId=${discoveryJobId}`);
    await waitForJob(baseUrl, discoveryJobId, apiHeaders, (j) => !!j?.finishedAt, jobWaitSeconds);

    let checkIdA: string | null = null;
    let caseAPassed = false;
    for (let attempt = 1; attempt <= 3; attempt++) {
      checkDeadline();
      info(`attempt=${attempt}`);
      const a = await triggerCheckAll(baseUrl, apiHeaders);
      assert(a.ok, `expected first check to start (200), got: ${JSON.stringify(a)}`);
      checkIdA = a.checkId;
      info(`checkIdA=${checkIdA}`);

      // Give the worker a tiny head start; if it's already finished, retry (too fast).
      await sleep(80);
      const jobA = await waitForJob(baseUrl, checkIdA, apiHeaders, (j) => j != null, 5);
      if (jobA?.finishedAt) {
        info("check finished too quickly; retrying to exercise concurrency guard");
        continue;
      }

      const b = await triggerCheckAll(baseUrl, apiHeaders);
      if (b.ok) {
        // If jobA finished between our last read and the second trigger, retry.
        const jobA2 = await waitForJob(baseUrl, checkIdA, apiHeaders, (j) => j != null, 5);
        if (jobA2?.finishedAt) {
          info("race: first check finished before second trigger; retrying");
          continue;
        }
        throw new Error(`expected 409 conflict, but got 200: checkId=${b.checkId}`);
      }

      assert(b.status === 409, `expected 409 conflict, got status=${(b as any).status}`);
      assert(b.code === "conflict", `expected error.code=conflict, got: ${JSON.stringify(b.body)}`);
      assert(!!b.existingJobId, `expected error.details.existingJobId, got: ${JSON.stringify(b.body)}`);
      info(`existingJobId=${b.existingJobId}`);

      // Sanity: ensure we don't have multiple running check(all) jobs.
      const jobs = await listJobs(baseUrl, apiHeaders);
      const runningChecks = jobs.filter((j: any) => j?.type === "check" && j?.scope === "all" && j?.status === "running");
      assert(runningChecks.length <= 1, `expected <=1 running check(all), got ${runningChecks.length}`);
      caseAPassed = true;
      break;
    }
    assert(caseAPassed, "failed to exercise concurrency guard after 3 attempts (checks may be too fast)");

    section("CASE B: restart terminates orphaned running check");
    checkDeadline();

    let checkIdB: string | null = null;
    let caseBStarted = false;
    for (let attempt = 1; attempt <= 3; attempt++) {
      checkDeadline();
      info(`attempt=${attempt}`);
      const a = await triggerCheckAll(baseUrl, apiHeaders);
      if (a.ok) {
        checkIdB = a.checkId;
        info(`checkIdB=${checkIdB}`);
      } else if (a.status === 409 && a.existingJobId) {
        checkIdB = a.existingJobId;
        info(`checkIdB(existing)=${checkIdB}`);
      } else {
        throw new Error(`unexpected response from POST /api/checks: ${JSON.stringify(a)}`);
      }
      await sleep(120);

      const jobB = await waitForJob(baseUrl, checkIdB, apiHeaders, (j) => j != null, 5);
      if (jobB?.finishedAt) {
        info("check finished too quickly before restart; retrying");
        continue;
      }

      caseBStarted = true;
      break;
    }
    assert(caseBStarted && !!checkIdB, "failed to start a long-enough check to test restart recovery");

    // Restart dockrev process on the remote host.
    await ssh(
      testboxHost,
      sshOpts,
      `
set -euo pipefail
cd ${bashQuote(remoteRun)}

pid="$(cat dockrev.pid || true)"
if [[ -z "$pid" ]]; then
  echo "missing dockrev.pid" >&2
  exit 2
fi

if [[ ${bashQuote(restartMode)} == "soft" ]]; then
  kill "$pid" || true
else
  kill -9 "$pid" || true
fi

export CARGO_TARGET_DIR=${bashQuote(`${remoteWorkspace}/target/testbox`)}
bin="$CARGO_TARGET_DIR/release/dockrev"

export DOCKREV_HTTP_ADDR="127.0.0.1:${remoteHttpPort}"
export DOCKREV_DB_PATH=${bashQuote(`${remoteRun}/data/dockrev.sqlite3`)}
export DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV="true"

nohup "$bin" >> dockrev.log 2>&1 &
echo "$!" > dockrev.pid
`,
    );
    if (restartGraceSeconds > 0) await sleep(restartGraceSeconds * 1000);

    await waitForHealth(baseUrl, Math.max(20, Math.min(60, jobWaitSeconds)));

    // Validate job is now terminal and annotated.
    const jobAfter = await waitForJob(
      baseUrl,
      checkIdB!,
      apiHeaders,
      (j) => !!j?.finishedAt,
      jobWaitSeconds,
    );
    info(`jobAfter.status=${jobAfter.status} finishedAt=${jobAfter.finishedAt}`);

    assert(jobAfter.status !== "running", "expected job to be terminal after restart");
    assert(jobAfter.status === "failed" || jobAfter.status === "success" || jobAfter.status === "rolled_back", `unexpected terminal status=${jobAfter.status}`);
    assert(jobAfter.status === "failed", `expected failed for terminated check, got status=${jobAfter.status}`);

    const terminated = jobAfter?.summary?.terminated;
    assert(terminated && typeof terminated === "object", "expected summary.terminated to exist");
    assert(
      terminated.reason === "server_shutdown" || terminated.reason === "server_restart",
      `expected terminated.reason to be server_shutdown/server_restart, got ${JSON.stringify(terminated)}`,
    );

    const logs: any[] = Array.isArray(jobAfter.logs) ? jobAfter.logs : [];
    const auditOk = logs.some((l) => typeof l?.msg === "string" && (l.msg.includes("job terminated:") || l.msg.includes("job recovered as terminated:")));
    assert(auditOk, "expected an audit log line about termination/recovery");

    // Ensure a new check can be started after restart recovery.
    const c = await triggerCheckAll(baseUrl, apiHeaders);
    assert(c.ok, `expected check to start after restart recovery, got: ${JSON.stringify(c)}`);
    assert(c.checkId !== checkIdB, "expected a new checkId after restart");

    section("RESULT");
    info("PASS");
    success = true;
    cleanupForward();
    await cleanupRemote(success);
  } catch (e) {
    cleanupForward();
    await cleanupRemote(false);
    throw e;
  }
}

main().catch((e) => {
  console.error(`\n[RESULT]\nFAIL: ${(e as Error).message || e}`);
  process.exit(1);
});
