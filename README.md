# Dockrev

Dockrev is a self-hosted Docker/Compose update manager.

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
cd crates/dockrev-api
cargo run
```

Defaults to `DOCKREV_HTTP_ADDR=0.0.0.0:50883` (override via `.env` / env vars).

Front-end:

```bash
cd web
npm install
npm run dev
```

## Credentials

Dockrev reads Docker registry credentials from `~/.docker/config.json` (mount it into the container in production).
