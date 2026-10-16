# Dockrev：服务端自主自检与无阻塞网页入口实现状态

## Current Status

- Implementation: complete and ready for review on `th/server-owned-self-checks`.
- Lifecycle: active.
- Source decision: [ADR 0016](../../adr/0016-server-owned-self-checks.md).

## Implementation Coverage

- Backend worker owns startup scheduling, five-minute review, relevant-input coalescing, bounded confirmation and report metadata.
- Deploy-check HTTP handlers keep GET read-only and POST explicit/manual, while preserving old envelope fields and old snapshot readability.
- Core failure and operation impact projections are separate from the complete diagnostic check list and per-request authorization diagnostics.
- The web app renders its target route immediately, uses the first read only for a one-time current-startup failure claim, and uses notices plus scoped impacts for late/background changes.
- The diagnostic page keeps browsing, settings, discovery and manual review reachable; IndexedDB claims coordinate same-origin tabs.

## Compatibility Record

- `public_api`: patch impact. Existing GET/POST paths and envelope fields remain; new report fields are additive and optional.
- `persistent_state`: patch impact. No database schema migration; metadata is embedded in report JSON and old snapshots are treated as historical diagnostics.
- The detailed release-unit record is [version-impact-record.json](./version-impact-record.json).

## Validation

- Backend focused tests cover worker scheduling, API read-only behavior, report metadata, impact projection and operation boundaries.
- Frontend tests cover entry rendering, zero automatic POSTs, late/background failure notices, IndexedDB claim races and diagnostic-page actions.
- Web lint, build, Storybook interaction tests and owner-approved mock-only desktop/mobile evidence passed for Step 5C readiness.
- Docker, Compose and heavy integration validation passed through the session-owned Agent VM deployment smoke; the VM guest had no native Rust/Node toolchains, so native full-suite evidence remains local.

## References

- `./SPEC.md`
- `./HISTORY.md`
- `../../adr/0016-server-owned-self-checks.md`
