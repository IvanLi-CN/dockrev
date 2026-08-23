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
- Self-upgrade writes its state and `self-upgrade.override.yml` below the shared `/supervisor-state` mount. The API receives that path read-only and the supervisor receives it read-write; the deployment smoke test verifies supervisor write/API read.
- Automatic updates use the durable image-only managed override directory and never pass a `/tmp/dockrev-override-*.yml` path to `compose up`. Historical deleted temporary overrides remain visible until an administrator explicitly reconciles them.

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
