---
title: Integrations
description: GHCR webhook integration, notifications, and external triggers.
---

# Integrations

## GitHub Packages (GHCR) webhook

Use GH package events (`package.published`) to trigger Dockrev discovery automatically.

### Setup steps

1. Open Settings -> GitHub Packages (GHCR) Webhook.
2. Set GitHub PAT (masked when read back).
3. Set callback URL (must be public HTTPS).
4. Add targets (repo or owner) and select tracked repos.
5. Run webhook sync and verify `created/noop/updated` results.

### PAT capability requirements

- List repos for target owners
- Manage repository webhooks on selected repos

### Common failures

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
