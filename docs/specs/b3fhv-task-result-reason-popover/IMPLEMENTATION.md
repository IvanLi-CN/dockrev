# Dockrev：任务结果原因摘要与气泡详情 实现状态（#b3fhv）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实，避免这些细节散落到 PR / Git 历史里。

## Current Status

- Implementation: 已实现
- Lifecycle: merge-ready
- Catalog note: fast-track（resultReason API contract + queue/detail/recent updates reason popover）

## Coverage / rollout summary

- 后端在 `GET /api/jobs` / `GET /api/jobs/:id` 统一派生 `resultReason`，不改持久化 schema；覆盖 `update` / `rollback` 终态结构化原因、generic terminal fallback，以及 multi-stack 失败 summary 选择。
- 前端新增共享 `TaskResultReason` 组件，复用 `useHoverPinnedPopover`，统一摘要截断、hover/focus/click 打开、点击 pin 与 `detail + raw` 气泡层级。
- `QueuePage`、`JobDetailPage`、`RecentUpdateRecords` 已接入一致的结果原因摘要展示；Queue 行从嵌套 button 改为 `role=button` 容器，避免嵌套交互元素与键盘事件串扰。
- Storybook 已补齐 `QueuePage/ResultReasonRollback`、`JobDetailPage/HealthRollback`、`TaskResultReason` 组件入口与 `play` 断言；视觉证据已落盘到 `./assets/`。

## Remaining Gaps

- 无阻断实现的剩余缺口；后续若要扩展更多 job type 的友好文案，可继续在后端派生器追加结构化分支与回归。

## Related Changes

- `crates/dockrev-api/src/api/jobs.rs`
- `crates/dockrev-api/src/api/tests/suite_09.rs`
- `crates/dockrev-api/src/api/types/jobs.rs`
- `web/src/api/types.ts`
- `web/src/components/TaskResultReason.tsx`
- `web/src/components/RecentUpdateRecords.tsx`
- `web/src/pages/QueuePage.tsx`
- `web/src/pages/JobDetailPage.tsx`
- `web/src/App.css`
- `web/src/stories/components/TaskResultReason.stories.tsx`
- `web/src/stories/pages/QueuePage.stories.tsx`
- `web/src/stories/pages/JobDetailPage.stories.tsx`
- `web/src/stories/mocks/dockrevMockApi/fixturesBase.ts`
- `web/src/stories/mocks/dockrevMockApi/fixturesQueues.ts`

## Validation

- `cargo test -p dockrev-api update_apply_healthcheck_rollback_exposes_attempted_and_final_digests_via_api -- --nocapture`
- `cargo test -p dockrev-api trigger_service_rollback_creates_rolled_back_job -- --nocapture`
- `cargo test -p dockrev-api result_reason_prefers_failed_stack_summary_over_first_stack -- --nocapture`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook`
- `codex review --base origin/main`（最终一轮 clean）

## References

- `./SPEC.md`
- `./HISTORY.md`
