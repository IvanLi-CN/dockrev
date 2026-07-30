# Dockrev：服务详情操作下拉与生命周期任务 实现状态（#9cq2a）

## Current Status

- Implementation: 已实现（本地验证完成）
- Lifecycle: active
- Catalog note: fast-track（服务级 lifecycle queue、split dropdown、页头服务名）

## Coverage

- 服务级 lifecycle API 已提供实时 Compose 状态、`start | stop | restart` 任务提交和活动任务直达信息；启动始终使用 `up -d --pull never`，不支持该选项的 Compose CLI 会失败而不会回退到可能拉取镜像的启动。
- `service_lifecycle` 与同一服务的 apply update、rollback 采用同一冲突保护；任务摘要、队列显示与服务操作历史均保留具体动作。
- 服务详情页已使用两个可访问 split dropdown；有候选时更新动作统一使用“更新”标签，生命周期按实时运行态切换默认项。split group、菜单语义、焦点和键盘关闭均由项目的 shadcn/Radix `ButtonGroup` 与 `DropdownMenu` 负责；主动作和菜单项使用 Lucide 语义图标，其中停止为实心方块，菜单图标与文字块沿 Y 轴居中；split 主动作按图标与标签自然收缩，不继承普通顶部 primary button 的固定宽度；菜单禁用原因通过 Radix Tooltip 与 Toast 呈现，不占用菜单行高，面板由最长操作项自然撑开并受视口安全上限约束；桌面图标 trigger 固定为 36px，移动端提升为 44px 触摸目标，箭头为 16px 并随 Radix 展开状态翻转，静态表面以居中的 16px 中性短线标明动作区，Dockrev 自身服务仍只显示 Supervisor 自升级。
- 生命周期任务创建后只在活动期间轮询该服务的 lifecycle status；结算后立即以服务级 Compose 查询收敛按钮状态。
- 服务详情通过页面状态把当前服务名与资源摘要渲染到 AppShell 顶栏：摘要位于名称和操作组之间，正文不再渲染重复的服务标题或资源摘要。摘要使用容器宽度控制三个不可拆分指标组的可见性，按网络、磁盘、CPU/内存顺序渐进隐藏。移动端使用双层页头，资源摘要整体隐藏；第一行以图标 Logo 紧邻当前服务名并沿 Y 轴居中，第二行承载服务操作，Stack 详情使用带 tooltip 与可访问名称的 Lucide 图标按钮收束宽度。
- 稳定 mock-only Storybook 证据已写入 `SPEC.md`；证据在已完成 live refresh 的 `LifecycleRunning` 状态捕获，确认资源摘要显示时仍保留“更新 / 停止”两个 split action，并覆盖更新与生命周期菜单中的图标状态。

## References

- `./SPEC.md`
- `./HISTORY.md`
- `./contracts/http-api.md`
