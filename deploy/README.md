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

Dockrev reads compose files from inside the `dockrev` container. To register a stack, the compose file paths you submit must exist in the container:

- Mount your compose directory into the container (example in `docker-compose.yml`)
- Register using the container path (absolute)

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

- `latest` is only updated by `push` to `main`; `release: published` only pushes `<semver>`.
- The image supports both direct socket mount and `DOCKER_HOST` (e.g. `tcp://docker-socket-proxy:2375`).
