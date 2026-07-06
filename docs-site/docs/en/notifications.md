---
title: Notifications
description: Dockrev notification types, payload schema, and URL/linking rules.
---

# Notifications

This page documents **which notifications Dockrev sends**, the **payload format** (v2 schemas), and how to configure the **Instance Public Base URL** so Telegram / Email / Web Push / Webhook messages can include clickable links to your Dockrev instance.

## Configure: Instance Public Base URL (for notification links)

Purpose: turn internal paths (e.g. `queue/{jobId}`) into absolute URLs (e.g. `https://dockrev.example.com/queue/job_...`).

- Web UI: `Settings -> System settings -> Instance Public Base URL`
- API:
  - `GET /api/settings -> instance.publicBaseUrl`
  - `PUT /api/settings -> instance.publicBaseUrl`
- Validation / normalization:
  - `null` or empty string clears the setting
  - when non-empty, it must be an absolute `http://` or `https://` URL
  - persisted value is trimmed and forced to end with `/`
- Fallback when not configured:
  - `links.*Url` will fall back to internal paths (starting with `/`)
  - Telegram / Email will show paths as code (not clickable) and include a hint to configure the base URL
  - Web Push will still open the path under the current origin
- Telegram dynamic cards do not depend on the Public Base URL; it only controls whether caption/detail links are clickable.

## Notification Types

| kind | schema | Trigger (high level) |
| --- | --- | --- |
| `job_finished` | `dockrev.notification.job.v2` | An update job finishes (success / failed / rolled back) and is not filtered out |
| `new_version_discovered` | `dockrev.notification.new_version_discovered.v2` | Scheduled checks or GHCR-webhook-triggered checks discover new versions (aggregated per check job) |
| `ghcr_webhook_anomaly` | `dockrev.notification.ghcr_webhook_anomaly.v2` | Scheduled GHCR webhook audit (`audit_all`) detects missing/conflict/error repos |
| `notification_test` | `dockrev.notification.test.v2` | `POST /api/notifications/test` |

> Webhook receivers should always route by `schema` (the switch to `dockrev.notification.job.v2` is a breaking change).

## Event switches in Settings

Path: `Settings -> Notifications`

Per-event switches:

- `Update finished notification` -> controls `job_finished`
- `New version discovered (scheduled / webhook check)` -> controls `new_version_discovered`
- `GitHub webhook anomaly (audit)` -> controls `ghcr_webhook_anomaly`

Behavior:

- If an event switch is off, that event is not sent to any channel.
- Delivery requires both switches to be on: event switch **and** channel switch.

## Job finished (`dockrev.notification.job.v2`)

### Fields (v2)

Top-level:

- `schema`: `dockrev.notification.job.v2`
- `kind`: `job_finished`
- `sentAt`: RFC3339 timestamp (string)
- `channel`: `telegram` / `email` / `webhook` / `webPush`
- `job`: job metadata
- `links`: URLs inside your Dockrev instance (the important part)
- `human`: Chinese title/summary/detail used for rendering
- `debug`: build/version metadata

`links` example:

```json
{
  "primaryUrl": "https://dockrev.example.com/services/stk_.../svc_...",
  "jobUrl": "https://dockrev.example.com/queue/job_...",
  "serviceUrls": [
    {
      "stackId": "stk_...",
      "stackName": "blog",
      "serviceId": "svc_...",
      "serviceName": "api",
      "url": "https://dockrev.example.com/services/stk_.../svc_..."
    }
  ],
  "truncated": { "serviceUrlsOmitted": 0 }
}
```

### URL / click rules

Internal routes:

- Job detail: `/queue/{jobId}`
- Service detail: `/services/{stackId}/{serviceId}`

When `instance.publicBaseUrl` is configured, Dockrev produces absolute URLs; otherwise it falls back to internal paths.

`primaryUrl` selection:

1. If the update can be uniquely mapped to **one service**, `primaryUrl = serviceUrl`
2. Otherwise, `primaryUrl = jobUrl`

Uniquely mapped means:

- the job scope is `service` and the job record includes `serviceId`, OR
- the update summary indicates exactly one changed service

### Truncation

- `links.serviceUrls` is capped at **10 entries**
- the omitted count is stored in `links.truncated.serviceUrlsOmitted`
- error excerpts (when present) are truncated to avoid overly long Telegram/Email messages

## New version discovered (`dockrev.notification.new_version_discovered.v2`)

Trigger: scheduled update checks and GHCR-webhook-triggered service checks. A notification is sent when a check run discovers new versions. UI-triggered manual checks stay silent.

Dispatch timing, dedupe, and lifecycle:

- the check job is finalized as `success` before any notification wait starts; only dispatch is delayed
- services that still depend on floating-tag inference primarily wait for an explicit `snapshot_worker task_finished` event before one final display-tag recompute; the fixed **10 second** cap is now only a safety fallback when the event/result does not arrive cleanly
- when a digest snapshot has already finished but `tags[]` still cannot produce a readable version, notifications also try OCI `org.opencontainers.image.version` as a display-tag fallback while leaving raw tag fields untouched for compatibility
- active dedupe is keyed by `serviceId + candidateDigest`
- only `pending` / `sent` records block repeats
- when the candidate disappears, the candidate digest changes, or the service baseline `imageRef:imageTag` changes, the previous active record is marked `superseded`
- when every enabled delivery channel fails, the record is finalized as `failed` and later jobs may retry

