---
title: Deployment
description: Production deployment guide for Dockrev with transparent Forward Auth ingress.
---

# Deployment

## Minimal topology

`deploy/docker-compose.yml` includes three services:

- `gateway` (nginx): external entrypoint
- `dockrev`: API + embedded UI
- `supervisor`: self-upgrade executor and console

## Recommended rollout steps

1. Prepare the data directory and Docker credentials file.
2. Verify the reverse proxy can inject trusted Forward Auth headers for signed-in users while still passing anonymous requests through to Dockrev.
3. Start compose and verify the anonymous health endpoints.
4. Use an allowlist-matching identity to complete discovery/check and deploy-check regression.

## Production essentials

- Set `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false`
- Inject `DOCKREV_AUTH_FORWARD_HEADER_NAME` and, if used, `DOCKREV_AUTH_GROUP_HEADER_NAME`
- Set at least one of `DOCKREV_AUTH_ALLOWED_USER` or `DOCKREV_AUTH_ALLOWED_GROUP`
- Persist DB (`DOCKREV_DB_PATH`) and supervisor state
- Mount compose files read-only at the same absolute host paths

## Copyable Traefik + Authelia example

The repo includes a production-oriented Forward Auth example you can copy directly:

- `deploy/examples/traefik-authelia/docker-compose.yml`
- `deploy/examples/traefik-authelia/authelia/configuration.yml`
- `deploy/examples/traefik-authelia/authelia/users.yml`
- `deploy/examples/traefik-authelia/README.md`

This example keeps all Dockrev traffic on normal service/path routers. Traefik does not split webhooks and does not enforce Dockrev-specific user/group/path ACL; Dockrev owns the public/protected boundary.

## Forward Auth (Traefik + Authelia)

### Responsibility split

- **Traefik**: routing, TLS, and Forward Auth subrequests.
- **Authelia**: session portal and trusted identity source; no Dockrev-specific ACL in this topology.
- **Dockrev / Supervisor**: public-route classification plus allowlist-based authorization.

### Transparent pass-through pattern

Keep the Dockrev domain transparent on the Authelia side:

```yaml
access_control:
  default_policy: deny
  rules:
    - domain: dockrev.example.com
      policy: bypass
```

Attach the same Forward Auth middleware to Dockrev and Supervisor routes in Traefik:

```yaml
http:
  middlewares:
    dockrev-forward-auth:
      forwardAuth:
        address: http://authelia:9091/api/authz/forward-auth
        trustForwardHeader: true
        authResponseHeaders:
          - Remote-User
          - Remote-Groups
```

Keep Dockrev's own authorization configured in the application:

```env
DOCKREV_AUTH_FORWARD_HEADER_NAME=Remote-User
DOCKREV_AUTH_GROUP_HEADER_NAME=Remote-Groups
DOCKREV_AUTH_ALLOWED_USER=
DOCKREV_AUTH_ALLOWED_GROUP=ops
DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV=false
```

Operationally this means:

- Operators sign in via `https://auth.example.com` when they want Authelia to attach `Remote-*` headers.
- Requests without a session still reach Dockrev; protected API/UI/supervisor routes return Dockrev-generated `401 auth_required` instead of a gateway ACL page.
- Dockrev treats `DOCKREV_AUTH_ALLOWED_USER` and `DOCKREV_AUTH_ALLOWED_GROUP` as single-value allowlists and accepts either match when both are set.

### Route boundary

- **Anonymous public API**: `GET /api/health`, `GET /api/version`, `/api/webhooks/*`
- **Protected API**: every other `/api/**`, including `GET /api/deploy-check/report`
- **Protected UI / Supervisor**: all business SPA routes and `/supervisor/*`
- **Webhook validation**: still handled inside Dockrev

## Use released images

```yaml
services:
  dockrev:
    image: ghcr.io/ivanli-cn/dockrev:latest
  supervisor:
    image: ghcr.io/ivanli-cn/dockrev-supervisor:latest
```

Notes:

- `latest` is updated by the stable release workflow only.
- Prefer `0.3.5+` to avoid historical executable-bit issues.
- If you want manual/immutable upgrades, switch `dockrev` back to a concrete semver tag; that pinned stack should not enable the stable-latest auto-deploy path below.

## Optional GitHub Actions stable latest auto-deploy

If you want the repository `Release` workflow to automatically apply the newest stable `latest` image to production after publishing succeeds, configure these repository-level variables and secrets:

- Variables:
  - `PRODUCTION_DEPLOY_HOST`
  - `PRODUCTION_DEPLOY_SSH_PORT` (optional, defaults to `22`)
  - `PRODUCTION_DEPLOY_USER`
  - `PRODUCTION_DEPLOY_STACK_DIR`
  - `PRODUCTION_DEPLOY_COMPOSE_FILE`
  - `PRODUCTION_DEPLOY_SERVICES` (optional, defaults to `dockrev supervisor`)
  - `PRODUCTION_DEPLOY_VERSION_URL`
- Secrets:
  - `PRODUCTION_DEPLOY_SSH_KEY`
  - `PRODUCTION_DEPLOY_SSH_KNOWN_HOSTS`

Notes:

- The job only runs for stable releases where `publish_latest=true`.
- If any required variable or secret is missing, the release still succeeds and the deploy job is marked as skipped in the workflow summary.
- If your production Compose service names differ from the minimal example (`dockrev` / `supervisor`), set `PRODUCTION_DEPLOY_SERVICES` explicitly.
- The target stack must already consume moving tags for the upgraded services (for example `ghcr.io/ivanli-cn/dockrev:latest` and `ghcr.io/ivanli-cn/dockrev-supervisor:latest`). If you pin `dockrev` to a semver tag for manual upgrades, do not enable this auto-deploy path.

## Paths and proxy routing

- Dockrev app: `/` and `/api/*`
- Supervisor app: `/supervisor/*`
- Self-upgrade jump path: `DOCKREV_SELF_UPGRADE_URL` (default `/supervisor/`)

## Deployment checks

- Anonymous `GET /api/health` and `GET /api/version` return `200`
- Anonymous `GET /api/deploy-check/report`, `GET /supervisor/health`, and `GET /supervisor/version` return Dockrev-generated `401 auth_required`
- An allowlist-matching forwarded identity can access deploy-check, the business UI, and `/supervisor/*`
