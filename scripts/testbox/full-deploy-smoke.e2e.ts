#!/usr/bin/env bun
/**
 * codex-testbox regression: full deploy smoke using deploy/docker-compose.yml
 *
 * This script stages a minimal remote bundle under /srv/codex/**, copies prebuilt linux/amd64
 * binaries from dist/ci/docker/amd64, renders a testbox-specific deploy compose file with a
 * unique host port and compose project, and then validates the real gateway topology:
 *   - GET / returns HTML
 *   - GET /api/health returns 200 + ok
 *   - GET /supervisor/ returns HTML
 *
 * Design notes:
 *  - No local Docker dependency; the script expects prebuilt artifacts to already exist.
 *  - No source docker build on codex-testbox: the remote bundle contains only deploy/ assets +
 *    dist/ci/docker/amd64 prebuilt binaries, then bind-mounts those binaries into prebuilt images.
 *  - All remote writes stay under /srv/codex/** and cleanup only removes this run's resources.
 */
import { createHash } from "crypto";
import fs from "fs";
import net from "net";
import os from "os";
import path from "path";

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
  const iso = new Date().toISOString();
  return `${iso.slice(0, 4)}${iso.slice(5, 7)}${iso.slice(8, 10)}_${iso.slice(11, 13)}${iso.slice(14, 16)}${iso.slice(17, 19)}Z_${gitSha || "nogit"}`;
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
  return `'${s.replace(/'/g, `'"'"'`)}'`;
}

async function ssh(host: string, sshOpts: string[], script: string, allowFailure?: boolean) {
  return await run(["ssh", ...sshOpts, host, "bash", "-s", "--"], { stdin: script, allowFailure });
}

async function rsyncPath(host: string, sshOpts: string[], source: string, remoteDest: string, extraArgs: string[] = []) {
  const sshCmd = ["ssh", ...sshOpts].join(" ");
  const args = ["rsync", "-az", ...extraArgs, "-e", sshCmd, source, `${host}:${remoteDest}`];
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
      s.listen(p, "127.0.0.1", () => s.close(() => resolve(true)));
    });
    if (ok) return p;
  }
  throw new Error("failed to find a free local port");
}

