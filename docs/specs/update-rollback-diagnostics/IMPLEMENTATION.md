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
4. Completed pre-rollback capture, job-boundary `tar.zst` assembly, serialized startup recovery, and terminal cleanup integration. Startup resets stale update-stop recovery claims and keeps every job with a recovery snapshot out of generic terminalization before the deferred restore worker reclaims it. The API listener binds before evidence archive reconstruction starts in the background, and the update-stop restore worker is spawned first; an old job may be visible before its download archive is attached. Successful deferred recovery commits job terminalization and snapshot clearing atomically. Generic job recovery runs before evidence recovery; evidence may attach before the existing deferred interrupted-update-backup recovery completes, and evidence recovery does not itself change job status. Final-manifest retry markers survive bounded error merges, retries require a valid checkpoint with a candidate count matching the terminal summary, and recovery conservatively sets `logsTruncated=true`; a failed retry preserves the spool and prior candidate total without attaching an archive. Other terminal recovery paths also reject a parsed checkpoint whose record count differs from an existing incomplete-summary candidate total. Recovery retains previous bounded service/error metadata on failure and prioritizes capture errors from candidates omitted by the service summary limit. Candidate logs are captured before service rollback; successful updates create no per-job spool or archive. If archive persistence fails, finalization records bounded error metadata and retains the spool for retry. Recovery preserves the spool when archive lookup fails, rather than treating a database error as an absent archive. Terminal recovery checks committed BLOBs before manifests, cleans spool/archive/part-only residue, reports cleanup failures without overwriting committed metadata, and uses the same bounded summary projection as normal finalization. Recovery attaches only when the BLOB is still empty. Archive manifest and raw log bytes are not truncated by summary bounds. Archive persistence and authorized download use incremental SQLite BLOB I/O and bounded chunks.
5. Completed job summary metadata and the authorized archive download endpoint.
6. Completed the Job Detail download affordance; focused and environment-dependent validation is tracked by the delivery gate.

- Spool initialization failures now use the same bounded top-level error projection as archive persistence failures, including the truncation note for overlong messages.
- Evidence summaries count only candidates whose evidence capture was actually established; recovery preserves a previously recorded candidate total when a manifest cannot be read, and omits the field when no count is known rather than inventing zero.
- Docker `State.Error` is retained in the private archive manifest but excluded from ordinary job-summary metadata because the runtime value may contain sensitive details.
- Interrupted update-backup recovery claims snapshots atomically; successful recovery commits job terminalization with snapshot clearing, terminal jobs are not rearmed within the same process, while snapshots left by an interrupted restore are reclaimed on the next startup.
- Successful deferred update-stop recovery removes a stale `recoveryError` from the new terminal summary while retaining unrelated prior summary fields.
- Evidence files and archive parts are created owner-only before candidate bytes are written, including atomic promotion and recovery-created files.
- Startup recovery clears claims persisted by a previous process before generic incomplete-job finalization; generic recovery leaves jobs with update-stop snapshots for the dedicated restore worker.

## Validation Evidence

- The current repair candidate passed `cargo test --workspace --locked --all-features` on the shared Linux testbox: `dockrev-api` 1047 passed and 1 ignored, `dockrev-common` 1 passed, and `dockrev-supervisor` 58 passed; doctests completed with no tests. The focused rollback-evidence group passed (43 tests), as did the unknown-count recovery, private `State.Error` summary, and post-BLOB summary-transaction failure regressions.
- The same candidate passed `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo check --workspace --locked --all-targets --all-features`. File-budget, Spec contract, and Spec drift checks pass for this working tree.
- `bun run lint` passed locally with 0 errors and 3 existing warnings in unrelated frontend files. No frontend files changed; frontend build and Storybook validation are not claimed for this repair batch.
- Existing regressions also cover retrying final-manifest failure without losing the spool, preserving prior summary metadata, capture errors beyond the service-summary limit, two-start claim retry, atomic terminalization, watchdog partial-output retention, owner-only archive/log files, spool-initialization failure metadata, and summary bounds.

## Remaining Gaps

- The broader SPEC Dockerfile-versus-Compose health-policy integration was not rerun for this log-capture change: the locked plan marks empirical acceptance unnecessary and this change does not alter health policy. That topic-level validation remains outside this change's acceptance.
- No production update has been retried, so the root cause of the historical candidate failure remains unproven.

## Related Changes

- Runtime: `crates/dockrev-api/src/rollback_evidence.rs`, `crates/dockrev-api/src/rollback_evidence_archive.rs`, `crates/dockrev-api/src/db/jobs_rollback_evidence.rs`, updater, DB, API, and Job Detail integration.
- Data/API contracts: `./contracts/db.md`, `./contracts/http-api.md`.

## References

- `./SPEC.md`
- `./HISTORY.md`
