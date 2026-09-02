# Dockrev CI Duration Optimization and Release Assurance

## Context and Scope

- Context: Fast CI feedback currently shares its critical path with a costly Dockerfile source build and Compose deployment smoke.
- In scope: Fast `CI (main)`, the source-build release gate, Storybook coverage partitioning, exact-SHA release eligibility, and controlled timing evidence.
- Out of scope: Product behavior, release asset formats, branch protection, and the developer default Compose/Dockerfile path.

## Terms and Interfaces

- `fast main gate`: The `CI (main)` result that establishes prompt feedback for a main commit. It is not publication permission.
- `source-build release gate`: The release-blocking verification of a target SHA's Dockerfile source build and Compose deployment topology.
- `CI Gate Verification`: A manual, non-publishing workflow with one `target_sha` input and fixed full Docker/Web scope.
- Interface: `Source Build Release Gate` and `CI Gate Verification` workflow runs, their exact-SHA attestations, and the Release evaluator.

## Requirements

### REQ-CI-DURATION-001

- The system MUST keep source-built `runtime` and `runtime-supervisor` Docker targets and the authored Compose deployment smoke release-blocking.
- Inputs: an exact target SHA and either the production main push scope or a forced full verification scope.
- Outputs: a successful source-gate attestation containing the target SHA, scope, source result, and `publish=false`.
- covers: `G1`, `G2`

### REQ-CI-DURATION-002

- The system MUST make Release publication depend on successful fast and source gates for the same target SHA.
- Inputs: the pending release snapshot target and GitHub Actions run metadata.
- Outputs: fail-closed eligibility or a bounded failure before any release build or publish job.
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
- Pass condition: the final ten warm runs prove full scope and cache hits and satisfy the fixed P50/P90 seconds thresholds. Candidate cache status is recorded for diagnosis but is not a required final cold precondition: candidates and final runs share the same exact-SHA verification cache scope. The measurement runner observes each workflow to a natural terminal state with a fixed 180-second collection grace after GitHub's `startedAt` timestamp, so runner queue time remains separate from repository execution; a status read may make at most three read-only transport attempts and never creates a replacement workflow. One or more recorded final runs can be resumed only by their exact IDs in chronological order. The fixed 204-minute serial matrix deadline still bounds queueing and execution together, and metrics fail closed when measured execution exceeds 720 seconds.

## Related ADRs

- [0005-source-build-release-gate](../../adr/0005-source-build-release-gate.md)

## References

- `./IMPLEMENTATION.md`
- `./HISTORY.md`
- `../../48mh8-release-snapshot-queue-alignment/SPEC.md`
