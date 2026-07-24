# Dockrev：Jobs 队列领取索引与慢告警 演进历史（#7xt5t）

> 这里记录会影响 Agent 理解“为什么一步步变成现在这样”的关键演进；单次任务流水账不放这里，规范正文仍以 `./SPEC.md` 为准。

## Decision Trace

- 2026-07-24: 101 诊断确认共享 SQLite 连接中的领取查询缺少 `type/status/created_at/id` 索引，导致高频空闲轮询放大为持续 CPU 消耗。
- 2026-07-24: 锁定最小修复为复合索引加按任务类型限频的慢领取 WARN；保持 worker 间隔和调度语义不变。
- 2026-07-24: 实现共用领取 SQL 的查询计划门禁、FIFO/过滤状态测试与 clone 共享限频器测试；本地全量 Rust 门禁通过。

## Key Reasons / Replacements

- 新 spec 跨越 GHCR webhook jobization 与 repo-link backfill 两个既有主题，避免把共享队列性能约束归属给其中任一已完成规格。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
