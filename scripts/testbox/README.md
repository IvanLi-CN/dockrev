# codex-testbox E2E scripts

This folder contains manual regression scripts that run Dockrev on the shared test machine
`codex-testbox` (192.168.31.15) in an isolated per-run workspace.

Available scripts:

1) `full-deploy-smoke.e2e.ts`
   - stages a minimal remote bundle under `/srv/codex/**`
   - copies prebuilt `dist/ci/docker/amd64/{dockrev,dockrev-supervisor}` artifacts
   - brings up the real `deploy/docker-compose.yml` topology with a unique remote host port
   - verifies `GET /`, `GET /api/health`, and `GET /supervisor/`
2) `check-job-recovery.e2e.ts`
   - `POST /api/checks` blocks parallel runs for the same scope (`409 conflict`)
   - Dockrev restart terminates orphaned running jobs (`failed` + `summary.terminated`)
3) `check-version-inference-sse.e2e.ts`
   - `/api/version-inference/events` pushes `version_inference_event` with monotonic ids
   - `POST /api/services/{id}/version-inference/refresh` emits `task_enqueued`
   - reconnect with `afterId` receives newer events
4) `check-service-update-no-semver-pull.e2e.ts`
   - rewrites a run-scoped compose fixture that points at a concrete HTTPS image ref
   - locally retags that ref on `codex-testbox` to force a digest mismatch without doing a remote source build
   - verifies update job does not issue fallback `docker pull <repo>:<semver>`
   - verifies update summary does not include `failureStep=semver_pull`

These scripts can be executed from GitHub Actions via the manual workflow
`.github/workflows/e2e-testbox.yml`, but only on a **self-hosted Linux x64 runner** that already has
network access plus SSH trust to `codex-testbox`. They are still kept out of the default PR/main CI
because they depend on a shared host environment and can be flaky due to network / registry rate
limits.

Important: the shared testbox runs Docker inside LXC where `CAP_SETFCAP` is not available.

- The three existing host-process regressions prefer the prebuilt `dist/ci/docker/amd64/dockrev` binary when it exists; they only fall back to a remote build when that artifact is missing. Docker is used only for the fixture stacks they scan/check.
- `full-deploy-smoke.e2e.ts` does **not** do a source docker build on the testbox. It requires
  prebuilt linux/amd64 artifacts, stages the real `deploy/docker-compose.yml`, layers a small
  testbox-only override on top, bind-mounts the branch binaries into `/usr/local/bin`, and then
  brings up that merged topology with `docker compose up --no-build`.

## Prerequisites (local machine)

- `bun`
- `ssh` and `rsync`
- SSH access to `codex-testbox` (host alias: `codex-testbox`)
- For `full-deploy-smoke.e2e.ts`: prebuilt `dist/ci/docker/amd64/dockrev` and
  `dist/ci/docker/amd64/dockrev-supervisor`

## GitHub Actions (manual)

Workflow: `.github/workflows/e2e-testbox.yml`

Recommended trigger settings:

- `scenario=all` for a full regression sweep (`full-deploy-smoke` + all three host-process regressions)
- `repeat_count=2` when you want extra confidence against shared-host flakes
- `keep_remote_artifacts=true` only when you need post-failure inspection
- run it on a `self-hosted`, `linux`, `x64` runner that can already `ssh codex-testbox`

When the workflow needs `full-deploy-smoke`, it first builds linux/amd64 musl artifacts into
`dist/ci/docker/amd64/` on the runner, then launches the shared-testbox scenarios. The workflow
uploads `.artifacts/testbox-e2e/` as a GitHub Actions artifact so each run keeps the raw
per-scenario logs.

## Quickstart

From the repo root, run one of:

```bash
bun scripts/testbox/check-job-recovery.e2e.ts
bun scripts/testbox/check-version-inference-sse.e2e.ts
bun scripts/testbox/check-service-update-no-semver-pull.e2e.ts
```

For the real deploy smoke, build prebuilt linux/amd64 artifacts first:

```bash
cargo zigbuild -p dockrev-api --bin dockrev --release --locked --target x86_64-unknown-linux-musl
cargo zigbuild -p dockrev-supervisor --bin dockrev-supervisor --release --locked --target x86_64-unknown-linux-musl
mkdir -p dist/ci/docker/amd64
cp target/x86_64-unknown-linux-musl/release/dockrev dist/ci/docker/amd64/dockrev
cp target/x86_64-unknown-linux-musl/release/dockrev-supervisor dist/ci/docker/amd64/dockrev-supervisor
bun scripts/testbox/full-deploy-smoke.e2e.ts
```

Run twice to reduce false confidence:

