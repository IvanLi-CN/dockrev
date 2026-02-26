---
title: Deployment
description: Production deployment guide for Dockrev with reverse proxy.
---

# Deployment

## Minimal topology

`deploy/docker-compose.yml` includes three services:

- `gateway` (nginx): external entrypoint
- `dockrev`: API + embedded UI
- `supervisor`: self-upgrade executor and console

## Recommended rollout steps

1. Prepare data directory and Docker credentials file.
2. Verify reverse proxy can inject forward auth header.
3. Start compose and verify health endpoints.
4. Run discovery/check from UI.

## Production essentials

- Set `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false`
- Inject `DOCKREV_AUTH_FORWARD_HEADER_NAME` via your gateway
- Persist DB (`DOCKREV_DB_PATH`) and supervisor state
- Mount compose files as read-only at the same absolute host paths

## Use released images

```yaml
services:
  dockrev:
    image: ghcr.io/ivanli-cn/dockrev:<semver>
  supervisor:
    image: ghcr.io/ivanli-cn/dockrev-supervisor:latest
```

Notes:

- `latest` is updated by stable release workflow only.
- Prefer `0.3.5+` to avoid historical executable-bit issues.

## Paths and proxy routing

- Dockrev app: `/` and `/api/*`
- Supervisor app: `/supervisor/*`
- Self-upgrade jump path: `DOCKREV_SELF_UPGRADE_URL` (default `/supervisor/`)

## Deployment checks

- `GET /api/health` returns `ok`
- `GET /api/deploy-check/report` returns preflight result
- `GET /supervisor/health` is reachable through the gateway
