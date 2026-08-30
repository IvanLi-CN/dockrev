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

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
- `../../adr/0003-service-accepted-state-generation.md`
- `../uupfm-update-status-settle-after-finish/SPEC.md`
