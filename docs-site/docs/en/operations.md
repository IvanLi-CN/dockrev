---
title: Operations
description: Production operations, backup/restore, and upgrade/rollback.
---

# Operations

## Daily checks

1. Check anonymous `/api/health` and `/api/version`.
2. Check `/api/deploy-check/report` and `/supervisor/health` with an allowlist-matching identity.
3. Review Queue for unusually long `running` jobs.
4. If GHCR webhooks are enabled, verify there are no streaks of failed deliveries.

## Health checks

- Anonymous liveness: `GET /api/health`, `GET /api/version`
- Operator preflight: `GET /api/deploy-check/report`
- Supervisor health: `GET /supervisor/health`
- Anonymous `401 auth_required` on protected routes is expected in the transparent Forward Auth model; treat it as an application boundary signal, not a gateway outage.

## Backup strategy

Back up at least:

1. SQLite DB (`DOCKREV_DB_PATH`)
2. Supervisor state file (`DOCKREV_SUPERVISOR_STATE_PATH`)
3. Deployment compose files and environment config

## Backup execution guidance

- Run during off-peak hours
- Avoid launching risky updates during backup windows
- Validate restore periodically

## Restore sequence

1. Stop Dockrev and Supervisor
2. Restore DB and state file
3. Restart services
4. Verify Queue/Overview consistency

## Upgrade and rollback

- Standard upgrade: change image tag and run `docker compose up -d`
- Self-upgrade: use supervisor apply/dry-run/rollback APIs
- Failure rollback: revert to previous stable image + restore latest valid backup if needed

## Observability suggestions

- Keep job logs for discovery/check/update
- Monitor spikes in `401`, `409`, `500`
- Watch webhook delivery dedup behavior for GHCR integration
- Raw resource samples are retained for 7 days; terminal jobs and their logs for 30 days. Startup and hourly bounded GC perform deletion without automatically running SQLite `VACUUM`.
- Reclaim SQLite file pages with `VACUUM` only in an approved maintenance window after confirming backups and acceptable service interruption.

## Shared testbox regression (optional)

`scripts/testbox/*.e2e.ts` can validate key flows on the shared test machine:

- Check job concurrency and restart recovery
- Version inference SSE continuity
