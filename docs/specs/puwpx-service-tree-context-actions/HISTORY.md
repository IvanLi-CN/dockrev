# Dockrev：服务树上下文快捷操作 演进历史（#puwpx）

## Decision Trace

- Stack 生命周期采用独立 `stack_lifecycle` 队列任务，不通过多个服务任务拼接。
- “启动/重启”按运行态二选一；停止仅在运行态显示，更新与生命周期组之间使用 Separator。
- 上下文菜单四类动作直接执行，不复用详情页确认框；成功后留在原页面并提供任务入口。
- Stack 名称状态区是触发范围，展开箭头只承担展开/收起。
- Dockrev 自身 Service 保留 Supervisor 例外；包含 Dockrev 的 Stack 禁用生命周期操作。
- Stack 生命周期使用持久化的 service target 集合参与原子入队锁；旧服务级入口也会反向查询覆盖目标服务的 Stack 锁。
- 最终实现统一使用 Compose V2+ capability gate；合法配置但无容器时服务树显示 `stopped`，V1/探测失败写操作返回 `compose_v2_required`。
- 菜单状态在打开时懒加载；加载中保持动作可见但禁用，禁用原因直接展示在菜单条目内。
- Stack 详情顶部复用服务详情的生命周期 split action 语义；Stack 启动直接提交，停止和重启使用带 Stack 名称与服务数的确认框。
- Stack 右键停止/重启与 Stack 详情统一确认，Service 右键继续保留直接执行，以保持既有 Service 快捷操作契约不变。
- Compose 生命周期统一收敛到 V2+：plugin 与 standalone 共用 no-pull/no-recreate 启动命令，V1 通过 `compose_v2_required` 门禁拒绝；空容器但配置有效的 Stack 显示 `stopped`。
- Apply 更新门禁覆盖 UI、Webhook 与自动策略 enqueue；DryRun 保持可用，生命周期写操作在查询状态前确认 Compose V2 能力。

## Key Reasons / Replacements

- 本主题扩展 `c2r2u` 的只读服务树和 `9cq2a` 的服务级生命周期边界，但不取代两份既有规范。

## References

- `./SPEC.md`
- `./IMPLEMENTATION.md`
