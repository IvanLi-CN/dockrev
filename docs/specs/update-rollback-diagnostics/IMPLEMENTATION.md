# Dockrev：自动更新回滚诊断实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖与 rollout 相关事实。

## Current Status

- Implementation: complete locally; delivery gates remain
- Lifecycle: active
- Catalog note: `docs/specs/README.md` records this topic in the canonical catalog.

## Coverage / Rollout Summary

- Runtime code, migration, API, UI, and recovery paths are implemented on the locked fast-track branch.
- Production configuration, deployment, and update retry remain explicitly out of scope.

## Implementation Order

1. Completed the nullable jobs BLOB migration and database methods for evidence metadata, archive storage, terminal-job retention, and recovery lookup.
2. Completed the private job spool and bounded, non-streamed candidate capture path with original bytes preserved.
3. Completed candidate effective-policy inspection and policy-derived health deadline calculation.
4. Completed pre-rollback capture, job-boundary `tar.zst` assembly, startup recovery, and terminal cleanup integration.
5. Completed job summary metadata and the authorized archive download endpoint.
6. Completed the Job Detail download affordance; focused and environment-dependent validation is tracked by the delivery gate.

## Remaining Gaps

- Focused Rust and web validation is complete locally; shared Docker and delivery checks remain environment gates.
- No production update has been retried, so the root cause of the historical candidate failure remains unproven.

## Related Changes

- Runtime: `crates/dockrev-api/src/rollback_evidence.rs`, updater, DB, API, and Job Detail integration.
- Data/API contracts: `./contracts/db.md`, `./contracts/http-api.md`.

## References

- `./SPEC.md`
- `./HISTORY.md`
