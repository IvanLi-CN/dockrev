---
title: Integrations
description: GHCR webhook integration, notifications, and external triggers.
---

# Integrations

## GitHub Packages (GHCR) webhook

Use GH package events (`package.published`) to trigger Dockrev discovery automatically.

### Settings field guide (Settings -> GitHub Packages (GHCR) Webhook)

| Field | Purpose | Notes |
| --- | --- | --- |
| Enable | Turns GHCR webhook integration on/off. | When disabled, Dockrev will not sync or consume GHCR webhooks. |
| GitHub PAT (leave empty to keep current) | Used to resolve owner/repo targets and sync repository webhooks. | Empty input does not clear saved PAT; enter a new PAT and save to rotate it. |
| Callback URL | Webhook endpoint used by GitHub. | Must be publicly reachable HTTPS, typically `https://<your-domain>/api/webhooks/github-packages`. |
| Repos / Add Repo | Manage tracked repositories. | Accepts `owner/repo`, `org/repo`, `https://github.com/org/repo`, `https://github.com/<owner>`. |
| Resolve and Add | Resolves input to repository candidates and appends them. | Depends on PAT scope and GitHub API reachability. |
| Search owner/repo | Filter the tracked repository list. | UI-only filtering; does not change webhook config itself. |
| Selected state | Marks repos that should participate in webhook sync. | Only selected repos get created/updated webhooks. |

### Setup steps (recommended)

1. Open Settings -> GitHub Packages (GHCR) Webhook.
2. Enable the feature, set GitHub PAT, verify callback URL, then click **Save Settings**.
3. Use **Add Repo** with a repo/owner input and click **Resolve and Add**.
4. Select repositories to track (`selected=true`).
5. Run webhook sync and verify `created/noop/updated` results.
6. In GitHub repository settings, confirm webhook entries now point to Dockrev.
7. Publish a GHCR package (`package.published`) and confirm discovery jobs appear in Dockrev Queue/logs.

### PAT capability requirements

- List repos for target owners
- Manage repository webhooks on selected repos

### Callback reachability checks

- Callback URL must be reachable from the public internet (private LAN URLs do not work).
- Your reverse proxy must preserve `POST /api/webhooks/github-packages`.
- `curl` without a GitHub signature often returns `400/401`; this is expected.

### Common failures

- Resolve/Add returns empty results: PAT invalid/insufficient or GitHub API unreachable.
- Sync finishes but tracked repo count is still 0: repos were not selected, or settings were not saved first.
- `401 invalid_signature`: webhook secret mismatch/signature failure
- `422`: PAT missing or insufficient permission
- `conflict`: duplicate webhook entries detected; resolve and retry

## Notification channels

Dockrev supports:

- Webhook
- Telegram
- Email (`smtpUrl` with `to/from` query)
- Web Push (VAPID)

### Generate VAPID keys

```bash
bunx web-push generate-vapid-keys --json
```

Then configure keys in Settings and test browser subscription.

## External trigger webhook

`/api/webhooks/trigger` can be used by external systems to trigger check/update.

- Header: `X-Dockrev-Webhook-Secret`
- Body: `action`, `scope`, optional `stackId/serviceId`
