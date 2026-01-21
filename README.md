# Dockrev

Dockrev is a self-hosted Docker/Compose update manager.

MVP status: see `docs/plan/README.md`.

## Tech stack (selected)

Back-end (Rust):

- Runtime: Tokio
- HTTP API: Axum
- Logging: tracing + tracing-subscriber
- Docker Engine API: via Docker socket (planned: Rust client)
- Registry auth: reads `~/.docker/config.json`
- State: SQLite (planned)

Front-end (React + TypeScript):

- Bundler/dev server: Vite
- UI: React + TypeScript

## Repo layout

- `crates/dockrev-api`: Rust HTTP API + worker runtime (initial scaffold)
- `web`: React + TypeScript (Vite) front-end

## Dev quickstart

Backend:

```bash
DOCKREV_HTTP_ADDR=127.0.0.1:50883 DOCKREV_DB_PATH=/tmp/dockrev.sqlite3 cargo run -p dockrev-api --bin dockrev
```

Front-end:

```bash
cd web
npm install
npm run dev
```

Open:

- UI (dev server): `http://127.0.0.1:5173/`
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
- `DOCKREV_WEBHOOK_SECRET` (optional) shared secret for `/api/webhooks/trigger`
- `DOCKREV_HOST_PLATFORM` (optional) override host platform (example `linux/amd64`)

## Deploy (minimal)

See `deploy/README.md` for a minimal Docker Compose deployment.

## Releases / Images

- GHCR: `ghcr.io/ivanli-cn/dockrev:<semver>` (single image)
- The `Release` workflow runs only via `workflow_run` after `CI (main)` succeeds on `main`
- `latest` is updated only by the automatic release path above
- GitHub Releases include Linux binaries (amd64/arm64 × gnu/musl) as `.tar.gz` + `.sha256`

## Notifications

Notifications are configured via UI (stored in SQLite; secrets are masked on read):

- Webhook: POSTs a JSON payload to the configured URL
- Telegram: calls `sendMessage`
- Email: `smtpUrl` supports `?to=a@example.com,b@example.com&from=Dockrev <noreply@example.com>`
- Web Push: configure VAPID keys, then use the UI buttons to subscribe/unsubscribe and test

VAPID keys can be generated with:

```bash
npm install web-push -g
web-push generate-vapid-keys --json
```
