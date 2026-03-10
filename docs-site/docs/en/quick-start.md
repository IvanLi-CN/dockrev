---
title: Quick Start
description: Bring up Dockrev quickly and run the first scan/check cycle.
---

# Quick Start

Goal: get Dockrev running in about 10 minutes and validate the end-to-end flow.

## Prerequisites

- Docker Engine is available
- Docker Compose is available (`docker-compose` or `docker compose`)
- Local port `50883` is available

## Minimal startup

```bash
cd deploy
mkdir -p data
# Dockrev reuses this file for update-job Docker/Compose auth, so you do not need an extra /root/.docker/config.json mount.
cp ~/.docker/config.json data/docker-config.json

docker compose up --build
```

Entry points:

- UI: `http://127.0.0.1:50883/`
- API health: `http://127.0.0.1:50883/api/health`
- Supervisor: `http://127.0.0.1:50883/supervisor/`

## First validation checklist

1. Open Overview and confirm services are listed.
2. Run Discovery scan from UI.
3. Trigger one Check job for any service.
4. Open Queue and confirm state transitions.

## Local dev startup (without containers)

### Backend

```bash
DOCKREV_HTTP_ADDR=127.0.0.1:50883 DOCKREV_DB_PATH=/tmp/dockrev.sqlite3 cargo run -p dockrev-api --bin dockrev
```

### Supervisor

```bash
DOCKREV_SUPERVISOR_HTTP_ADDR=127.0.0.1:50884 cargo run -p dockrev-supervisor --bin dockrev-supervisor
```

### Web UI

```bash
cd web
bun install
bun run dev
```

## Next steps

- Continue with [Deployment](./deploy)
- Then apply [Configuration](./config)