Key fields:

- `check.jobId`: check job id
- `check.servicesChecked`: total services checked
- `check.newVersions`: number of services with newly discovered versions
- `links.jobUrl`: `/queue/{jobId}`
- `links.serviceUrls[]`: `/services/{stackId}/{serviceId}`
- `links.serviceUrls[].currentTag` / `candidateTag`: raw tags kept for backward compatibility
- `links.serviceUrls[].currentDisplayTag` / `candidateDisplayTag`: priority is `snapshot inference > frozen/live resolved tag > OCI explicit version > raw tag`; raw fields remain unchanged for compatibility
- `links.primaryUrl`: service URL when exactly one service is affected, otherwise job URL
- `human.title`: a compact headline for the Email subject / Web Push title; single-service payloads use the service name, while multi-service payloads use an aggregate count
- `human.summary`: the human-facing copy is tightened without changing the schema version

Human-readable summary rules:

- single service, both sides readable: `blog / api has a new version available (1.0.0 -> 1.1.0).`
- single service, only one side readable: `blog / api has a new version available (1.0.0 -> latest).`
- single service, both sides still unreadable after settling: `blog / api has a new version available.` (never `latest -> latest`)
- multi-service aggregate:
  ```text
  3 services have new versions available:
  - blog / api
  - blog / worker (1.0.0 -> 1.1.0)
  - shop / gateway (2.4.0 -> 2.5.0)
  ```

Channel rendering rules:

- Telegram / Email (single service): start directly with the summary sentence and keep only one `Service details` action; no generic banner title, no trailing `Details` suffix, no `Service list` block, and no duplicate link list
- Telegram / Email (multi service): keep the aggregate notification, but render one service per line; unreadable entries show only the service name without a transition suffix
- Web Push: `title` uses the compact `human.title` headline, `body` is exactly `human.summary`, and the click target still uses `links.primaryUrl`

Truncation:

- `links.serviceUrls` keeps at most 10 entries
- overflow is reported via `links.truncated.serviceUrlsOmitted`

## GHCR webhook anomaly (`dockrev.notification.ghcr_webhook_anomaly.v2`)

Trigger: scheduled GHCR webhook audit (`audit_all`) only. Sent when missing/conflict/error repos are found.

Key fields:

- `job.id`: audit job id
- `job.missing/conflict/error`: anomaly counts
- `links.settingsUrl`: settings page URL for remediation
- `links.jobUrl`: job detail URL for audit logs
- `links.primaryUrl`: defaults to `links.jobUrl` (Web Push click target)
- `links.repos[]`: failing repo list with state and error excerpt
- `human.summary`: directly lists visible failing repos (example: `Found 2 anomalous repos: acme/api [missing], acme/worker [error].`)
- Telegram first line includes a clickable task link: `Dockrev: GitHub Webhook audit anomaly 任务`.

Truncation:

- `links.repos` keeps at most 10 entries
- overflow is reported via `links.truncated.reposOmitted`

## Notification test (`dockrev.notification.test.v2`)

- `url` points to the settings page (`/settings`) to demonstrate a clickable link
- Web Push payload additionally contains top-level `title` / `body` / `url`

## Channel rendering

### Telegram (dynamic card + HTML details)

Telegram notifications first send a content-related PNG card instead of relying on Telegram-generated link previews. The card uses a theme-neutral light information layout and does not assume the Telegram client or Dockrev Web UI is in dark mode. The card only includes summary-level information:

- notification type, status, and key object
- task id, scope/reason, check count, anomaly count, version transition, or up to 5 summary rows
- omitted count when the underlying list is longer

The photo caption stays short and includes the primary action link. Full service lists, full URLs, digests, error excerpts, and debug JSON remain in follow-up text details when needed.

Every Telegram `sendMessage` text path disables link previews so task, service, and settings links do not auto-expand. If card rendering or `sendPhoto` upload fails, Dockrev falls back to the existing text details; if that fallback succeeds, the channel is still treated as delivered and the job log records a photo fallback diagnostic.

Privacy boundary:

- The image does not render full URLs, digests, long error stacks, or debug JSON.
- Public Base URL only affects clickable links in the caption/details, not card generation.

### Email (multipart)

Subject includes status (no internal job id). Body is sent as both HTML and plain text.

### Webhook (JSON)

Raw JSON payload is POSTed. Route by `schema`:

- `dockrev.notification.job.v2`
- `dockrev.notification.new_version_discovered.v2`
- `dockrev.notification.ghcr_webhook_anomaly.v2`
- `dockrev.notification.test.v2`

### Web Push

The payload includes `title` / `body` / `url`. Clicking opens `url` (service page, job page, or settings page depending on event type).