async function probeFreeRemotePort(host: string, sshOpts: string[], start: number): Promise<string> {
  const probe = await ssh(
    host,
    sshOpts,
    `
set -euo pipefail
PY=python3
command -v "$PY" >/dev/null 2>&1 || PY=python
"$PY" - <<'PY'
import socket
start = ${start}
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
  const port = probe.stdout.trim();
  assert(port.length > 0, "failed to probe a free remote port");
  return port;
}

async function waitForText(
  url: string,
  timeoutSeconds: number,
  predicate: (status: number, text: string, headers: Headers) => boolean,
  label: string,
) {
  const deadline = Date.now() + timeoutSeconds * 1000;
  let lastStatus = 0;
  let lastText = "";
  while (Date.now() < deadline) {
    try {
      const resp = await fetch(url);
      const text = await resp.text();
      lastStatus = resp.status;
      lastText = text;
      if (predicate(resp.status, text, resp.headers)) {
        return { status: resp.status, text, headers: resp.headers };
      }
    } catch {
      // ignore while the deploy is starting
    }
    await Bun.sleep(500);
  }
  throw new Error(`timeout waiting for ${label}; last_status=${lastStatus} last_body=${JSON.stringify(lastText.slice(0, 240))}`);
}

function resolveArtifactPath(repoRoot: string, value: string): string {
  return path.isAbsolute(value) ? value : path.join(repoRoot, value);
}

function renderDeployOverride(opts: {
  dockrevBaseImage: string;
  supervisorBaseImage: string;
  versionLabel: string;
}) {
  return `services:
  dockrev:
    image: ${opts.dockrevBaseImage}
    environment:
      APP_EFFECTIVE_VERSION: "${opts.versionLabel}"
      DOCKREV_IMAGE_REPO: "ghcr.io/ivanli-cn/dockrev"
    volumes:
      - ../dist/ci/docker/amd64/dockrev:/usr/local/bin/dockrev:ro

  supervisor:
    image: ${opts.supervisorBaseImage}
    environment:
      APP_EFFECTIVE_VERSION: "${opts.versionLabel}"
      DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO: "ghcr.io/ivanli-cn/dockrev"
    volumes:
      - ../dist/ci/docker/amd64/dockrev-supervisor:/usr/local/bin/dockrev-supervisor:ro
`;
}

async function main() {
  const testboxHost = env("TESTBOX_HOST", "codex-testbox")!;
  const sshOpts = (env("TESTBOX_SSH_OPTS", "-o BatchMode=yes") || "")
    .split(" ")
    .map((s) => s.trim())
    .filter(Boolean);

  const keepOnFailure = envBool("DOCKREV_TEST_KEEP", false);
  const keepOnSuccess = envBool("DOCKREV_TEST_KEEP_SUCCESS", false);
  const waitSeconds = envInt("DOCKREV_DEPLOY_WAIT_SECONDS", 120);
  const localForwardStart = envInt("LOCAL_HTTP_PORT", 56000);
  const prebuiltDir = env("DOCKREV_PREBUILT_DIR", "dist/ci/docker/amd64")!;
  const dockrevBaseImage = env("DOCKREV_BASE_IMAGE", "ghcr.io/ivanli-cn/dockrev:latest")!;
  const supervisorBaseImage = env("DOCKREV_SUPERVISOR_BASE_IMAGE", "ghcr.io/ivanli-cn/dockrev-supervisor:latest")!;

  section("SETUP");
  const repoRoot = (await run(["git", "rev-parse", "--show-toplevel"])).stdout.trim();
  const repoRootReal = fs.realpathSync(repoRoot);
  const repoName = path.basename(repoRootReal);
  const gitSha =
    (await run(["git", "-C", repoRootReal, "rev-parse", "--short", "HEAD"], { allowFailure: true })).stdout.trim() ||
    "nogit";
  const pathHash8 = createHash("sha256").update(repoRootReal).digest("hex").slice(0, 8);
  const runId = nowRunId(gitSha);
  const versionLabel = `0.0.0-testbox-${gitSha}`;

  const artifactRoot = resolveArtifactPath(repoRootReal, prebuiltDir);
  const dockrevBin = path.join(artifactRoot, "dockrev");
  const supervisorBin = path.join(artifactRoot, "dockrev-supervisor");
  if (!fs.existsSync(dockrevBin) || !fs.existsSync(supervisorBin)) {
    throw new Error(
      [
        `missing prebuilt linux/amd64 artifacts under ${artifactRoot}`,
        "expected files:",
        `  - ${dockrevBin}`,
        `  - ${supervisorBin}`,
        "build them first, for example:",
        "  cargo zigbuild -p dockrev-api --bin dockrev --release --locked --target x86_64-unknown-linux-musl",
        "  cargo zigbuild -p dockrev-supervisor --bin dockrev-supervisor --release --locked --target x86_64-unknown-linux-musl",
        `  mkdir -p ${prebuiltDir}`,
        `  cp target/x86_64-unknown-linux-musl/release/dockrev ${prebuiltDir}/dockrev`,
        `  cp target/x86_64-unknown-linux-musl/release/dockrev-supervisor ${prebuiltDir}/dockrev-supervisor`,
      ].join("\n"),
    );
  }

  const remoteUser = (await ssh(testboxHost, sshOpts, "whoami")).stdout.trim();
  assert(remoteUser.length > 0, "remote user is empty");
  const remoteWorkspace = `/srv/codex/workspaces/${remoteUser}/${repoName}__${pathHash8}`;
  const remoteRun = `${remoteWorkspace}/runs/${runId}`;
  const composeProject = sanitizeComposeProject(`codex_${repoName}__${pathHash8}_${runId}`);
  const remoteGatewayPort = env("REMOTE_HTTP_PORT") || (await probeFreeRemotePort(testboxHost, sshOpts, 56000));
  const localPort = await findFreeLocalPort(localForwardStart);
  const gatewayBind = `127.0.0.1:${remoteGatewayPort}:80`;

  info(`repoRoot=${repoRootReal}`);
  info(`PREBUILT_DIR=${artifactRoot}`);
  info(`remoteUser=${remoteUser}`);
  info(`REMOTE_RUN=${remoteRun}`);
  info(`COMPOSE_PROJECT=${composeProject}`);
  info(`REMOTE_GATEWAY_PORT=${remoteGatewayPort}`);
  info(`DOCKREV_GATEWAY_BIND=${gatewayBind}`);
  info(`LOCAL_FORWARD_PORT=${localPort}`);
  info(`DOCKREV_BASE_IMAGE=${dockrevBaseImage}`);
  info(`DOCKREV_SUPERVISOR_BASE_IMAGE=${supervisorBaseImage}`);

  const stageDir = fs.mkdtempSync(path.join(os.tmpdir(), "dockrev-testbox-full-deploy-"));
  const stageDeployDir = path.join(stageDir, "deploy");
  const stageArtifactDir = path.join(stageDir, "dist", "ci", "docker", "amd64");
  fs.mkdirSync(stageDeployDir, { recursive: true });
  fs.mkdirSync(path.join(stageDeployDir, "data"), { recursive: true });
  fs.mkdirSync(stageArtifactDir, { recursive: true });

  fs.copyFileSync(path.join(repoRootReal, "deploy", "docker-compose.yml"), path.join(stageDeployDir, "docker-compose.yml"));
  fs.copyFileSync(path.join(repoRootReal, "deploy", "nginx.conf"), path.join(stageDeployDir, "nginx.conf"));
  fs.copyFileSync(dockrevBin, path.join(stageArtifactDir, "dockrev"));
  fs.copyFileSync(supervisorBin, path.join(stageArtifactDir, "dockrev-supervisor"));
  fs.writeFileSync(
    path.join(stageDeployDir, "docker-compose.testbox.override.yml"),
    renderDeployOverride({
      dockrevBaseImage,
      supervisorBaseImage,
      versionLabel,
    }),
  );

  let remoteRunReady = false;
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

  const cleanupRemote = async (ok: boolean) => {
    if (!remoteRunReady) return;
    if ((ok && keepOnSuccess) || (!ok && keepOnFailure)) {
      section("CLEANUP");
      info(ok ? "DOCKREV_TEST_KEEP_SUCCESS=1 set; keeping remote run for verification." : "DOCKREV_TEST_KEEP=1 set; keeping remote run for debugging.");
      info(`REMOTE_RUN=${remoteRun}`);
      info(`COMPOSE_PROJECT=${composeProject}`);
      return;
    }
    section("CLEANUP");
    await ssh(
      testboxHost,
      sshOpts,
      `
set -euo pipefail
cd ${bashQuote(remoteRun)}
export DOCKREV_GATEWAY_BIND=${bashQuote(gatewayBind)}
docker compose -p ${bashQuote(composeProject)} -f deploy/docker-compose.yml -f deploy/docker-compose.testbox.override.yml -f deploy/.codex.caps-compat.deploy.yml down -v --remove-orphans || true
rm -rf ${bashQuote(remoteRun)} || true
`,
      true,
    );
  };

  try {
    const createdUtc = new Date().toISOString();
    await ssh(
      testboxHost,
      sshOpts,
      `mkdir -p ${bashQuote(remoteRun)} && mkdir -p ${bashQuote(remoteWorkspace)} && cat > ${bashQuote(`${remoteWorkspace}/workspace.txt`)} <<'TXT'\nlocal_repo_root=${repoRootReal}\ncreated_utc=${createdUtc}\nTXT`,
    );
    remoteRunReady = true;

    await rsyncPath(testboxHost, sshOpts, `${stageDir.replace(/\/$/, "")}/`, `${remoteRun.replace(/\/$/, "")}/`, ["--delete"]);

    section("REMOTE DEPLOY");
    await ssh(
      testboxHost,
      sshOpts,
      `
set -euo pipefail
cd ${bashQuote(remoteRun)}

if [[ ! -f deploy/docker-compose.yml ]]; then
  echo "missing deploy/docker-compose.yml in remote bundle" >&2
  exit 2
fi
if [[ ! -f deploy/docker-compose.testbox.override.yml ]]; then
  echo "missing deploy/docker-compose.testbox.override.yml in remote bundle" >&2
  exit 2
fi
if [[ ! -s dist/ci/docker/amd64/dockrev || ! -s dist/ci/docker/amd64/dockrev-supervisor ]]; then
  echo "missing dist/ci/docker/amd64 prebuilt artifacts in remote bundle" >&2
  exit 2
fi
chmod 0755 dist/ci/docker/amd64/dockrev dist/ci/docker/amd64/dockrev-supervisor

export DOCKREV_GATEWAY_BIND=${bashQuote(gatewayBind)}

services=$(docker compose -f deploy/docker-compose.yml -f deploy/docker-compose.testbox.override.yml config --services)
{
  echo 'services:'
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
} > deploy/.codex.caps-compat.deploy.yml

pull_attempt=0
until docker compose -p ${bashQuote(composeProject)} -f deploy/docker-compose.yml -f deploy/docker-compose.testbox.override.yml -f deploy/.codex.caps-compat.deploy.yml pull gateway dockrev supervisor; do
  pull_attempt=$((pull_attempt + 1))
  if (( pull_attempt >= 5 )); then
    echo "docker compose pull failed after \${pull_attempt} attempts" >&2
    exit 1
  fi
  sleep 5
  echo "retrying docker compose pull (\${pull_attempt}/5)" >&2
done

docker compose -p ${bashQuote(composeProject)} -f deploy/docker-compose.yml -f deploy/docker-compose.testbox.override.yml -f deploy/.codex.caps-compat.deploy.yml up -d --no-build
docker compose -p ${bashQuote(composeProject)} -f deploy/docker-compose.yml -f deploy/docker-compose.testbox.override.yml -f deploy/.codex.caps-compat.deploy.yml ps
`,
    );

    section("PORT FORWARD");
    const forwardArgs = [
      "ssh",
      ...sshOpts,
      "-o",
      "ExitOnForwardFailure=yes",
      "-N",
      "-L",
      `${localPort}:127.0.0.1:${remoteGatewayPort}`,
      testboxHost,
    ];
    info(`localPortForward=127.0.0.1:${localPort} -> ${testboxHost}:127.0.0.1:${remoteGatewayPort}`);
    forwardProc = Bun.spawn(forwardArgs, { stdout: "pipe", stderr: "pipe" });

    const baseUrl = `http://127.0.0.1:${localPort}`;

    section("ASSERTIONS");
    const health = await waitForText(
      `${baseUrl}/api/health`,
      waitSeconds,
      (status, text) => status === 200 && text.trim() === "ok",
      "/api/health",
    );
    info(`GET /api/health => ${health.status} ${health.text.trim()}`);

    const root = await waitForText(
      `${baseUrl}/`,
      waitSeconds,
      (status, text, headers) => status === 200 && headers.get("content-type")?.includes("text/html") === true && /<html/i.test(text),
      "/",
    );
    info(`GET / => ${root.status} content-type=${root.headers.get("content-type") || "unknown"}`);

    const supervisor = await waitForText(
      `${baseUrl}/supervisor/`,
      waitSeconds,
      (status, text, headers) => status === 200 && headers.get("content-type")?.includes("text/html") === true && /<html/i.test(text),
      "/supervisor/",
    );
    info(`GET /supervisor/ => ${supervisor.status} content-type=${supervisor.headers.get("content-type") || "unknown"}`);

    section("RESULT");
    info("PASS");

    cleanupForward();
    await cleanupRemote(true);
    fs.rmSync(stageDir, { recursive: true, force: true });
  } catch (e) {
    cleanupForward();
    await cleanupRemote(false);
    fs.rmSync(stageDir, { recursive: true, force: true });
    throw e;
  }
}

main().catch((e) => {
  console.error(`\n[RESULT]\nFAIL: ${(e as Error).message || e}`);
  process.exit(1);
});
