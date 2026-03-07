# Traefik + Authelia example

This example keeps Dockrev's local `deploy/docker-compose.yml` unchanged and provides a production-oriented Forward Auth stack you can copy.

## What this example does

- Traefik handles ingress and TLS.
- Authelia handles authentication via Traefik Forward Auth.
- Dockrev and Supervisor perform project-side authorization using `Remote-User` / `Remote-Groups`.
- Webhook endpoints are routed without Forward Auth middleware, so you do not need Authelia `bypass` rules for them.
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
docker run --rm authelia/authelia:4.39 authelia crypto hash generate argon2 --password 'change-me'
```

Then paste the generated hash into `authelia/users.yml`.

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

## Why webhooks are split at Traefik instead of Authelia bypass

This example keeps the protected application routes behind a single Forward Auth middleware and handles webhook openness in Traefik routing, not in Authelia policy rules. That keeps the Authelia policy simple while avoiding broad anonymous allowances for Dockrev pages or protected APIs.
