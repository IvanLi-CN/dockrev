# Dockrev：自动更新回滚诊断实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖与 rollout 相关事实。

## Current Status

- Implementation: not started
- Lifecycle: active
- Catalog note: `docs/specs/README.md` records this topic in the canonical catalog.

## Coverage / Rollout Summary

- No runtime code, migration, production configuration, deployment, or update retry is part of this design handoff.
- Implementation begins only after the owner authorizes it.

## Implementation Order

1. Add the nullable jobs BLOB migration and database methods for evidence metadata, archive storage, terminal-job retention, and recovery lookup.
2. Add the private job spool abstraction and bounded, non-streamed candidate capture path. It must preserve original content in spool files without routing it through `DbLoggingRunner`.
3. Replace the fixed health wait with candidate effective-policy inspection and the specified deadline calculation.
4. Persist candidate files before each health-triggered rollback; assemble, persist, and recover the `tar.zst` archive at the job boundary.
5. Extend job summary and API contracts, then add the authorized archive download endpoint.
6. Add the Job Detail download affordance and all focused, API, migration, recovery, and shared-testbox validation described in `SPEC.md`.

## Remaining Gaps

- All acceptance criteria remain unimplemented.
- No production update has been retried, so the root cause of the historical candidate failure remains unproven.

## Related Changes

- None

## References

- `./SPEC.md`
- `./HISTORY.md`
