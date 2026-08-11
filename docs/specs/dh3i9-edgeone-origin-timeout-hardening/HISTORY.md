# History

- 2026-06-26: Created the follow-up spec to freeze the EdgeOne `15s` origin timeout constraint, snapshot-backed cleanup/deploy-check reads, release drawer fallback, and `5s` SSE heartbeat policy.
- 2026-06-26: Implemented cleanup snapshot workers, deploy-check refresh workers, owner-facing API contract changes, Web polling flows, Storybook coverage, and visual evidence.
- Deploy-check now acts as a hard capability gate on startup and foreground resume; cached preference settings cannot bypass a required-core failure.
- Deploy-check pass/fail desktop and `393x852` mobile mock evidence now covers the hard gate and Dashboard lock; the full Storybook smoke suite is green.
- Required core checks now require explicit `pass`, and App-level startup failure stories verify that `neverAutoOpen` cannot bypass the deploy-check gate.
- 2026-08-11: Fixed startup reconciliation so `missing` discovery projects also archive their linked active Stack records. This repairs legacy state where a deleted Compose path could remain in deploy-check after the discovery row had already been archived.
- The startup reconciliation regression covers both archived and unarchived missing discovery rows, preserves valid Stack and Service metadata, and verifies that existing fixture containers retain both identity and running state.
- Shared-testbox cleanup remains relative to the verified run directory after containment checks, so a later parent-path replacement cannot redirect deletion outside that run scope.
