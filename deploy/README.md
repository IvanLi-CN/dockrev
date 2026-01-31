# Deploy (minimal)

This directory contains a minimal Docker Compose deployment with a reverse proxy:

- `gateway` (nginx): routes `/` + `/api/*` to Dockrev, and `/supervisor/*` to the supervisor
- `dockrev`: Rust backend + embedded web UI
- `supervisor`: self-upgrade executor + console (stays available while Dockrev restarts)

## Quickstart

```bash
cd deploy
mkdir -p data

# Copy your Docker credentials (read-only). This is optional, but required for private registries.
cp ~/.docker/config.json data/docker-config.json

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

- The Dockrev UI probes `GET /supervisor/self-upgrade` (same origin) before enabling the “升级 Dockrev” entry (401 means auth/forward header is missing).
- Self-upgrade uses Docker + Compose on the host via the mounted Docker socket. The target compose files must be readable inside the supervisor container too (same absolute path requirement).

## Auth / reverse proxy

- Production default is to require a forward header (see `DOCKREV_AUTH_FORWARD_HEADER_NAME`).
- In the sample Compose, `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false` is set. You must inject the forward header in front of Dockrev.

## Using a released image

Replace the `build:` section with:

```yaml
services:
  dockrev:
    image: ghcr.io/ivanli-cn/dockrev:<semver>
```

Notes:

- `latest` is updated only by the automatic release path after `CI (main)` succeeds on `main`.
- Use `0.3.5` or newer to avoid the historical exec-bit issue (`/usr/local/bin/dockrev`: permission denied).
- The image supports both direct socket mount and `DOCKER_HOST` (e.g. `tcp://docker-socket-proxy:2375`).
