# Deploy (minimal)

This directory contains a minimal Docker Compose deployment:

- `dockrev`: single container (Rust backend + embedded web UI)

## Quickstart

```bash
cd deploy
mkdir -p data

# Copy your Docker credentials (read-only). This is optional, but required for private registries.
cp ~/.docker/config.json data/docker-config.json

docker compose up --build
```

Open:

- UI: `http://127.0.0.1:50883/`
- API health: `http://127.0.0.1:50883/api/health`

## Registering a stack

Dockrev automatically discovers Docker Compose projects by scanning containers for Compose labels and registering stacks automatically.

Important: Dockrev reads compose files from inside the `dockrev` container. The Compose label `com.docker.compose.project.config_files` contains **absolute paths** that must exist and be readable in the container. When Dockrev runs in Docker:

- Bind-mount the host compose directories into the container **read-only at the same absolute path** (example in `docker-compose.yml`)
- If the mount is missing/mismatched, discovery will not register/update the stack and will surface an actionable error

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
- The image supports both direct socket mount and `DOCKER_HOST` (e.g. `tcp://docker-socket-proxy:2375`).
