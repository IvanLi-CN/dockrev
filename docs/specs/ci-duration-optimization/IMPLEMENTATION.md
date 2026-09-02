# Dockrev CI Duration Optimization and Release Assurance 实现状态

## Current Status

- Implementation: implemented; remote timing evidence remains a release acceptance step
- Lifecycle: active
- Catalog note: Fast gate, source gate, and exact-SHA release proof are implemented together.

## Implementation Coverage

- `REQ-CI-DURATION-001`: `.github/workflows/source-build-release-gate.yml`, `.github/scripts/deploy-smoke.sh`, and Buildx cache scopes.
- `REQ-CI-DURATION-002`: `.github/scripts/release_source_gate.py` and the Release prepare dependency.
- `REQ-CI-DURATION-003`: `web/scripts/storybook-sharding.mjs` and Storybook matrix jobs.
- `REQ-CI-DURATION-004`: `.github/workflows/ci-gate-verification.yml`, `.github/scripts/verify_ci_gate_metrics.py`, and attestation artifacts.
- Verification commands and final rollout receipts are recorded only after implementation and controlled GitHub Actions validation.

## Coverage / rollout summary

- The controlled validation matrix is 16 serial `workflow_dispatch` runs: six candidate runs followed by ten final warm samples. The fixed wall-clock deadline remains 204 minutes.

## Controlled Validation Contract

- Freeze one candidate SHA before the first dispatch and record it in every run artifact; do not use a moving branch or a natural `main` sample.
- Dispatch only `CI Gate Verification` with its single `target_sha` input. The workflow forces `full`, `web=true`, `docker=true`, and `publish=false` and queues runs with `cancel-in-progress: false`.
- The only supported matrix runner is `.github/scripts/run_ci_gate_validation.py`. Run it first with `--phase candidates` and the frozen two- and three-shard refs, then with `--phase final --candidate-dir <candidate-output>` and the exact target/ref selected from the candidate P90s. It refuses non-contract timeout/interval values, an existing output directory, a missing candidate matrix, or a final target that does not match the selected candidate.
- For each run, capture the run id from `gh workflow run`, then poll `gh run view <run-id> --json status,conclusion,startedAt` every 15 seconds until the workflow reaches a natural terminal state. A status read may make at most three read-only transport attempts before failing; it never dispatches a replacement workflow. A stopped final observer can resume one or more exact already-dispatched run IDs in chronological order, then reapply every metrics and exact-SHA assertion before dispatching only missing samples. GitHub's `startedAt` timestamp begins the fixed 720-second execution observation plus a fixed 180-second collection grace, so runner queue time remains separate from repository execution. The fixed 204-minute serial matrix deadline still bounds queueing and execution together. The observer rechecks its deadlines after each status query and never cancels a run at the execution boundary. Metrics still reject execution over 720 seconds (and warm execution over 600 seconds), so the grace cannot make a slow sample pass. A terminal failure, observation timeout, missing artifact, SHA mismatch, or failed child gate consumes that sample and is never retried automatically.
- Keep the sample order serial: three controlled two-shard candidate runs, three controlled three-shard candidate runs, then ten consecutive warm full-path runs. Candidate P90 is the third sorted fast duration for each matrix; a difference below 30 seconds selects three shards, otherwise the lower P90 selects the final matrix. The candidate and final phases share the exact-SHA verification cache scope, so a separate final cold run would be impossible after candidate execution. The chosen shard matrix must pass the Storybook coverage artifact check before its timing is comparable.
- The candidate phase writes the single absolute deadline (`204 minutes` from its start) into `deadline.json`; the final phase reuses it, so splitting the command cannot extend the validation budget.
- Aggregate only the final ten warm artifacts with `python3 .github/scripts/verify_ci_gate_metrics.py <metrics-dir>`. It computes P50 as `(x5+x6)/2` and P90 as `x9`, using absolute UTC `Z` timestamps; queue seconds remain separate from execution seconds.
- Acceptance is deterministic: every workflow run is bounded by the fixed 720-second timeout; the final ten warm samples must have fast P50 <= 360s, fast P90 <= 420s, source P50 <= 390s, source P90 <= 480s, eligibility P50 <= 420s, eligibility P90 <= 480s, and no warm sample over 600s. Candidate cache status is recorded for comparison but is not part of the warm-sample threshold. Any invalid sample requires fresh owner authorization before expanding the fixed validation budget.

## Remaining Gaps

- Remote controlled timing samples remain pending until the registered workflow is dispatched against frozen two-shard and three-shard refs. If dispatch cannot create a run, either phase stops without retry; a failed candidate phase cannot authorize the final phase.

## References

- `./SPEC.md`
- `./HISTORY.md`
