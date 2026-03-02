#!/usr/bin/env bun
/**
 * codex-testbox regression: service update should not perform semver tag pull fallback.
 *
 * This script runs Dockrev + a dedicated fixture compose project on `codex-testbox` in an isolated
 * remote workspace, then validates:
 *  - `scope=service` update can be triggered with explicit `targetTag + targetDigest`
 *  - update job logs do not contain fallback pulls like `docker pull <repo>:<semver>`
 *  - update summary does not report `failureStep=semver_pull`
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

async function listStacks(baseUrl: string, headers: Record<string, string>) {
  const r = await jsonRequest(baseUrl, { method: "GET", path: "/api/stacks", headers });
  if (r.status !== 200) {
    throw new Error(`GET /api/stacks failed: status=${r.status} body=${JSON.stringify(r.data)}`);
  }
  return r.data?.stacks || [];
}

async function getStack(baseUrl: string, stackId: string, headers: Record<string, string>) {
  const r = await jsonRequest(baseUrl, {
    method: "GET",
    path: `/api/stacks/${encodeURIComponent(stackId)}`,
    headers,
  });
  if (r.status !== 200) {
    throw new Error(`GET /api/stacks/${stackId} failed: status=${r.status} body=${JSON.stringify(r.data)}`);
  }
  return r.data?.stack;
}

async function triggerServiceUpdate(
  baseUrl: string,
  headers: Record<string, string>,
  input: { serviceId: string; targetTag: string; targetDigest: string },
) {
  const r = await jsonRequest(baseUrl, {
    method: "POST",
    path: "/api/updates",
    headers,
    body: {
      scope: "service",
      serviceId: input.serviceId,
      targetTag: input.targetTag,
      targetDigest: input.targetDigest,
      mode: "apply",
      allowArchMismatch: false,
      backupMode: "inherit",
      reason: "ui",
    },
  });
  if (r.status !== 200) {
    throw new Error(`POST /api/updates failed: status=${r.status} body=${JSON.stringify(r.data)}`);
  }
  const jobId = r.data?.jobId || r.data?.job_id;
  if (!jobId) throw new Error(`update response missing jobId: ${JSON.stringify(r.data)}`);
  return jobId as string;
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
docker compose -p ${bashQuote(fixturesProject)} -f scripts/testbox/fixtures.semver-missing.yml -f .codex.caps-compat.fixtures.yml down -v --remove-orphans || true
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

# Dedicated fixture for this regression:
# - image tag is latest
# - old code would derive semver label (e.g. 0.13.3) and try pulling :<semver> after update.
# Force a local, mismatched starting digest so service update actually performs a pull+recreate.
docker pull nginx:1.27-alpine >/dev/null
docker tag nginx:1.27-alpine ghcr.io/ivanli-cn/codex-vibe-monitor:latest

services_fixtures="$(docker compose -f scripts/testbox/fixtures.semver-missing.yml config --services)"
gen_caps "$services_fixtures" .codex.caps-compat.fixtures.yml

docker compose -p ${bashQuote(fixturesProject)} \\
  -f scripts/testbox/fixtures.semver-missing.yml \\
  -f .codex.caps-compat.fixtures.yml \\
  up -d

docker compose -p ${bashQuote(fixturesProject)} \\
  -f scripts/testbox/fixtures.semver-missing.yml \\
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

    section("CASE: service update skips semver fallback pull");
    checkDeadline();

    // 1) discovery
    const discoveryJobId = await triggerDiscovery(baseUrl, apiHeaders);
    info(`discoveryJobId=${discoveryJobId}`);
    await waitForJob(baseUrl, discoveryJobId, apiHeaders, (j) => !!j?.finishedAt, jobWaitSeconds);

    // 2) check to persist latest candidate digest
    const check = await triggerCheckAll(baseUrl, apiHeaders);
    assert(check.ok, `expected check(all) to start, got: ${JSON.stringify(check)}`);
    info(`checkId=${check.checkId}`);
    await waitForJob(baseUrl, check.checkId, apiHeaders, (j) => !!j?.finishedAt, jobWaitSeconds);

    // 3) locate the fixture service
    const stacks = await listStacks(baseUrl, apiHeaders);
    info(`discoveredStacks=${stacks.length}`);
    let stackId = "";
    let svc: any = null;
    for (const s of stacks) {
      if (!s?.id) continue;
      const detail = await getStack(baseUrl, String(s.id), apiHeaders);
      const found = (detail?.services || []).find((x: any) => {
        if ((x?.name || "").toString() === "semvercase") return true;
        const ref = (x?.image?.ref || x?.image?.reference || "").toString();
        return ref.includes("ghcr.io/ivanli-cn/codex-vibe-monitor");
      });
      if (found) {
        stackId = String(s.id);
        svc = found;
        break;
      }
    }

    assert(!!svc, "target fixture service not found in discovered stacks");
    info(`stackId=${stackId}`);

    assert(!!svc, "target service not found in stack detail");

    const serviceId = svc.id as string;
    const targetTag = (svc?.image?.tag || "").toString();
    const targetDigest = (svc?.candidate?.digest || "").toString();
    info(`serviceId=${serviceId}`);
    info(`targetTag=${targetTag}`);
    info(`targetDigest=${targetDigest}`);

    assert(targetTag.length > 0, "service image tag is empty");
    assert(targetDigest.startsWith("sha256:"), "candidate digest missing after check");

    // 4) apply service update with explicit target lock
    const updateJobId = await triggerServiceUpdate(baseUrl, apiHeaders, {
      serviceId,
      targetTag,
      targetDigest,
    });
    info(`updateJobId=${updateJobId}`);

    const updateJob = await waitForJob(
      baseUrl,
      updateJobId,
      apiHeaders,
      (j) => !!j?.finishedAt,
      Math.max(jobWaitSeconds, 120),
    );

    info(`updateJob.status=${updateJob?.status}`);
    assert(updateJob?.status === "success", `expected update success, got: ${JSON.stringify(updateJob?.status)}`);

    const logs: any[] = Array.isArray(updateJob?.logs) ? updateJob.logs : [];
    const semverPullLog = logs.find((l) => {
      const msg = typeof l?.msg === "string" ? l.msg : "";
      return /\$ docker pull ghcr\.io\/ivanli-cn\/codex-vibe-monitor:(?!latest\b)[^\s]+/.test(msg);
    });
    assert(!semverPullLog, `unexpected semver fallback pull log found: ${JSON.stringify(semverPullLog)}`);

    const summaryDump = JSON.stringify(updateJob?.summary || {});
    assert(!summaryDump.includes("semver_pull"), `summary unexpectedly references semver_pull: ${summaryDump}`);

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
