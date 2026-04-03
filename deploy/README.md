# Deploy (minimal)

This directory contains a minimal Docker Compose deployment with a reverse proxy:

For complete production docs (deployment, operations, integrations, API reference), use the Rspress docs site in `docs-site/`.

- `gateway` (nginx): routes `/` + `/api/*` to Dockrev, and `/supervisor/*` to the supervisor
- `dockrev`: Rust backend + embedded web UI
- `supervisor`: self-upgrade executor + console (stays available while Dockrev restarts)

## Quickstart

```bash
cd deploy
mkdir -p data

# Copy your Docker credentials (read-only). This is optional, but required for private registries.
# Dockrev stages this file for update jobs, so you do not need to also mount /root/.docker/config.json.
cp ~/.docker/config.json data/docker-config.json
# If you rely on Docker contexts, point DOCKREV_DOCKER_CONFIG at a real config.json path instead of a renamed copy.

docker compose up --build
```

Open:

- UI (via gateway): `http://127.0.0.1:50883/`
- API health (via gateway): `http://127.0.0.1:50883/api/health`
- Self-upgrade console (via gateway): `http://127.0.0.1:50883/supervisor/`

## Registering a stack

Dockrev automatically discovers Docker Compose projects by scanning containers for Compose labels and registering stacks automatically.

Important: Dockrev reads compose files from inside the `dockrev` container. The Compose label `com.docker.compose.project.config_files` contains **absolute paths** that must exist and be readable in the container. When Dockrev runs in Docker:

- Bind-mount the host compose directories into the container **read-only at the same absolute path** (example in `docker-compose.yml`)
- If the mount is missing/mismatched, discovery will not register/update the stack and will surface an actionable error

## Self-upgrade (Dockrev updates Dockrev)

The `supervisor` container is designed to keep the self-upgrade console available during the Dockrev restart window.

Notes:

- The Dockrev UI probes `GET /supervisor/self-upgrade` (same origin) before enabling the “升级 Dockrev” entry (a `401` means Dockrev authorization denied the request or no trusted identity reached Supervisor).
- Self-upgrade uses Docker + Compose on the host via the mounted Docker socket. The target compose files must be readable inside the supervisor container too (same absolute path requirement).
- Self-upgrade applies an extra Compose override file (image-only) and may cause containers in the same project to report different `com.docker.compose.project.config_files` label values. Dockrev will surface this as a warning (not invalid) and pick a stable canonical compose file list.
  - To let Dockrev read the override file (so discovery can reflect the self-upgraded image), set `DOCKREV_SUPERVISOR_STATE_PATH` to the same mounted absolute path in both `dockrev` and `supervisor` (for example `/data/self-upgrade.json`) and mount that directory into Dockrev read-only.

## Traefik + Authelia example

If you want a production-oriented Forward Auth deployment you can copy directly, use:

- `deploy/examples/traefik-authelia/docker-compose.yml`
- `deploy/examples/traefik-authelia/authelia/configuration.yml`
- `deploy/examples/traefik-authelia/authelia/users.yml`
- `deploy/examples/traefik-authelia/README.md`

This example keeps all Dockrev and Supervisor traffic on normal service/path routers. Traefik only forwards trusted identity headers from Authelia when a session exists; it does not perform Dockrev-specific user/group/path ACL and it does not split webhooks into special public routers.

## Forward Auth / reverse proxy

- Production should use trusted identity forwarding in front of Dockrev. Traefik / Authelia may establish identity, but Dockrev authorizes the request by matching `DOCKREV_AUTH_ALLOWED_USER` and/or `DOCKREV_AUTH_ALLOWED_GROUP`.
- `DOCKREV_AUTH_ALLOWED_USER` and `DOCKREV_AUTH_ALLOWED_GROUP` each accept a single value. If both are set, Dockrev allows the request when either the user or the group matches.
- Public anonymous surface is intentionally limited to `GET /api/health`, `GET /api/version`, and `/api/webhooks/*`. Other API/UI/supervisor routes rely on Dockrev authorization even when the gateway transparently forwards anonymous requests.
- In the sample Compose, `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false` is set. You must inject the trusted Forward Auth user header and, if used, the group header in front of Dockrev.
- For transparent Traefik + Authelia, apply the same `forwardAuth` middleware to Dockrev and Supervisor routes and keep the Authelia policy for `dockrev.example.com` at `bypass`, so Dockrev receives either trusted headers or no identity at all.

## Using a released image

The root README documents the currently published images and runtime variables.

For a production stack that should automatically consume the newest stable release, point both services at moving tags:

```yaml
services:
  dockrev:
    image: ghcr.io/ivanli-cn/dockrev:latest
  supervisor:
    image: ghcr.io/ivanli-cn/dockrev-supervisor:latest
```

If you prefer immutable/manual upgrades, pin `dockrev` to a concrete semver tag instead and leave the GitHub Actions production auto-deploy job disabled for that stack.

## Optional GitHub Actions production auto-deploy

If you want `Release` to automatically consume the newest stable `latest` image on production after GHCR + GitHub Release succeed, configure these repository-level variables and secrets:

- Variables:
  - `PRODUCTION_DEPLOY_HOST`
  - `PRODUCTION_DEPLOY_SSH_PORT` (optional, defaults to `22`)
  - `PRODUCTION_DEPLOY_USER`
  - `PRODUCTION_DEPLOY_STACK_DIR`
  - `PRODUCTION_DEPLOY_COMPOSE_FILE`
  - `PRODUCTION_DEPLOY_SERVICES` (optional, defaults to `dockrev supervisor`)
  - `PRODUCTION_DEPLOY_VERSION_URL`
- Secrets:
  - `PRODUCTION_DEPLOY_SSH_KEY`
  - `PRODUCTION_DEPLOY_SSH_KNOWN_HOSTS`

Behavior:

- The deploy job only runs for stable releases where `publish_latest=true`.
- If any required variable/secret is missing, the release still succeeds and the deploy job is skipped with an explicit summary entry.
- If your production Compose service names differ from the minimal example (`dockrev` + `supervisor`), set `PRODUCTION_DEPLOY_SERVICES` explicitly.
- The target stack must already consume moving tags for the upgraded services (for example `ghcr.io/ivanli-cn/dockrev:latest` and `ghcr.io/ivanli-cn/dockrev-supervisor:latest`). If you pin `dockrev` to a semver tag for manual upgrades, do not enable this auto-deploy path.
- When enabled, the job runs `docker compose pull` + `docker compose up -d` for the configured services on the target host, verifies each container image label version, then verifies the public version endpoint matches the released tag.
