# Dockrev 异步数据连续性与加载反馈 实现状态

> 当前有效规范以 `./SPEC.md` 为准；这里仅记录实现覆盖和验证事实。

## Current Status

- Implementation: automatic refresh contract, candidate read model, and route migration in progress
- Lifecycle: active
- Delivery flow: fast-track；目标为一个直接 PR 的 `Step 5C Ready`。

## Coverage / rollout summary

- 基线实现已提供区域级骨架、用户读取遮罩、错误恢复、v2 snapshot readiness 与资源历史按当前时间窗裁剪。
- 本轮将共享原语改为区分用户读取与后台同步，新增三档新鲜度、15 秒 GET deadline、批量候选摘要和候选表虚拟化。
- 本轮覆盖首页、服务大盘、Queue、版本推测、GHCR、设置、详情页、服务树、监控、归档与部署检查的自动触发和写后读取意图。
- 视觉证据将在 mock-only `ui_demo` 与 Storybook 的新后台同步状态完成后重新采集；此前的遮罩证据不适用于本合同。

## References

- `./SPEC.md`
- `./HISTORY.md`
