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
2. Verify reverse proxy can inject Forward Auth user/group headers.
3. Start compose and verify health endpoints.
4. Run discovery/check from UI.

## Production essentials

- Set `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false`
- Inject `DOCKREV_AUTH_FORWARD_HEADER_NAME` and, if used, `DOCKREV_AUTH_GROUP_HEADER_NAME` via your gateway
- Set at least one of `DOCKREV_AUTH_ALLOWED_USER` or `DOCKREV_AUTH_ALLOWED_GROUP`
- Persist DB (`DOCKREV_DB_PATH`) and supervisor state
- Mount compose files as read-only at the same absolute host paths

## Copyable Traefik + Authelia example

The repo includes a production-oriented Forward Auth example you can copy directly:

- `deploy/examples/traefik-authelia/docker-compose.yml`
- `deploy/examples/traefik-authelia/authelia/configuration.yml`
- `deploy/examples/traefik-authelia/authelia/users.yml`
- `deploy/examples/traefik-authelia/README.md`

This example keeps protected Dockrev routes behind one Traefik `forwardAuth` middleware and splits webhook endpoints with dedicated Traefik routers, so you do not need Authelia `bypass` rules for Dockrev pages or protected APIs.

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
- `GET /api/deploy-check/report` returns the preflight result; when authorization fails it still returns an auth-only report
- `GET /supervisor/health` is reachable through the gateway
