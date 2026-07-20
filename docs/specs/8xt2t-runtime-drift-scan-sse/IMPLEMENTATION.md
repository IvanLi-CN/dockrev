# Implementation

## 当前覆盖

- Backend:
  - `runtime_scan` job type and `/api/runtime-scans` trigger path.
  - Scope-aware runtime scan for all stacks, one stack, or one service.
  - Docker Compose project/service label lookup with container `.Image` and started-at collection.
  - Runtime digest reconciliation through the same check/candidate calculation path used by manual checks.
  - Moving-tag hardening: if repo-matched `RepoDigests` are unavailable for the running image, the container image ID is persisted as the runtime truth source instead of falling back to the host-local `repo:tag` pointer.
- Web/API:
  - Runtime scan job events remain surfaced through existing job/SSE paths.
  - Read-only UI surfaces no longer enqueue runtime scans on page-open; runtime reconciliation stays on scheduled/background scans plus explicit operator-triggered jobs.
  - No external response shape changes.

## 验证

- Targeted runtime scan tests cover drift update, check/runtime inference parity, no-drift registry call avoidance, and shared moving tag image-ID fallback.

## Legacy Migration

- Canonical spec created from `docs/plan/8xt2t:runtime-drift-scan-sse/PLAN.md`.
- Legacy source is retained pending delete approval.
