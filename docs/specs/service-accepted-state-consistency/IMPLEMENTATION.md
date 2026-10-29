# Dockrev Service Accepted State 一致性实现状态

> 当前有效规范以 `./SPEC.md` 为准；这里仅记录实现覆盖、执行顺序和验证事实。

## Current Status

- Implementation: active
- Lifecycle: implementation candidate
- Catalog note: `docs/specs/README.md` records this canonical topic.
- Diagnosis: production race confirmed from Job logs, source write paths, and persisted state history.
- Existing focused baseline: `cargo test -p dockrev-api update_ --no-fail-fast` passes 68 tests with the accepted-state guard enabled.
- Update stop requests now interrupt operation-lock waits and settle the immutable baseline before terminal cancellation.
- A stop observed while an updater is returning a failed step result is still converted to `cancelled` when the apply gate was not committed; the baseline settlement remains atomic and the Compose service is left unchanged.
- Startup recovery now handles pre-apply stop requests, restores backup snapshots, verifies observed runtime digests, and settles fenced interrupted updates atomically; unavailable runtime facts remain retryable and fenced.
- A single update may persist an ordered snapshot list covering every affected stack; startup restores and verifies the complete list before clearing recovery ownership.
- Recovery snapshots are cleared per stack only for pre-apply backup cleanup; non-empty snapshots remain through apply and are cleared only by terminal job cleanup, preserving the full multi-stack crash window.
- Startup update recovery completes before the listener and job-producing background workers start, so a live worker cannot be mistaken for interrupted work by the recovery scan.
- After the apply gate commits, a terminal update must provide an authoritative runtime settlement for every baseline target; incomplete observations preserve the odd generation and retry diagnostic. Failed rollbacks retain the durable candidate while settling the observed baseline.
- Managed-override restore failures are treated as recovery fences alongside ordinary runtime restore errors.
- Red regression: `api::tests::update_apply_healthcheck_rollback_preserves_candidate_across_concurrent_observers` first reproduced declaration overwrite and candidate loss, then passed after CAS/ownership integration.

## Implementation Order

### 1. Establish the red regression (covered)

- Extend the integration seam used by `api::tests::update_apply_healthcheck_rollback_exposes_attempted_and_final_digests_via_api`; the new regression lives in `api::tests::suite_27.rs` so the existing suite remains within the repository file budget.
- Add `health_waiting` and `release_health` notifications to `HealthRollbackUpdateRunner`; signal after the candidate is running and before returning its unhealthy status. Do not retain the runner step mutex across an await.
- While update waits at that gate, invoke the real `POST /api/discovery/scan` and `POST /api/runtime-scans` routes and wait for both Jobs to finish.
- Release the unhealthy result, wait for `rolled_back`, and assert declaration unchanged, `current=old`, and `candidate=new`.
- Run the exact focused test and retain its pre-fix failure output as implementation evidence.

### 2. Add persistence primitives (covered)

- Add `services.accepted_state_generation INTEGER NOT NULL DEFAULT 0`.
- Add nullable historical-compatible `job_service_targets.opened_generation` and `baseline_snapshot_json`; require both for newly accepted mutating Jobs.
- Introduce typed, versioned baseline serialization covering every accepted-state column.
- Implement one `IMMEDIATE` acquisition transaction that performs conflict detection, complete target resolution, baseline capture, generation transition, Job insertion, target insertion, and initial Job log.
- Implement observation CAS results and make successful observer persistence advance the even generation by `2`.
- Implement an owner-checked multi-target settlement transaction that writes all target snapshots and closes generations before the terminal Job transaction; terminal event publication remains after both commits.

### 3. Fence observation writers (covered for check/runtime/discovery)

- Carry generation through `ServiceForCheck` and `ServiceForRuntimeScan`.
- Move Service snapshot and candidate-dependent projection writes behind one CAS boundary; remove unconditional candidate clearing from runtime fallback.
- Add a Stack observation token and change `sync_stack_from_compose` to all-or-nothing membership/generation validation.
- Treat CAS rejection as deferred/stale observation with structured logs, not a failed Docker or registry check.

### 4. Unify terminal settlement (covered for update transition and recovery)

