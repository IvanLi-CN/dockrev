---
title: Operations
description: Production operations, backup/restore, and upgrade/rollback.
---

# Operations

## Health checks

- API health: `GET /api/health`
- API version: `GET /api/version`
- Deploy preflight: `GET /api/deploy-check/report`
- Supervisor health: `GET /supervisor/health`

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

## Shared testbox regression (optional)

`scripts/testbox/*.e2e.ts` can validate key flows on the shared test machine:

- Check job concurrency and restart recovery
- Version inference SSE continuity
