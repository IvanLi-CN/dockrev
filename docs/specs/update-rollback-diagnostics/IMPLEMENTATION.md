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
2. Completed the private per-job spool and candidate log capture path: Docker CLI stdout/stderr are merged and streamed byte-for-byte to disk without an application-side size cap, with a 300-second watchdog, interrupted-capture manifest checkpoint, and explicit partial-capture metadata.
3. Completed candidate effective-policy inspection and policy-derived health deadline calculation.
4. Completed pre-rollback capture, job-boundary `tar.zst` assembly, serialized startup recovery, and terminal cleanup integration. Startup runs generic incomplete-job recovery before evidence recovery; evidence may attach before the existing deferred interrupted-update-backup recovery completes, and evidence recovery does not itself change job status. Candidate logs are captured before service rollback; successful updates create no per-job spool or archive. If archive persistence fails, finalization records bounded error metadata and retains the spool for retry. Recovery preserves the spool when archive lookup fails, rather than treating a database error as an absent archive. Archive persistence and authorized download use incremental SQLite BLOB I/O and bounded chunks.
5. Completed job summary metadata and the authorized archive download endpoint.
6. Completed the Job Detail download affordance; focused and environment-dependent validation is tracked by the delivery gate.

## Remaining Gaps

- Raw-to-file runner tests cover exact binary output larger than 1 MiB, merged Docker CLI stdout/stderr, and partial-file retention on timeout and nonzero exit; evidence tests verify extracted archive bytes, interrupted-capture recovery, post-commit cleanup, existing-archive preservation, and successful-update absence through the update job path. The API download test now captures candidate bytes, archives and commits them to the job BLOB, downloads through the authorized endpoint, and extracts the downloaded archive to verify exact bytes without exposing the marker in job detail JSON. Checkpoint persistence failure skips log-command startup, and an internal update outcome signal preserves later healthcheck failure detection when the public first `failureStep` differs. Final Linux `cargo test --workspace --locked` passed (993 API tests, 1 ignored; 1 common test; 58 supervisor tests); all-target/all-feature Clippy and check, file-budget, formatting, release helper tests, and documentation build passed. Frontend lint/build and Storybook build passed. The 419-story smoke mode and focused global interaction mode passed independently; the combined Storybook test command timed out at different late global menu/version-navigation assertions after the stories passed, so its CI result remains a delivery gate.
- The broader SPEC Dockerfile-versus-Compose health-policy integration was not rerun for this log-capture change: the locked plan marks empirical acceptance unnecessary and this change does not alter health policy. That topic-level validation remains outside this change's acceptance.
- No production update has been retried, so the root cause of the historical candidate failure remains unproven.

## Related Changes

- Runtime: `crates/dockrev-api/src/rollback_evidence.rs`, updater, DB, API, and Job Detail integration.
- Data/API contracts: `./contracts/db.md`, `./contracts/http-api.md`.

## References

- `./SPEC.md`
- `./HISTORY.md`
