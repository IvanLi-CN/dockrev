# Deploy (minimal)

This directory contains a minimal Docker Compose deployment:

- `dockrev-api`: Rust HTTP API (binds to Docker socket, stores SQLite under `deploy/data/`)
- `dockrev-web`: static web UI served by Nginx, proxies `/api/*` to `dockrev-api`

## Quickstart

```bash
cd deploy
mkdir -p data

# Copy your Docker credentials (read-only). This is optional, but required for private registries.
cp ~/.docker/config.json data/docker-config.json

docker compose up --build
```

Open:

- UI: `http://127.0.0.1:5173/`
- API health: `http://127.0.0.1:5173/api/health`

## Registering a stack

Dockrev reads compose files from inside the `dockrev-api` container. To register a stack, the compose file paths you submit must exist in the container:

- Mount your compose directory into the container (example in `docker-compose.yml`)
- Register using the container path (absolute)

## Auth / reverse proxy

- Production default is to require a forward header (see `DOCKREV_AUTH_FORWARD_HEADER_NAME`).
- In the sample Compose, `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false` is set. You must inject the forward header in front of Nginx/API.

