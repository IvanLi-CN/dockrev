# Dockrev：任务结果原因摘要与气泡详情 演进历史（#b3fhv）

> 这里记录会影响 Agent 理解“为什么一步步变成现在这样”的关键演进；单次任务流水账不放这里，规范正文仍以 `./SPEC.md` 为准。

## Decision Trace

- 2026-06-29: 新建本 spec，冻结“API 统一派生 resultReason + queue/detail/recent updates 共用摘要与气泡详情”的范围与验收口径。
- 2026-06-29: 实现 `resultReason` API 派生器、共享前端展示组件与 Storybook 视觉证据；补上 multi-stack 终态失败时优先选择失败 stack summary 的派生规则。

## Key Reasons / Replacements

- 任务状态本身不能回答“为什么成功/失败/已回滚”，需要一个跨列表/详情复用的 owner-facing 结果原因合同。
- 既有 `progress.message`、结构化 `summary`、`failureStep`/`lastError` 已能提供原始事实，本 spec 只负责把它们提升为统一展示合同。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
