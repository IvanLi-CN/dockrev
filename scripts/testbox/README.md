# codex-testbox E2E scripts

This folder contains manual regression scripts that run Dockrev on the shared test machine
`codex-testbox` (192.168.31.15) in an isolated per-run workspace.

Available scripts:

1) `check-job-recovery.e2e.ts`
   - `POST /api/checks` blocks parallel runs for the same scope (`409 conflict`)
   - Dockrev restart terminates orphaned running jobs (`failed` + `summary.terminated`)
2) `check-version-inference-sse.e2e.ts`
   - `/api/version-inference/events` pushes `version_inference_event` with monotonic ids
   - `POST /api/services/{id}/version-inference/refresh` emits `task_enqueued`
   - reconnect with `afterId` receives newer events

These scripts are intentionally **not wired into GitHub Actions** because they depend on SSH + a shared
host environment and can be flaky due to network / registry rate limits.

Important: the shared testbox runs Docker inside LXC where `CAP_SETFCAP` is not available. This
means `docker build` / `docker compose up --build` can fail. To keep tests reliable, these
scripts:

- builds the Dockrev binary on the testbox **inside a `rust:1.91-bookworm` container** with caps
  dropped (so it can run under LXC),
- runs Dockrev as a **host process** (so it can read compose files directly),
- uses Docker only for starting the fixture Compose project that Dockrev scans/checks.

## Prerequisites (local machine)

- `bun` (to run the script)
- `ssh` and `rsync`
- SSH access to `codex-testbox` (host alias: `codex-testbox`)

## Quickstart

From the repo root, run one of:

```bash
bun scripts/testbox/check-job-recovery.e2e.ts
bun scripts/testbox/check-version-inference-sse.e2e.ts
```

Run twice to reduce false confidence:

```bash
bun scripts/testbox/check-job-recovery.e2e.ts
bun scripts/testbox/check-job-recovery.e2e.ts
bun scripts/testbox/check-version-inference-sse.e2e.ts
bun scripts/testbox/check-version-inference-sse.e2e.ts
```

## Environment variables

- `TESTBOX_HOST` (default: `codex-testbox`)
- `TESTBOX_SSH_OPTS` (default: `-o BatchMode=yes`)
- `REMOTE_HTTP_PORT` (optional): force the remote gateway port; otherwise a free port is selected.
- `DOCKREV_AUTH_HEADER_NAME` (default: `X-Forwarded-User`)
- `DOCKREV_AUTH_HEADER_VALUE` (default: `test`)
- `DOCKREV_TEST_KEEP` (default: `0`): set to `1` to keep remote artifacts on failure.
- `DOCKREV_BUILD_TIMEOUT_SECONDS` (default: `900`): timeout for building the Dockrev binary on the testbox.
- `DOCKREV_TEST_TIMEOUT_SECONDS` (default: `180`): overall timeout for the test portion (excluding build).
- `DOCKREV_JOB_WAIT_SECONDS` (default: `60`): timeout for job state transitions.
- `DOCKREV_SSE_WAIT_MS` (default: `45000`): SSE event wait timeout (used by `check-version-inference-sse.e2e.ts`).
- `DOCKREV_RESTART_GRACE_SECONDS` (default: `1`): small sleep after restart.
- `DOCKREV_RESTART_MODE` (default: `hard`): `hard` uses SIGKILL to exercise startup recovery;
  `soft` uses SIGTERM to exercise graceful shutdown.

## What the script does (high level)

- Creates an isolated remote run directory under `/srv/codex/workspaces/$USER/.../runs/$RUN_ID`
- `rsync`s this repo to that remote directory (excluding large build artifacts)
- Builds and runs Dockrev on the testbox as a host process (binds to `127.0.0.1:<port>`).
- Starts the fixture stack (`scripts/testbox/fixtures.compose.yml`) under a unique `<project>_fixtures`
  and drops capabilities (LXC quirk) so containers can start on the shared host.
- Opens an SSH port-forward so the local script can call Dockrev via `http://127.0.0.1:<localPort>`
- Runs script-specific assertions (job recovery or version inference SSE)
- Cleans up remote containers/volumes and the remote run directory (unless `DOCKREV_TEST_KEEP=1`)

## Troubleshooting

If the script fails and you want to inspect the remote run:

1) Rerun with `DOCKREV_TEST_KEEP=1`
2) Use the printed `REMOTE_RUN` and `COMPOSE_PROJECT` values to inspect:

```bash
ssh codex-testbox
cd "<REMOTE_RUN>"
docker compose -p "<COMPOSE_PROJECT>_fixtures" -f scripts/testbox/fixtures.compose.yml ... ps
cat dockrev.log
```

If public registries are rate-limiting the testbox, consider reducing fixture services or rerun
later.
