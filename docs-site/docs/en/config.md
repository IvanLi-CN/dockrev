---
title: Configuration
description: Runtime configuration reference for Dockrev API and Supervisor.
---

# Configuration

## API core config (`dockrev-api`)

| Variable | Default | Purpose |
| --- | --- | --- |
| `DOCKREV_HTTP_ADDR` | `0.0.0.0:50883` | API bind address |
| `DOCKREV_DB_PATH` | `./data/dockrev.sqlite3` | SQLite file path |
| `DOCKREV_DOCKER_CONFIG` | empty | Docker registry credentials file |
| `DOCKREV_COMPOSE_BIN` | `docker-compose` | Compose command selector |
| `DOCKREV_AUTH_FORWARD_HEADER_NAME` | `X-Forwarded-User` | Forward auth header name |
| `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV` | `true` | Anonymous mode for dev; auto-disabled when an allowed user/group is configured |
| `DOCKREV_SELF_UPGRADE_URL` | `/supervisor/` | Self-upgrade UI URL |
| `DOCKREV_IMAGE_REPO` | `ghcr.io/ivanli-cn/dockrev` | Dockrev service image repo matcher |
| `DOCKREV_WEBHOOK_SECRET` | empty | Shared secret for `/api/webhooks/trigger` |
| `DOCKREV_HOST_PLATFORM` | empty | Host platform override |
| `DOCKREV_DISCOVERY_INTERVAL_SECONDS` | `60` | Discovery interval |
| `DOCKREV_DISCOVERY_MAX_ACTIONS` | `200` | Max actions per discovery run |

## Check and retry controls

| Variable | Default | Purpose |
| --- | --- | --- |
| `DOCKREV_REGISTRY_RETRY_MAX_ATTEMPTS` | `3` | Retry attempts after 429 |
| `DOCKREV_REGISTRY_RETRY_BASE_MS` | `250` | Retry backoff base |
| `DOCKREV_REGISTRY_RETRY_MAX_MS` | `2000` | Retry backoff cap |
| `DOCKREV_DEPLOY_CHECK_LOCAL_COMMAND_TIMEOUT_SECONDS` | `12` | Preflight local probe timeout |

Fixed scheduler behavior:

- Check worker concurrency: `7`
- Worker start stagger: `1s`
- Registry per-host concurrency: `5`

## Supervisor config (`dockrev-supervisor`)

| Variable | Default | Purpose |
| --- | --- | --- |
| `DOCKREV_SUPERVISOR_HTTP_ADDR` | `0.0.0.0:50884` | Supervisor bind address |
| `DOCKREV_SUPERVISOR_BASE_PATH` | `/supervisor` | Mounted base path |
| `DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO` | `ghcr.io/ivanli-cn/dockrev` | Target image repo for self-upgrade |
| `DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID` | empty | Override auto-matched container |
| `DOCKREV_SUPERVISOR_DOCKER_HOST` | empty | Docker endpoint override |
| `DOCKREV_SUPERVISOR_COMPOSE_BIN` | `docker-compose` | Compose command selector |
| `DOCKREV_SUPERVISOR_STATE_PATH` | `./data/supervisor/self-upgrade.json` | Persisted operation state |

## Production baseline

- Disable anonymous mode (it is also ignored once an allowed user/group is configured)
- Ensure forward auth header injection
- Persist DB/state on durable volume
- Reduce Docker socket exposure (or use socket proxy)