```bash
bun scripts/testbox/full-deploy-smoke.e2e.ts
bun scripts/testbox/full-deploy-smoke.e2e.ts
bun scripts/testbox/check-job-recovery.e2e.ts
bun scripts/testbox/check-job-recovery.e2e.ts
bun scripts/testbox/check-version-inference-sse.e2e.ts
bun scripts/testbox/check-version-inference-sse.e2e.ts
bun scripts/testbox/check-service-update-no-semver-pull.e2e.ts
bun scripts/testbox/check-service-update-no-semver-pull.e2e.ts
```

## Environment variables

Shared across all scripts:

- `TESTBOX_HOST` (default: `codex-testbox`)
- `TESTBOX_SSH_OPTS` (default: `-o BatchMode=yes`)
- `REMOTE_HTTP_PORT` (optional): force the remote HTTP/gateway port; otherwise a free port is selected.
- `DOCKREV_TEST_KEEP` (default: `0`): set to `1` to keep remote artifacts on failure.
- `DOCKREV_TEST_KEEP_SUCCESS` (default: `0`): `full-deploy-smoke.e2e.ts` only; set to `1` to keep a successful remote deploy up for manual verification.
- `LOCAL_HTTP_PORT` (optional): preferred local SSH-forward start port.

`full-deploy-smoke.e2e.ts` specific:

- `DOCKREV_PREBUILT_DIR` (default: `dist/ci/docker/amd64`)
- `DOCKREV_BASE_IMAGE` (default: `ghcr.io/ivanli-cn/dockrev:latest`)
- `DOCKREV_SUPERVISOR_BASE_IMAGE` (default: `ghcr.io/ivanli-cn/dockrev-supervisor:latest`)
- `DOCKREV_DEPLOY_WAIT_SECONDS` (default: `120`)

Existing host-process regressions:

- `DOCKREV_AUTH_HEADER_NAME` (default: `X-Forwarded-User`)
- `DOCKREV_AUTH_HEADER_VALUE` (default: `test`)
- `DOCKREV_BUILD_TIMEOUT_SECONDS` (default: `900`): timeout for building the Dockrev binary on the testbox.
- `DOCKREV_TEST_TIMEOUT_SECONDS` (default: `180`): overall timeout for the test portion (excluding build).
- `DOCKREV_JOB_WAIT_SECONDS` (default: `60`): timeout for job state transitions.
- `DOCKREV_SSE_WAIT_MS` (default: `45000`): SSE event wait timeout (used by `check-version-inference-sse.e2e.ts`).
- `DOCKREV_RESTART_GRACE_SECONDS` (default: `1`): small sleep after restart.
- `DOCKREV_RESTART_MODE` (default: `hard`): `hard` uses SIGKILL to exercise startup recovery;
  `soft` uses SIGTERM to exercise graceful shutdown.
- `SEMVER_FIXTURE_IMAGE_REF` (default: `ghcr.io/ivanli-cn/dockrev:latest`): image repo used by `check-service-update-no-semver-pull.e2e.ts`; the script temporarily retags it locally on the testbox to force an explicit digest update path.

## What the scripts do (high level)

`full-deploy-smoke.e2e.ts`:

- Creates an isolated remote run directory under `/srv/codex/workspaces/$USER/.../runs/$RUN_ID`
- Rsyncs a **minimal** bundle (`deploy/`, `dist/ci/docker/amd64/`) instead of the full repo
- Stages the repo's `deploy/docker-compose.yml` plus a small testbox-only override with a unique host port and compose project
- Pulls prebuilt base images, bind-mounts the branch binaries, runs `docker compose up -d --no-build`, and validates the gateway over an SSH port-forward
- Cleans up only this run's containers/volumes/networks/remote directory (unless `DOCKREV_TEST_KEEP=1`)

Existing host-process regressions:

- Rsync the repo to a remote run directory (excluding large build artifacts)
- Stage the prebuilt `dockrev` binary when available, then run Dockrev on the testbox as a host process
- Start a fixture Compose project under a unique `<project>_fixtures` with caps dropped
- Open an SSH port-forward so the local script can call Dockrev via `http://127.0.0.1:<localPort>`
- Run script-specific assertions
- Clean up remote containers/volumes and the remote run directory (unless `DOCKREV_TEST_KEEP=1`)

## Troubleshooting

If a script fails and you want to inspect the remote run:

1) rerun with `DOCKREV_TEST_KEEP=1`
2) use the printed `REMOTE_RUN` and `COMPOSE_PROJECT` values to inspect:

```bash
ssh codex-testbox
cd "<REMOTE_RUN>"
docker compose -p "<COMPOSE_PROJECT>" -f deploy/docker-compose.yml -f deploy/.codex.caps-compat.deploy.yml ps
```

For the host-process scenarios, inspect the fixture project and process log instead:

```bash
ssh codex-testbox
cd "<REMOTE_RUN>"
docker compose -p "<COMPOSE_PROJECT>_fixtures" -f scripts/testbox/fixtures.compose.yml ... ps
cat dockrev.log
```

If public registries are rate-limiting the testbox, consider rerunning later.
