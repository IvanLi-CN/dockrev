# Traefik + Authelia example

This example keeps Dockrev's local `deploy/docker-compose.yml` unchanged and provides a production-oriented transparent identity-forwarding stack you can copy.

## What this example does

- Traefik handles ingress and TLS. Use `traefik:v3.6.1` or newer when you rely on the Docker provider.
- Traefik sends every Dockrev and Supervisor request through the same Forward Auth middleware; there is no webhook split router.
- Authelia provides the identity portal and trusted response headers, but it does not enforce Dockrev-specific user/group/path ACL in this topology.
- Dockrev and Supervisor decide which routes are public and which forwarded identities are allowed.
- Anonymous public routes stay limited to:
  - `GET /api/health`
  - `GET /api/version`
  - `/api/webhooks/*`
- Dockrev still validates webhook requests itself:
  - `/api/webhooks/trigger` via `DOCKREV_WEBHOOK_SECRET`
  - `/api/webhooks/github-packages` via GitHub signature

## Before you start

Replace these placeholders in both `docker-compose.yml` and `authelia/configuration.yml`:

- `dockrev.example.com`
- `auth.example.com`
- `example.com`
- `admin@example.com`
- `ghcr.io/ivanli-cn/dockrev:<semver>`
- every `change-me-*` secret

Also replace the password hash in `authelia/users.yml`.

## Password hash

Generate a password hash with the official Authelia image:

```bash
docker run --rm authelia/authelia:4.39 authelia crypto hash generate argon2 --password 'change-me' | sed 's/^Digest: //'
```

Then paste the generated digest into the `password:` field in `authelia/users.yml`. If you keep the `Digest: ` prefix, wrap the whole value in quotes.

## Secrets

For a quick start, the example keeps secrets inline in `configuration.yml`. For a long-lived production deployment, move them to files or your secret manager.

## Start

```bash
cd deploy/examples/traefik-authelia
mkdir -p data/traefik data/authelia data/dockrev data/supervisor
: > data/traefik/acme.json
chmod 600 data/traefik/acme.json

docker compose up -d
```

## Transparent ingress contract

- Sign in through `https://auth.example.com` when you want Authelia to attach `Remote-User` / `Remote-Groups`.
- Requests without an Authelia session still reach Dockrev and Supervisor.
- Protected APIs, protected UI routes, `/api/deploy-check/report`, and `/supervisor/*` return Dockrev-generated `401 auth_required` unless the forwarded identity matches `DOCKREV_AUTH_ALLOWED_USER` or `DOCKREV_AUTH_ALLOWED_GROUP`.
- The gateway does not perform Dockrev-specific user/group/path ACL.

## Authorization examples

Allow a single user:

```yaml
environment:
  DOCKREV_AUTH_ALLOWED_USER: alice
```

Allow a single group:

```yaml
environment:
  DOCKREV_AUTH_ALLOWED_GROUP: dockrev-users
```

Allow either one:

```yaml
environment:
  DOCKREV_AUTH_ALLOWED_USER: alice
  DOCKREV_AUTH_ALLOWED_GROUP: dockrev-users
```

## Why there is no webhook split or gateway ACL

Dockrev owns the boundary: the application itself decides which routes stay anonymous and which requests must return `401 auth_required`. Traefik only routes traffic and forwards trusted identity headers from Authelia when a session exists.