- Success updates collect every acquired target and settle them together; apply-before-failure/cancellation settles from the durable baseline through the same terminal Job transaction.
- Recovery builds settlements from durable baselines plus verified runtime inspection. Registry lookup enriches candidate knowledge but cannot erase it on failure.
- Publish terminal management events only after the settlement/Job transaction commits.
- Preserve the accepted declaration on rollback and accept the committed managed override declaration on success.

### 5. Enroll every runtime mutation (covered for update/rollback/lifecycle/reconcile acquisition)

- Route manual rollback, service/stack lifecycle and managed-override reconcile through the atomic acquisition interface.
- Route backup modes that stop or restart Services through the same ownership protocol; live backup without runtime mutation remains outside it.
- Remove check-then-insert TOCTOU behavior from managed-override reconcile.
- Keep the existing managed-override process lock limited to Compose/override filesystem effects.

### 6. Recover durable ownership (covered for update jobs)

- Reorder startup so odd-generation ownership is recovered before incomplete Jobs become terminal.
- Restore or clean managed overrides before generic recovery, then inspect runtime and settle the owning Job atomically.
- Keep unresolved Services fenced when Docker facts are unavailable or runtime verification mismatches, and expose retryable diagnostics.
- Detect odd generations without a recoverable owning target as invariant violations; never reset them silently.

## Expected Code Surfaces

- `crates/dockrev-api/src/db/schema.rs`
- `crates/dockrev-api/src/db/schema_job_history_retention.rs` or a sibling migration module
- `crates/dockrev-api/src/db/service_operations.rs`
- `crates/dockrev-api/src/db/snapshots.rs`
- `crates/dockrev-api/src/db/stacks.rs`
- `crates/dockrev-api/src/service_check.rs`
- `crates/dockrev-api/src/runtime_scan.rs`
- `crates/dockrev-api/src/discovery.rs`
- `crates/dockrev-api/src/api/operations/transitions/execution.rs`
- lifecycle, rollback, backup and managed-override operation adapters
- `crates/dockrev-api/src/main.rs`
- `crates/dockrev-api/src/api/tests/suite_27.rs` and test support runners

## Verification Matrix

| Layer | Required proof |
| --- | --- |
| API regression | Paused candidate plus real discovery/runtime scan ends rolled back with candidate preserved |
| DB acquisition | Multi-target ownership is atomic; unrelated Services remain independent |
| Observer CAS | Before/during/after mutation stale writes fail; successful observers advance generation |
| Discovery | Stack membership and generations validate all-or-nothing |
| Settlement | Every terminal outcome closes ownership before terminal event |
| Registry degradation | Last credible candidate survives registry failure |
| Recovery | Odd generation and override recover before incomplete Job terminalization |
| Adapters | Every managed runtime mutation uses the shared protocol |

## Remaining Gaps

- Startup recovery now restores managed overrides before generic incomplete-job recovery and skips jobs whose odd ownership is still active. Legacy targets with empty historical baselines remain outside the new settlement path and are left for the existing invariant diagnostics.
- Discovery uses a pre-I/O generation token and an all-or-nothing guarded compose sync; a structured deferred-result field is not exposed in the HTTP response.
- Runtime-scan CAS rejection is now treated as deferred and does not increment the update count; notification reconciliation is transactional with authoritative observer CAS.
- Full workspace/all-features validation is blocked in the shared testbox before Cargo starts: two isolated runs fail with `Permission denied (os error 13) at path "/workspace/target..."`; focused/local validation is green and remote required CI remains the independent full-suite gate.

## Related Changes

- `crates/dockrev-api/src/db/schema_accepted_state_generation.rs`
- `crates/dockrev-api/src/db/service_operations.rs`
- `crates/dockrev-api/src/db/snapshots.rs`
- `crates/dockrev-api/src/db/jobs.rs`
- `crates/dockrev-api/src/db/stacks.rs`
- `crates/dockrev-api/src/discovery.rs`
- `crates/dockrev-api/src/api/discovery_routes.rs`
- `crates/dockrev-api/src/api/operations/transitions/execution.rs`
- `crates/dockrev-api/src/runtime_scan.rs`
- `crates/dockrev-api/src/service_check.rs`
- `crates/dockrev-api/src/main.rs`
- `crates/dockrev-api/src/api/tests/suite_09.rs`
- `crates/dockrev-api/src/api/tests/support_02.rs`

## References

- `./SPEC.md`
- `./HISTORY.md`
- `../../adr/0003-service-accepted-state-generation.md`
