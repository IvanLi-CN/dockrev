# Dockrev

Dockrev is a self-hosted Docker/Compose update manager.

MVP status: see `docs/plan/README.md`.

## Documentation site

Complete deployment, usage, operations, troubleshooting, and API docs live in the Rspress docs site:

- Source: `docs-site/`
- Local dev: `bun run docs:dev` (default `http://127.0.0.1:50885`)
- Local build: `bun run docs:build`
- Local preview: `bun run docs:preview` (default `http://127.0.0.1:50885`)
- Port override: `DOCS_PORT=<port> bun run docs:dev` / `DOCS_PORT=<port> bun run docs:preview`
- Online (GitHub Pages): published by `.github/workflows/docs-pages.yml`

This README intentionally stays as a quick entrypoint.

## Tech stack (selected)

Back-end (Rust):

- Runtime: Tokio
- HTTP API: Axum
- Logging: tracing + tracing-subscriber
- Docker Engine access: via `docker` CLI (typically through docker-socket-proxy via `DOCKER_HOST`)
- Registry auth: reads `DOCKREV_DOCKER_CONFIG` when configured
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

Codex UI/UX skill (UI UX Pro Max):

```bash
npx -y uipro-cli@2.2.3 init --ai codex --offline
python3 .codex/skills/ui-ux-pro-max/scripts/search.py "dockrev docker compose dashboard" --design-system -f markdown
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
- `DOCKREV_DOCKER_CONFIG` (optional) path to a Docker `config.json`; Dockrev stages it into a temporary Docker CLI config directory for update-job `docker compose` / `docker pull` auth, and also copies Docker context metadata (`contexts/`) when the configured path is a real `config.json`
- `DOCKREV_COMPOSE_BIN` (default `docker-compose`; set to `docker` to use the plugin)
- `DOCKREV_DEPLOY_CHECK_LOCAL_COMMAND_TIMEOUT_SECONDS` (default `12`; must be `>= 1`) timeout for local `docker info` / `compose version` probes used by `GET /api/deploy-check/report`
- `DOCKREV_AUTH_FORWARD_HEADER_NAME` (default `X-Forwarded-User`) trusted Forward Auth user header
- `DOCKREV_AUTH_GROUP_HEADER_NAME` (default `Remote-Groups`) trusted Forward Auth group header
- `DOCKREV_AUTH_ALLOWED_USER` (optional) single allowed user for Dockrev authorization
- `DOCKREV_AUTH_ALLOWED_GROUP` (optional) single allowed group for Dockrev authorization
- `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV` (default `true`; auto-disabled once `DOCKREV_AUTH_ALLOWED_USER` or `DOCKREV_AUTH_ALLOWED_GROUP` is set; still set it to `false` in production)
- Anonymous public surface is intentionally limited to `GET /api/health`, `GET /api/version`, and `/api/webhooks/*`; all other API/UI/supervisor routes rely on Dockrev authorization even when the gateway transparently forwards anonymous requests.
- `DOCKREV_SELF_UPGRADE_URL` (default `/supervisor/`) UI jump target for “升级 Dockrev”
- `DOCKREV_IMAGE_REPO` (default `ghcr.io/ivanli-cn/dockrev`) image repo used by the UI to detect which service is “Dockrev” for showing “升级 Dockrev” (example: set to `dockrev` for local images like `dockrev:local`)
- `DOCKREV_SUPERVISOR_STATE_PATH` (optional for `dockrev-api`; set the same absolute path used by `dockrev-supervisor` so discovery can recognize the generated `self-upgrade.override.yml`)
- `DOCKREV_WEBHOOK_SECRET` (optional) shared secret for `/api/webhooks/trigger`
- `DOCKREV_HOST_PLATFORM` (optional) override host platform (example `linux/amd64`)
- `DOCKREV_DISCOVERY_INTERVAL_SECONDS` (default `60`; must be `>= 10`)
- `DOCKREV_DISCOVERY_MAX_ACTIONS` (default `200`) max actions returned by `POST /api/discovery/scan`
- Check scheduling is fixed: `7` concurrent check workers with a `1s` stagger between worker starts (not runtime-configurable)
- Registry host concurrency is fixed: `5` in-flight requests per host (independent fixed cap; not runtime-configurable)
- Legacy `DOCKREV_CHECK_CONCURRENCY` / `DOCKREV_REGISTRY_PER_HOST_CONCURRENCY` are deprecated:
  any value = warning + ignored (remove them)
- `DOCKREV_REGISTRY_RETRY_MAX_ATTEMPTS` (default `3`) max retry attempts after a `429 Too Many Requests`
- `DOCKREV_REGISTRY_RETRY_BASE_MS` (default `250`; must be `>= 1`) exponential backoff base for `429` retries
- `DOCKREV_REGISTRY_RETRY_MAX_MS` (default `2000`; must be `>= DOCKREV_REGISTRY_RETRY_BASE_MS`) max single retry delay

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
  - For the Dockrev service, “升级 Dockrev” jumps to the supervisor console (disabled unless `GET {selfUpgradeBaseUrl}/self-upgrade` returns 2xx; a 401 means Forward Auth is missing or Dockrev authorization denied the request).

## Auto-discovery (Compose projects)

Dockrev automatically discovers Docker Compose projects by scanning running containers and grouping by Compose labels:

- `com.docker.compose.project`
- `com.docker.compose.project.config_files`

Notes:

- Auto-discovery is always enabled (no enable/disable switch).
- Manual stack registration (`POST /api/stacks`) is disabled.
- The `config_files` paths are **container-visible absolute paths**. If Dockrev runs in a container, you must bind-mount the host directories into Dockrev **read-only at the same absolute path**, otherwise discovery will surface an actionable error (mount missing/unreadable).
- If the same Compose project reports multiple distinct `config_files` variants (common after self-upgrade or when a one-off compose override file was used):
  - Dockrev will try to pick a deterministic canonical list (prefer a safe superset that only adds an image-only override file).
  - If an extra override file path is not readable in the Dockrev container, Dockrev falls back to the common compose files and surfaces a warning with a mounting hint.

## Deploy (minimal)

See `deploy/README.md` for a minimal Docker Compose deployment.

## Releases / Images

- GHCR (Dockrev): `ghcr.io/ivanli-cn/dockrev:<semver>`
- GHCR (Supervisor): `ghcr.io/ivanli-cn/dockrev-supervisor:<semver>`
- Since `0.3.5`, images ensure shipped binaries are executable (0755); the release workflow validates this before pushing to GHCR.
- `CI (main)` materializes immutable release snapshots into git notes `refs/notes/release-snapshots`; it backfills missing first-parent snapshots so burst merges on `main` do not skip intermediate releases.
- The `Release` workflow still auto-starts from `workflow_run` after `CI (main)` succeeds on `main`, but it now releases the oldest pending snapshot up to that `head_sha` instead of re-deriving intent/version directly from the triggering commit.
- `workflow_dispatch(head_sha=<main-commit-sha>)` keeps the same input name and serves as the manual backfill path for a specific `main` commit.
- The `Release` workflow no longer `git push`es release tags directly; the GitHub Release API path creates/updates the release tag for the selected `TARGET_SHA`, which avoids the workflow-file tag permission dead-end under the default `GITHUB_TOKEN` model.
- After GHCR image push, GitHub Release creation/update, and source-PR release comment verification all succeed, the `Release` workflow records a mutable publication ledger in git notes `refs/notes/release-publications`; stable `latest` is derived from the newest published stable note, not merely the newest stable snapshot.
- The `Release` workflow cleans up Actions artifacts after a successful run; on non-success, it keeps key artifacts with `retention-days: 1` and deletes `*.dockerbuild` build records to avoid long-tail storage usage
- Automatic releases are gated by PR intent labels (exactly one required on PRs targeting `main`):
  - `type:docs` / `type:skip` → skip release
  - `type:patch` / `type:minor` / `type:major` → publish with the corresponding semver bump
- Release-enabled PRs (`type:patch` / `type:minor` / `type:major`) must not touch `.github/workflows/**`; release-infra-only PRs must use `type:skip` or `type:docs`
- Required release channel label (exactly one required on PRs targeting `main`):
  - `channel:stable` → publish a stable release (tag: `<semver>`, updates `latest`)
  - `channel:rc` → publish an RC prerelease (tag: `<semver>-rc.<shortsha>`, GitHub Release marked as prerelease, and does **not** update `latest`)
- Direct `push` to `main` without an associated PR conservatively skips release
- `latest` is updated only by the newest published stable release currently visible on `main`
- The release admin path can mark a frozen mislabel target as skipped via `workflow_dispatch(head_sha=<main-commit-sha>, admin_action=skip)`; the skip is stored in `refs/notes/release-overrides` without rewriting immutable snapshots
- A manually requested `workflow_dispatch(..., admin_action=release)` run refuses targets already marked as skipped, so override-ledger decisions cannot be bypassed into a partial artifact publish
- After GitHub Release succeeds, the workflow must leave exactly one bot-owned marker-based issue comment on the source PR with the actual `release_tag`, release URL, workflow run URL, and channel; the workflow auto-prunes older bot-owned duplicate markers, but still fails if a foreign marker blocks the contract
- Live quality-gates checks now run explicitly inside `CI (PR)` / `CI (main)` with authenticated `GITHUB_TOKEN`; `release-channel-contract-check.sh` stays offline and only covers contract + mock API self-tests
- GitHub Releases include Linux binaries for `dockrev` and `dockrev-supervisor` (amd64/arm64 × gnu/musl) as `.tar.gz` + `.sha256`
- Release assets are validated as executable binaries (the workflow enforces `chmod +x` before packaging to avoid artifacts losing exec bits)

## Notifications

Notifications are configured via UI (stored in SQLite; secrets are masked on read):

- Webhook: POSTs a JSON payload to the configured URL
- Telegram: calls `sendMessage`
- Email: `smtpUrl` supports `?to=a@example.com,b@example.com&from=Dockrev <noreply@example.com>`
- Web Push: configure VAPID keys, then use the UI buttons to subscribe/unsubscribe and test

## GitHub Packages (GHCR) webhook

Dockrev can register GitHub repository webhooks for the `package` event (GitHub Packages / GHCR). When a new package version is published, GitHub calls back Dockrev and Dockrev triggers a discovery scan.

- Configure in Settings UI: targets (repo URL / profile URL / username), repo selection (default all), PAT (masked), and callback URL.
- PAT requirements: must be able to list repositories (for owner targets) and manage repository webhooks for the selected repos.
- Callback URL must be reachable by GitHub (public HTTPS).

VAPID keys can be generated with:

```bash
bunx web-push generate-vapid-keys --json
```
