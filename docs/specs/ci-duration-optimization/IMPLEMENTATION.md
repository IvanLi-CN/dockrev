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

- The controlled validation budget is 17 serial `workflow_dispatch` runs: six candidate runs followed by one cold cache warm-up and ten final warm samples.

## Controlled Validation Contract

- Freeze one candidate SHA before the first dispatch and record it in every run artifact; do not use a moving branch or a natural `main` sample.
- Dispatch only `CI Gate Verification` with its single `target_sha` input. The workflow forces `full`, `web=true`, `docker=true`, and `publish=false` and queues runs with `cancel-in-progress: false`.
- The only supported matrix runner is `.github/scripts/run_ci_gate_validation.py`. Run it first with `--phase candidates` and the frozen two- and three-shard refs, then with `--phase final --candidate-dir <candidate-output>` and the exact target/ref selected from the candidate P90s. It refuses non-contract timeout/interval values, an existing output directory, a missing candidate matrix, or a final target that does not match the selected candidate.
- For each run, capture the run id from `gh workflow run`, then use `timeout --signal=TERM 720s gh run watch <run-id> --interval 15 --exit-status` when GNU `timeout` is available; the runner uses an equivalent Python subprocess timeout on agents without GNU `timeout`. A timeout, cancellation, missing artifact, SHA mismatch, or failed child gate consumes that sample and is never retried automatically.
- Keep the sample order serial: three controlled two-shard candidate runs, three controlled three-shard candidate runs, then one cold cache warm-up and ten consecutive warm full-path runs. Candidate P90 is the third sorted fast duration for each matrix; a difference below 30 seconds selects three shards, otherwise the lower P90 selects the final matrix. The chosen shard matrix must pass the Storybook coverage artifact check before its timing is comparable.
- The candidate phase writes the single absolute deadline (`204 minutes` from its start) into `deadline.json`; the final phase reuses it, so splitting the command cannot extend the validation budget.
- Aggregate only the final ten warm artifacts with `python3 .github/scripts/verify_ci_gate_metrics.py <metrics-dir>`. It computes P50 as `(x5+x6)/2` and P90 as `x9`, using absolute UTC `Z` timestamps; queue seconds remain separate from execution seconds.
- Acceptance is deterministic: fast P50 <= 360s, fast P90 <= 420s, source P50 <= 390s, source P90 <= 480s, eligibility P50 <= 420s, eligibility P90 <= 480s, and no sample over 600s. Any invalid sample requires fresh owner authorization before expanding the fixed 17-run budget.

## Remaining Gaps

- Remote controlled timing samples remain pending until the registered workflow is dispatched against frozen two-shard and three-shard refs. If dispatch cannot create a run, either phase stops without retry; a failed candidate phase cannot authorize the final phase.

## References

- `./SPEC.md`
- `./HISTORY.md`
