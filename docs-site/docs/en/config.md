---
title: Configuration
description: Runtime configuration reference for Dockrev API and Supervisor.
---

# Configuration

## Production must-haves

1. `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false`
2. `DOCKREV_AUTH_FORWARD_HEADER_NAME` (match your ingress Forward Auth user header)
3. `DOCKREV_AUTH_GROUP_HEADER_NAME` (if you use group-based authorization)
4. At least one of `DOCKREV_AUTH_ALLOWED_USER` or `DOCKREV_AUTH_ALLOWED_GROUP`
5. `DOCKREV_DB_PATH` on durable storage
6. `DOCKREV_SUPERVISOR_STATE_PATH` on durable storage
7. `DOCKREV_IMAGE_REPO` matching the image repo you actually deploy

## API core config (`dockrev-api`)

| Variable | Default | Purpose |
| --- | --- | --- |
| `DOCKREV_HTTP_ADDR` | `0.0.0.0:50883` | API bind address |
| `DOCKREV_DB_PATH` | `./data/dockrev.sqlite3` | SQLite file path |
| `DOCKREV_DOCKER_CONFIG` | empty | Path to a Docker `config.json`; Dockrev stages it into a temporary Docker CLI config directory for update jobs and also copies sibling metadata when the path is a real `config.json` |
| `DOCKREV_COMPOSE_BIN` | `docker-compose` | Compose command selector |
| `DOCKREV_AUTH_FORWARD_HEADER_NAME` | `X-Forwarded-User` | Forward Auth user header name |
| `DOCKREV_AUTH_GROUP_HEADER_NAME` | `Remote-Groups` | Forward Auth group header name |
| `DOCKREV_AUTH_ALLOWED_USER` | empty | Single allowed Dockrev username |
| `DOCKREV_AUTH_ALLOWED_GROUP` | empty | Single allowed Dockrev group |
| `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV` | `true` | Anonymous mode for dev; auto-disabled when an allowed user/group is configured |
| `DOCKREV_SELF_UPGRADE_URL` | `/supervisor/` | Self-upgrade UI URL |
| `DOCKREV_IMAGE_REPO` | `ghcr.io/ivanli-cn/dockrev` | Dockrev service image repo matcher |
| `DOCKREV_WEBHOOK_SECRET` | empty | Shared secret for `/api/webhooks/trigger` |
| `DOCKREV_HOST_PLATFORM` | empty | Host platform override |
| `DOCKREV_DISCOVERY_INTERVAL_SECONDS` | `60` | Discovery interval |
| `DOCKREV_DISCOVERY_MAX_ACTIONS` | `200` | Max actions per discovery run |

Notes:

- `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV` is for local development only; once `DOCKREV_AUTH_ALLOWED_USER` or `DOCKREV_AUTH_ALLOWED_GROUP` is set, the anonymous dev bypass is ignored.
- `DOCKREV_AUTH_ALLOWED_USER` and `DOCKREV_AUTH_ALLOWED_GROUP` are single-value allowlists; Dockrev accepts either match when both are configured.
- The anonymous public surface is intentionally fixed to `GET /api/health`, `GET /api/version`, and `/api/webhooks/*`. Every other API/UI/supervisor route is still authorized inside Dockrev.
- A wrong `DOCKREV_IMAGE_REPO` breaks Dockrev self-upgrade detection in the UI.

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
| `DOCKREV_AUTH_FORWARD_HEADER_NAME` | `X-Forwarded-User` | Supervisor Forward Auth user header name |
| `DOCKREV_AUTH_GROUP_HEADER_NAME` | `Remote-Groups` | Supervisor Forward Auth group header name |
| `DOCKREV_AUTH_ALLOWED_USER` | empty | Single allowed Supervisor username |
| `DOCKREV_AUTH_ALLOWED_GROUP` | empty | Single allowed Supervisor group |
| `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV` | `true` | Anonymous dev mode; ignored once an allowed user/group is configured |
| `DOCKREV_SUPERVISOR_TARGET_IMAGE_REPO` | `ghcr.io/ivanli-cn/dockrev` | Target image repo for self-upgrade |
| `DOCKREV_SUPERVISOR_TARGET_CONTAINER_ID` | empty | Override auto-matched container |
| `DOCKREV_SUPERVISOR_DOCKER_HOST` | empty | Docker endpoint override |
| `DOCKREV_SUPERVISOR_COMPOSE_BIN` | `docker-compose` | Compose command selector |
| `DOCKREV_SUPERVISOR_STATE_PATH` | `./data/supervisor/self-upgrade.json` | Persisted operation state |

## Production baseline

- Disable anonymous mode
- Ensure trusted Forward Auth header injection
- Persist DB/state on durable volumes
- Reduce Docker socket exposure (or use socket proxy)
- After changes, run `GET /api/deploy-check/report` and `GET /supervisor/health` with an allowlist-matching forwarded identity
