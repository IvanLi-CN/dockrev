# Dockrev CI Duration Optimization and Release Assurance

## Status

Superseded by [ADR 0008](../../adr/0008-pr-label-release-identity.md). The candidate/readiness and FIFO release contract described here is retained as historical context only.

## Context and Scope

- Context: Fast CI feedback currently shares its critical path with a costly Dockerfile source build and Compose deployment smoke.
- In scope: Fast `CI (main)`, the source-build release gate, Storybook coverage partitioning, exact-SHA release eligibility, and controlled timing evidence.
- Out of scope: Product behavior, release asset formats, branch protection, and the developer default Compose/Dockerfile path.

## Terms and Interfaces

- `fast main gate`: The `CI (main)` result that establishes prompt feedback for a main commit. It is not publication permission.
- `source-build release gate`: The release-blocking verification of a target SHA's Dockerfile source build and Compose deployment topology.
- `CI Gate Verification`: A manual, non-publishing workflow with one `target_sha` input and fixed full Docker/Web scope.
- `release preparation artifact`: An unpublished, exact-SHA Web and binary deliverable with a SHA-256 manifest. It is not source-build proof.
- `readiness receipt`: An immutable `refs/notes/release-readiness` proof binding one candidate run, source attestation, preparation artifact, target SHA, operation/audit metadata, and `publish=false`.
- Interface: `Release Candidate Pipeline`, its reusable child workflows and readiness receipt, plus the Release evaluator.

## Requirements

### REQ-CI-DURATION-001

- The system MUST keep source-built `runtime` and `runtime-supervisor` Docker targets and the authored Compose deployment smoke release-blocking.
- Inputs: an exact target SHA and either the production main push scope or a forced full verification scope.
- Outputs: a successful source-gate attestation containing the target SHA, scope, source result, and `publish=false`.
- covers: `G1`, `G2`

### REQ-CI-DURATION-002

- The system MUST make Release publication depend on a readiness receipt whose fast, source, and preparation evidence all bind the same target SHA.
- Inputs: an exact-SHA candidate run and the immutable receipt recorded after its child workflows succeed.
- Outputs: fail-closed eligibility or a bounded failure before any release build or publish job; Release MUST not poll Actions runs.
- covers: `G2`, `G3`

### REQ-CI-DURATION-003

- The system MUST partition generic Storybook smoke stories without duplication while running global interaction and rollback checks exactly once.
- Inputs: a stable Storybook story-id list and a one-based shard index/total.
- Outputs: disjoint shard coverage whose union equals the input list, plus one global result.
- covers: `G3`

### REQ-CI-DURATION-004

- The system MUST provide a non-publishing verification workflow that forces `full`, `web=true`, and `docker=true` for an exact target SHA.
- Inputs: only a 40-character `target_sha` on `workflow_dispatch`.
- Outputs: UTC timing, queue, cache, scope, coverage, and publish markers in a metrics artifact.
- covers: `G4`, `G5`

### REQ-CI-DURATION-005

- The system MUST prepare release-enabled main commits before publication with Web and amd64/arm64 gnu/musl binary inputs in an immutable artifact retained for one day.
- The preparation workflow MUST have no package, tag, GitHub Release, or image publication authority and MUST write `publish=false` plus a complete SHA-256 manifest.
- The complete manifest MUST include `web/dist/.dockrev-route-contract.json`; Release MUST bind the downloaded manifest to the preparation gate's SHA-256 and verify every listed file's presence, size, and SHA-256 before consuming the artifact.
- The candidate pipeline MUST write a readiness receipt only after the source attestation and preparation manifest have been validated. The receipt MUST include the target SHA, candidate run id, source attestation run/name/digest, preparation run/name/manifest digest, and `publish=false`.
- Release MUST consume only a receipt whose target and recorded artifacts match the oldest pending snapshot. Missing, expired, or mismatched evidence blocks both automatic and manual publication; Release MUST not dispatch recovery or wait for another workflow.
- Candidate MUST expose `operation=verify|recover`; `verify` MUST remain non-publishing, while `recover` MUST require an exact main SHA and non-empty reason, accept only the oldest unreleased first-parent release-enabled target, write a target-only `candidate-recovery` snapshot, and append an auditable readiness entry without publishing.
- covers: `G2`, `G3`

## Verification

### VER-CI-DURATION-001

- Method: local Python and shell contract fixtures plus workflow YAML parsing.
- covers: `REQ-CI-DURATION-001`, `REQ-CI-DURATION-002`, `REQ-CI-DURATION-004`
- Pass condition: missing, mismatched, failed, or publishing source-gate proof is rejected and ordinary local Compose mode remains source-built.

### VER-CI-DURATION-002

- Method: Storybook sharding fixture over the checked-in story-id selection function.
- covers: `REQ-CI-DURATION-003`
- Pass condition: two- and three-shard partitions have no overlap, no omissions, and exactly one global/rollback execution path.

### VER-CI-DURATION-003

- Method: six serial candidate verification dispatches, deterministic shard selection from candidate P90s, then ten serial warm final verification dispatches, followed by metrics aggregation.
- covers: `REQ-CI-DURATION-001`, `REQ-CI-DURATION-002`, `REQ-CI-DURATION-004`
- Pass condition: the final ten warm runs prove full scope and cache hits and satisfy the fixed P50/P90 seconds thresholds. Candidate cache status is recorded for diagnosis but is not a required final cold precondition: candidates and final runs share the same exact-SHA verification cache scope. Metrics record the top-level workflow queue, each reusable child gate queue, and absolute UTC start/completion timestamps. `fast_seconds`, `source_seconds`, and the 720-second `execution_seconds` bound begin when the corresponding child gate first receives a runner; `eligibility_seconds` and `wall_seconds` remain measured from the top-level `run_started_at`. A status read may make at most three read-only transport attempts and never creates a replacement workflow. One or more recorded final runs can be resumed only by their exact IDs in chronological order. The fixed 204-minute serial matrix deadline still bounds queueing and execution together.

### VER-CI-DURATION-004

- Method: Python manifest/readiness fixtures, recovery/FIFO fixtures, and a workflow YAML contract check for reusable candidate children, explicit recovery preflight, and verification-only dispatch.
- covers: `REQ-CI-DURATION-005`
- Pass condition: exact target SHA, trusted main workflow, complete file digests, one-day retention, and `publish=false` are required; a missing, mismatched, or verification-mode receipt is rejected, recovery is oldest-first and audited, and no polling/automatic-recovery path exists.

## Related ADRs

- [0005-source-build-release-gate](../../adr/0005-source-build-release-gate.md)
- [0006-early-release-preparation-artifacts](../../adr/0006-early-release-preparation-artifacts.md)
- [0007-event-driven-release-readiness](../../adr/0007-event-driven-release-readiness.md)

## References

- `./IMPLEMENTATION.md`
- `./HISTORY.md`
- `../../48mh8-release-snapshot-queue-alignment/SPEC.md`
