# Dockrev：服务生命周期可观测性 实现状态

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实。

## Current Status

- Implementation: active
- Lifecycle: active
- Catalog note: 账本、Engine observer、资源图表和服务日志投影按同一事件合同实施。

## Coverage / rollout summary

- 尚未有运行时 rollout；事件只从功能上线后开始记录，不回填上线前历史。操作观测在 Docker Engine events 不可用时继续执行并写入不完整边界。

## Remaining Gaps

- 已实现：主库 30 天账本迁移、幂等写入与清理；手动服务/Stack 及更新执行路径的 OperationScopedLifecycleObserver；生命周期 REST/SSE 与资源 history 投影；图表线/区段标记和生命周期分隔事件/筛选。
- 待补强：真实 Compose/shared-testbox rollout；本机 Docker 按仓库约束未启用，运行时只从功能上线后开始记录事件。

## Related Changes

- `docs/adr/0001-service-lifecycle-event-ledger.md`

## References

- `./SPEC.md`
- `./HISTORY.md`
