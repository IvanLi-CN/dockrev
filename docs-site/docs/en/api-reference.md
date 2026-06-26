---
title: API Reference (Complete)
description: Full endpoint inventory for Dockrev API and Supervisor API.
---

# API Reference (Complete)

This page documents every HTTP route exposed in:

- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-supervisor/src/app.rs`

## Authentication model

- **Public**: no auth header required.
- **Forward Auth**: the ingress may forward trusted identity headers from the upstream auth system, but Dockrev still decides whether a protected route returns data or `401 auth_required`.
- **Dockrev authorization**: Dockrev reads `DOCKREV_AUTH_ALLOWED_USER` and `DOCKREV_AUTH_ALLOWED_GROUP`; each accepts a single value, and matching either one is enough when both are set.
- **Webhook Secret**: requires `X-Dockrev-Webhook-Secret` matching server config.
- **GitHub Signature**: requires `X-Hub-Signature-256`, `X-GitHub-Event`, and `X-GitHub-Delivery`.

---

## Dockrev API (`/api/*`)

### 1) Core

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| GET | `/api/health` | Public | Health probe | `200` |
| GET | `/api/version` | Public | Effective app version | `200` |

### 2) Stacks / Services / Version inference

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| GET | `/api/stacks` | Forward Auth | List stacks (with archived filter) | `200` `401` `400` |
| POST | `/api/stacks` | Forward Auth | Manual register endpoint is disabled | `400/405` `401` |
| GET | `/api/stacks/{stack_id}` | Forward Auth | Get stack detail | `200` `404` `401` |
| POST | `/api/stacks/{stack_id}/archive` | Forward Auth | Archive stack | `200` `404` `401` |
| POST | `/api/stacks/{stack_id}/restore` | Forward Auth | Restore stack | `200` `404` `401` |
| POST | `/api/services/{service_id}/archive` | Forward Auth | Archive service | `200` `404` `401` |
| POST | `/api/services/{service_id}/restore` | Forward Auth | Restore service | `200` `404` `401` |
| GET | `/api/services/{service_id}/digest-tags` | Forward Auth | Fetch the digest-tags debug view (now snapshot-backed; returns `202 pending` while the snapshot is missing or refreshing) | `200` `202` `404` `401` |
| GET | `/api/services/{service_id}/digest-tags-snapshot` | Forward Auth | Fetch digest-tag snapshot (`202 pending` while the target digest is refreshing) | `200` `202` `404` `401` |
| POST | `/api/services/{service_id}/version-inference/refresh` | Forward Auth | Trigger digest-scoped version inference refresh (`digest` body required) | `202` `400` `404` `401` |
| GET | `/api/version-inference/overview` | Forward Auth | Version inference overview | `200` `401` |
| GET | `/api/version-inference/events` | Forward Auth | Version inference SSE stream | `200` `401` |

### 3) Discovery / Check / Runtime scan / Update

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| POST | `/api/discovery/scan` | Forward Auth | Trigger discovery scan job | `200` `401` |
| GET | `/api/discovery/projects` | Forward Auth | List discovery projects | `200` `401` |
| POST | `/api/discovery/projects/{project}/archive` | Forward Auth | Archive discovery project | `200` `404` `401` |
| POST | `/api/discovery/projects/{project}/restore` | Forward Auth | Restore discovery project | `200` `404` `401` |
| POST | `/api/checks` | Forward Auth | Create check job | `200` `400` `401` `409` |
| POST | `/api/runtime-scans` | Forward Auth | Create runtime scan job | `200` `400` `401` `409` |
| POST | `/api/updates` | Forward Auth | Create update job | `200` `400` `401` `409` |

- `POST /api/updates` contract:
  - `scope=service`: requires explicit `serviceId + targetTag + targetDigest + pullTags`.
  - `scope=stack|all`: requires explicit `targets[]`, each entry shaped as `{ serviceId, targetTag, targetDigest, pullTags }`.

### 4) Jobs / Events

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| GET | `/api/jobs` | Forward Auth | List jobs | `200` `401` |
| GET | `/api/jobs/events` | Forward Auth | Job SSE stream | `200` `401` |
| GET | `/api/jobs/{job_id}` | Forward Auth | Get single job | `200` `404` `401` |
| GET | `/api/jobs/{job_id}/events` | Forward Auth | Single job SSE stream | `200` `404` `401` |

### 5) Ignores / Service settings / Notifications / Settings

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| GET | `/api/ignores` | Forward Auth | List ignore rules | `200` `401` |
| POST | `/api/ignores` | Forward Auth | Create ignore rule | `200` `400` `401` |
| DELETE | `/api/ignores` | Forward Auth | Delete ignore rule | `200` `400` `401` |
| GET | `/api/services/{service_id}/settings` | Forward Auth | Get service settings | `200` `404` `401` |
| PUT | `/api/services/{service_id}/settings` | Forward Auth | Update service settings | `200` `400` `404` `401` |
| GET | `/api/notifications` | Forward Auth | Read notification settings (`botToken` omitted; `botTokenConfigured` + plain `chatId`) | `200` `401` |
| PUT | `/api/notifications` | Forward Auth | Update notification settings | `200` `400` `401` |
| POST | `/api/notifications/test` | Forward Auth | Send test notification | `200` `400` `401` |
| GET | `/api/settings` | Forward Auth | Get system settings | `200` `401` |
| PUT | `/api/settings` | Forward Auth | Update system settings | `200` `400` `401` |

- Starting `2026-03-05`, downstream payloads emitted by `POST /api/notifications/test` use **v2 (breaking)**:
  - legacy `type/ts/message` fields are removed;
  - new base structure is `schema/kind/sentAt/channel/human/debug`;
  - Web Push adds `title/body` as display-oriented top-level fields for service-worker notifications;
  - rendering is capability-aware: Telegram/Email include code blocks, Web Push is natural-language only (no code blocks).

### 6) GitHub Packages (GHCR) integration

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| GET | `/api/github-packages/settings` | Forward Auth | Read GHCR webhook settings (masked PAT) | `200` `401` |
| PUT | `/api/github-packages/settings` | Forward Auth | Update GHCR webhook settings | `200` `400` `401` |
| GET | `/api/github-packages/repos` | Forward Auth | List tracked repos with paging/filter | `200` `401` |
| POST | `/api/github-packages/repos/selected` | Forward Auth | Set `selected` for a repo | `200` `400` `401` |
| POST | `/api/github-packages/repos/delete` | Forward Auth | Remove tracked repo entry | `200` `400` `401` |
| POST | `/api/github-packages/repos/bulk-selected` | Forward Auth | Bulk update `selected` flags | `200` `400` `401` |
| POST | `/api/github-packages/targets/add` | Forward Auth | Add target input (repo/owner) | `200` `400` `401` |
| POST | `/api/github-packages/targets/remove` | Forward Auth | Remove target input | `200` `400` `401` |
| POST | `/api/github-packages/resolve` | Forward Auth | Resolve input to repo candidates | `200` `400` `401` `422` |
| POST | `/api/github-packages/sync` | Forward Auth | Sync webhook state with GitHub | `200` `400` `401` |

### 7) Web Push / Webhooks / Deploy checks

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| POST | `/api/web-push/subscriptions` | Forward Auth | Create/update web push subscription | `200` `400` `401` |
| DELETE | `/api/web-push/subscriptions` | Forward Auth | Delete web push subscription | `200` `400` `401` |
| POST | `/api/webhooks/trigger` | Webhook Secret | External trigger for check/update jobs (`action=update` requires explicit `targets[]`) | `200` `400` `401` |
| POST | `/api/webhooks/github-packages` | GitHub Signature | Receive GH package webhooks and enqueue matching `check.service` jobs, or one discovery fallback when nothing matches | `200` `202` `400` `401` |
| GET | `/api/deploy-check/report` | Forward Auth | Fetch the deploy-check report snapshot; cached responses may include `refreshing=true`, and cache-miss / not-ready states return `202 pending` | `200` `202` `401` |
| POST | `/api/deploy-check/report/refresh` | Forward Auth | Enqueue a deploy-check report refresh and return a `pending` envelope | `202` `401` |
| GET | `/api/deploy-welcome` | Forward Auth | Get deploy welcome status | `200` `401` |
| PUT | `/api/deploy-welcome` | Forward Auth | Update deploy welcome status | `200` `400` `401` |

---

## Supervisor API (default base path `/supervisor`)

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| GET | `/supervisor/health` | Forward Auth | Supervisor health probe (protected) | `200` `401` |
| GET | `/supervisor/version` | Forward Auth | Supervisor metadata (`version` + `repository` + `developerName` + `developerUrl`) | `200` `401` |
| GET | `/supervisor/self-upgrade` | Forward Auth | Read self-upgrade state | `200` `401` |
| POST | `/supervisor/self-upgrade` | Forward Auth | Start self-upgrade (dry-run/apply) | `200` `400` `401` `409` |
| POST | `/supervisor/self-upgrade/rollback` | Forward Auth | Roll back operation | `200` `400` `401` |
| GET | `/supervisor/favicon.png` | Public | UI favicon | `200` |
| GET | `/supervisor/` | Forward Auth | Supervisor web UI | `200` `401` |

- `GET /supervisor/self-upgrade` now includes an optional `request` object (`mode` + `rollbackOnFailure`) while `state=running`, so the UI can restore which action button is actively running after refresh. The field may be omitted for idle/legacy states.

---

## Request examples

### Trigger all-scope check

```bash
curl -X POST \
  -H 'Content-Type: application/json' \
  -H 'X-Forwarded-User: ops' \
  -d '{"scope":"all"}' \
  http://127.0.0.1:50883/api/checks
```

### Trigger update from external webhook

```bash
curl -X POST \
  -H 'Content-Type: application/json' \
  -H 'X-Dockrev-Webhook-Secret: change-me' \
  -d '{"action":"update","scope":"stack","stackId":"stk_xxx","targets":[{"serviceId":"svc_web","targetTag":"latest","targetDigest":"sha256:abc123","pullTags":["v1.1.2"]}],"allowArchMismatch":false,"backupMode":"inherit"}' \
  http://127.0.0.1:50883/api/webhooks/trigger
```

> Note: `POST /api/webhooks/trigger` rejects legacy `action=update` payloads that omit `targets[]` with `400 invalid_argument`.
> webhook update also supports `scope=service`, but you must still provide `serviceId` and a matching `{ serviceId, targetTag, targetDigest, pullTags }` entry inside `targets[]`.

### Read supervisor state

```bash
curl -H 'X-Forwarded-User: ops' \
  http://127.0.0.1:50883/supervisor/self-upgrade
```
