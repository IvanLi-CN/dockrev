# Dockrev

Dockrev is a self-hosted Docker/Compose update manager.

MVP status: see `docs/plan/README.md`.

## Tech stack (selected)

Back-end (Rust):

- Runtime: Tokio
- HTTP API: Axum
- Logging: tracing + tracing-subscriber
- Docker Engine access: via `docker` CLI (typically through docker-socket-proxy via `DOCKER_HOST`)
- Registry auth: reads `~/.docker/config.json`
- State: SQLite (planned)

Front-end (React + TypeScript):

- Bundler/dev server: Vite
- UI: React + TypeScript

## Repo layout

- `crates/dockrev-api`: Rust HTTP API + worker runtime (initial scaffold)
- `crates/dockrev-supervisor`: Self-upgrade supervisor (independent console + executor)
- `web`: React + TypeScript (Vite) front-end

## Dev quickstart

Backend:

```bash
DOCKREV_HTTP_ADDR=127.0.0.1:50883 DOCKREV_DB_PATH=/tmp/dockrev.sqlite3 cargo run -p dockrev-api --bin dockrev
```

Supervisor (self-upgrade console):

```bash
DOCKREV_SUPERVISOR_HTTP_ADDR=127.0.0.1:50884 cargo run -p dockrev-supervisor --bin dockrev-supervisor
```

Front-end:

```bash
cd web
bun install
bun run dev
```

Storybook:

```bash
cd web
bun run storybook:start
```

Open:

- UI (dev server): `http://127.0.0.1:50884/`
- UI (embedded): `http://127.0.0.1:50883/`
- API health: `http://127.0.0.1:50883/api/health`
- API version: `http://127.0.0.1:50883/api/version`

## Runtime config

Environment variables (API):

- `APP_EFFECTIVE_VERSION` (optional) effective version used by `/api/version` (defaults to `CARGO_PKG_VERSION`)
- `DOCKREV_HTTP_ADDR` (default `0.0.0.0:50883`)
- `DOCKREV_DB_PATH` (default `./data/dockrev.sqlite3`)
- `DOCKREV_DOCKER_CONFIG` (optional) path to Docker `config.json` for registry credentials
- `DOCKREV_COMPOSE_BIN` (default `docker-compose`; set to `docker` to use the plugin)
- `DOCKREV_AUTH_FORWARD_HEADER_NAME` (default `X-Forwarded-User`)
- `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV` (default `true`; set to `false` in production)
- `DOCKREV_SELF_UPGRADE_URL` (default `/supervisor/`) UI jump target for “升级 Dockrev”
- `DOCKREV_IMAGE_REPO` (default `ghcr.io/ivanli-cn/dockrev`) image repo used by the UI to detect which service is “Dockrev” for showing “升级 Dockrev” (example: set to `dockrev` for local images like `dockrev:local`)
- `DOCKREV_WEBHOOK_SECRET` (optional) shared secret for `/api/webhooks/trigger`
- `DOCKREV_HOST_PLATFORM` (optional) override host platform (example `linux/amd64`)
- `DOCKREV_DISCOVERY_INTERVAL_SECONDS` (default `60`; must be `>= 10`)
- `DOCKREV_DISCOVERY_MAX_ACTIONS` (default `200`) max actions returned by `POST /api/discovery/scan`

Environment variables (Supervisor):

- `DOCKREV_SUPERVISOR_HTTP_ADDR` (default `0.0.0.0:50884`)
- `DOCKREV_SUPERVISOR_BASE_PATH` (default `/supervisor`)
- `DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO` (default `ghcr.io/ivanli-cn/dockrev`)
- `DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID` (optional) override auto-match
- `DOCKREV_SUPERVISOR_TARGET_COMPOSE_PROJECT` / `DOCKREV_SUPERVISOR_TARGET_COMPOSE_SERVICE` / `DOCKREV_SUPERVISOR_TARGET_COMPOSE_FILES` (optional overrides)
- `DOCKREV_SUPERVISOR_DOCKER_HOST` (optional) docker engine endpoint
- `DOCKREV_SUPERVISOR_COMPOSE_BIN` (default `docker-compose`; set to `docker` to use the plugin)
- `DOCKREV_SUPERVISOR_STATE_PATH` (default `./data/supervisor/self-upgrade.json`)

## UI: scan / preview / apply

- Scan: Overview/Services “立即扫描”
- Preview (dry-run): Service detail “预览更新”
- Apply (one-click):
  - Overview: “更新全部”
  - Overview/Services: “更新此 stack” + service row “执行更新”
  - Service detail: “执行更新”
- Dockrev self-upgrade:
  - For the Dockrev service, “升级 Dockrev” jumps to the supervisor console (disabled unless `GET {selfUpgradeBaseUrl}/self-upgrade` returns 2xx; a 401 means auth/forward header is missing).

## Auto-discovery (Compose projects)

Dockrev automatically discovers Docker Compose projects by scanning running containers and grouping by Compose labels:

- `com.docker.compose.project`
- `com.docker.compose.project.config_files`

Notes:

- Auto-discovery is always enabled (no enable/disable switch).
- Manual stack registration (`POST /api/stacks`) is disabled.
- The `config_files` paths are **container-visible absolute paths**. If Dockrev runs in a container, you must bind-mount the host directories into Dockrev **read-only at the same absolute path**, otherwise discovery will surface an actionable error (mount missing/unreadable).

## Deploy (minimal)

See `deploy/README.md` for a minimal Docker Compose deployment.

## Releases / Images

- GHCR: `ghcr.io/ivanli-cn/dockrev:<semver>` (single image)
- The `Release` workflow runs only via `workflow_run` after `CI (main)` succeeds on `main`
- The `Release` workflow cleans up Actions artifacts after a successful run; on non-success, it keeps key artifacts with `retention-days: 1` and deletes `*.dockerbuild` build records to avoid long-tail storage usage
- Automatic releases are gated by PR intent labels (exactly one required on PRs targeting `main`):
  - `type:docs` / `type:skip` → skip release
  - `type:patch` / `type:minor` / `type:major` → publish with the corresponding semver bump
- Direct `push` to `main` without an associated PR conservatively skips release
- `latest` is updated only by the automatic release path above
- GitHub Releases include Linux binaries for `dockrev` and `dockrev-supervisor` (amd64/arm64 × gnu/musl) as `.tar.gz` + `.sha256`

## Notifications

Notifications are configured via UI (stored in SQLite; secrets are masked on read):

- Webhook: POSTs a JSON payload to the configured URL
- Telegram: calls `sendMessage`
- Email: `smtpUrl` supports `?to=a@example.com,b@example.com&from=Dockrev <noreply@example.com>`
- Web Push: configure VAPID keys, then use the UI buttons to subscribe/unsubscribe and test

VAPID keys can be generated with:

```bash
bunx web-push generate-vapid-keys --json
```
