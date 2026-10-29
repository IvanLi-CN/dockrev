# Dockrev Service Accepted State 一致性演进历史

> 记录影响长期行为的决策原因；规范正文仍以 `./SPEC.md` 为准。

## Lifecycle / Compatibility

- Lifecycle is active; the additive schema, observer CAS, mutation ownership and terminal fencing are implemented in the current topic branch.
- The contract is additive to existing HTTP requests and retains historical `job_service_targets` rows with nullable ownership fields.

## Replacements / Background

- This topic extends the success-only settlement boundary in `uupfm-update-status-settle-after-finish` to all terminal outcomes, concurrent observers and every managed runtime mutation.
- The established update and rollback behavior remains intact; the new boundary defines when its result becomes accepted Service state.

## Decision Trace

- 生产诊断确认 update、discovery 与 runtime scan 共享无条件 Service 写入口，临时 override 和 candidate runtime 因此可在回滚期间成为持久快照。
- `CONTEXT.md` 将 Service snapshot 明确为 accepted deployment state，并区分 transient operation observation。
- generation CAS、ownership carrier 和替代方案的完整取舍保存在关联 ADR；本文只保留主题边界和兼容背景。
- 停止请求与操作锁等待改为同一 watch 信号驱动；apply gate 尚未提交时只允许用持久 baseline 结算并取消，避免把未知 runtime 当作终态。
- 即使 Compose pull 在停止信号到达后返回的是失败步骤结果，只要 apply gate 尚未提交仍统一发布 `cancelled`，并以 baseline 完成同一事务的 generation 结算。
- 启动恢复先恢复 backup/override，再检查实际 compose runtime；无法确认时保留奇数 generation、记录 recovery error，并保留下一次可重试的恢复占用。
- 多栈 update 的 backup recovery snapshot 改为按 stack 累积保存；恢复必须逐栈 restore 并完成全部服务 runtime verification 后才能清理占用。
- apply 后不再按 stack 提前清除非空 snapshot；只有整单终态清理才能释放完整多栈恢复现场，备份前置失败则使用 stack-scoped cleanup。
- update recovery 在 listener 和会创建任务的后台 worker 启动前完成，避免启动后的扫描把仍在运行的新任务误判为中断任务。
- apply gate 已提交但缺少任一服务的权威 runtime settlement 时保持奇数 generation；回滚失败路径保留 baseline 中的 candidate，`managedOverrideRecoveryError` 也会触发 recovery fence。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
- `../../adr/0003-service-accepted-state-generation.md`
- `../uupfm-update-status-settle-after-finish/SPEC.md`
