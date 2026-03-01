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
- **Forward Header**: requires `DOCKREV_AUTH_FORWARD_HEADER_NAME` (default `X-Forwarded-User`).
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
| GET | `/api/stacks` | Forward Header | List stacks (with archived filter) | `200` `401` `400` |
| POST | `/api/stacks` | Forward Header | Manual register endpoint is disabled | `400/405` `401` |
| GET | `/api/stacks/{stack_id}` | Forward Header | Get stack detail | `200` `404` `401` |
| POST | `/api/stacks/{stack_id}/archive` | Forward Header | Archive stack | `200` `404` `401` |
| POST | `/api/stacks/{stack_id}/restore` | Forward Header | Restore stack | `200` `404` `401` |
| POST | `/api/services/{service_id}/archive` | Forward Header | Archive service | `200` `404` `401` |
| POST | `/api/services/{service_id}/restore` | Forward Header | Restore service | `200` `404` `401` |
| GET | `/api/services/{service_id}/digest-tags` | Forward Header | Fetch digest-to-tags mapping | `200` `404` `401` |
| GET | `/api/services/{service_id}/digest-tags-snapshot` | Forward Header | Fetch digest-tag snapshot | `200` `404` `401` |
| POST | `/api/services/{service_id}/version-inference/refresh` | Forward Header | Trigger service-level version inference refresh | `200` `404` `401` |
| GET | `/api/version-inference/overview` | Forward Header | Version inference overview | `200` `401` |
| GET | `/api/version-inference/events` | Forward Header | Version inference SSE stream | `200` `401` |

### 3) Discovery / Check / Runtime scan / Update

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| POST | `/api/discovery/scan` | Forward Header | Trigger discovery scan job | `200` `401` |
| GET | `/api/discovery/projects` | Forward Header | List discovery projects | `200` `401` |
| POST | `/api/discovery/projects/{project}/archive` | Forward Header | Archive discovery project | `200` `404` `401` |
| POST | `/api/discovery/projects/{project}/restore` | Forward Header | Restore discovery project | `200` `404` `401` |
| POST | `/api/checks` | Forward Header | Create check job | `200` `400` `401` `409` |
| POST | `/api/runtime-scans` | Forward Header | Create runtime scan job | `200` `400` `401` `409` |
| POST | `/api/updates` | Forward Header | Create update job | `200` `400` `401` `409` |

### 4) Jobs / Events

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| GET | `/api/jobs` | Forward Header | List jobs | `200` `401` |
| GET | `/api/jobs/events` | Forward Header | Job SSE stream | `200` `401` |
| GET | `/api/jobs/{job_id}` | Forward Header | Get single job | `200` `404` `401` |
| GET | `/api/jobs/{job_id}/events` | Forward Header | Single job SSE stream | `200` `404` `401` |

### 5) Ignores / Service settings / Notifications / Settings

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| GET | `/api/ignores` | Forward Header | List ignore rules | `200` `401` |
| POST | `/api/ignores` | Forward Header | Create ignore rule | `200` `400` `401` |
| DELETE | `/api/ignores` | Forward Header | Delete ignore rule | `200` `400` `401` |
| GET | `/api/services/{service_id}/settings` | Forward Header | Get service settings | `200` `404` `401` |
| PUT | `/api/services/{service_id}/settings` | Forward Header | Update service settings | `200` `400` `404` `401` |
| GET | `/api/notifications` | Forward Header | Read notification settings (masked secrets) | `200` `401` |
| PUT | `/api/notifications` | Forward Header | Update notification settings | `200` `400` `401` |
| POST | `/api/notifications/test` | Forward Header | Send test notification | `200` `400` `401` |
| GET | `/api/settings` | Forward Header | Get system settings | `200` `401` |
| PUT | `/api/settings` | Forward Header | Update system settings | `200` `400` `401` |

### 6) GitHub Packages (GHCR) integration

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| GET | `/api/github-packages/settings` | Forward Header | Read GHCR webhook settings (masked PAT) | `200` `401` |
| PUT | `/api/github-packages/settings` | Forward Header | Update GHCR webhook settings | `200` `400` `401` |
| GET | `/api/github-packages/repos` | Forward Header | List tracked repos with paging/filter | `200` `401` |
| POST | `/api/github-packages/repos/selected` | Forward Header | Set `selected` for a repo | `200` `400` `401` |
| POST | `/api/github-packages/repos/delete` | Forward Header | Remove tracked repo entry | `200` `400` `401` |
| POST | `/api/github-packages/repos/bulk-selected` | Forward Header | Bulk update `selected` flags | `200` `400` `401` |
| POST | `/api/github-packages/targets/add` | Forward Header | Add target input (repo/owner) | `200` `400` `401` |
| POST | `/api/github-packages/targets/remove` | Forward Header | Remove target input | `200` `400` `401` |
| POST | `/api/github-packages/resolve` | Forward Header | Resolve input to repo candidates | `200` `400` `401` `422` |
| POST | `/api/github-packages/sync` | Forward Header | Sync webhook state with GitHub | `200` `400` `401` |

### 7) Web Push / Webhooks / Deploy checks

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| POST | `/api/web-push/subscriptions` | Forward Header | Create/update web push subscription | `200` `400` `401` |
| DELETE | `/api/web-push/subscriptions` | Forward Header | Delete web push subscription | `200` `400` `401` |
| POST | `/api/webhooks/trigger` | Webhook Secret | External trigger for check/update jobs | `200` `400` `401` |
| POST | `/api/webhooks/github-packages` | GitHub Signature | Receive GH package webhook and enqueue discovery | `200` `202` `400` `401` |
| GET | `/api/deploy-check/report` | Forward Header | Deployment preflight report | `200` `401` |
| GET | `/api/deploy-welcome` | Forward Header | Get deploy welcome status | `200` `401` |
| PUT | `/api/deploy-welcome` | Forward Header | Update deploy welcome status | `200` `400` `401` |

---

## Supervisor API (default base path `/supervisor`)

| Method | Path | Auth | Purpose | Key status codes |
| --- | --- | --- | --- | --- |
| GET | `/supervisor/health` | Public | Supervisor health probe | `200` |
| GET | `/supervisor/version` | Public | Supervisor metadata (`version` + `repository` + `developerName` + `developerUrl`) | `200` |
| GET | `/supervisor/self-upgrade` | Forward Header | Read self-upgrade state | `200` `401` |
| POST | `/supervisor/self-upgrade` | Forward Header | Start self-upgrade (dry-run/apply) | `200` `400` `401` `409` |
| POST | `/supervisor/self-upgrade/rollback` | Forward Header | Roll back operation | `200` `400` `401` |
| GET | `/supervisor/favicon.png` | Public | UI favicon | `200` |
| GET | `/supervisor/` | Public | Supervisor web UI | `200` |

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
  -d '{"action":"update","scope":"service","serviceId":"svc_xxx"}' \
  http://127.0.0.1:50883/api/webhooks/trigger
```

### Read supervisor state

```bash
curl -H 'X-Forwarded-User: ops' \
  http://127.0.0.1:50883/supervisor/self-upgrade
```
