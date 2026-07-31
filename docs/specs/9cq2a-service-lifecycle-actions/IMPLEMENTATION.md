# Dockrev：服务详情操作下拉与生命周期任务 实现状态（#9cq2a）

## Current Status

- Implementation: 已实现（本地验证完成）
- Lifecycle: active
- Catalog note: fast-track（服务级 lifecycle queue、split dropdown、页头服务名）

## Coverage

- 服务级 lifecycle API 已提供实时 Compose 状态、`start | stop | restart` 任务提交和活动任务直达信息；Compose V2 启动使用 `up -d --pull never --no-recreate`，Compose V1 使用仅启动已有容器的 `start`，两条路径都不会拉取或替换已有容器。
- 活动 lifecycle 任务结算后，服务详情会立即刷新最近记录、版本记录与操作历史，不依赖重新加载页面。
- `service_lifecycle` 与同一服务的 apply update、rollback 采用同一冲突保护；任务占锁、调用方提供的实际服务目标和首条日志在同一 SQLite 事务提交，避免日志失败留下永久活动锁；定向 Stack/全局更新按持久化的实际服务目标占锁，仅对缺少目标记录的旧活动任务回退到 scope 判断；任务摘要、队列显示与服务操作历史均保留并展示具体动作。
- 服务详情页已使用两个可访问 split dropdown；有候选时更新动作统一使用“更新”标签，生命周期按实时运行态切换默认项。split group、菜单语义、焦点和键盘关闭均由项目的 shadcn/Radix `ButtonGroup` 与 `DropdownMenu` 负责；主动作和菜单项使用 Lucide 语义图标，其中停止为实心方块，菜单图标与文字块沿 Y 轴居中；split 主动作按图标与标签自然收缩，不继承普通顶部 primary button 的固定宽度；菜单禁用原因通过 Radix Tooltip 与 Toast 呈现，不占用菜单行高，面板由最长操作项自然撑开并受视口安全上限约束；桌面图标 trigger 固定为 36px，移动端提升为 44px 触摸目标，箭头为 16px 并随 Radix 展开状态翻转，静态表面以居中的 16px 中性短线标明动作区，Dockrev 自身服务仍只显示 Supervisor 自升级。
- 生命周期任务创建后只在活动期间轮询该服务的 lifecycle status；结算后立即以服务级 Compose 查询收敛按钮状态。
- 生命周期写入口拒绝已归档服务及其所属已归档 Stack，保留状态读取和历史查看。
- 服务详情通过页面状态把当前服务名与资源摘要渲染到 AppShell 顶栏：摘要位于名称和操作组之间，正文不再渲染重复的服务标题或资源摘要。摘要使用容器宽度控制三个不可拆分指标组的可见性，按网络、磁盘、CPU/内存顺序渐进隐藏。移动端资源摘要整体隐藏，页头保持单行，以图标 Logo、当前服务名和 44px 服务操作入口构成；入口使用 shadcn/Radix DropdownMenu，将更新、生命周期与 Stack 三组动作直接平铺并以库内分隔线区分。离线或缓存快照状态也复用同一移动入口，刷新项保持可见但禁用，避免只读分支重新挤入独立页头按钮。
- 稳定 mock-only Storybook 证据已写入 `SPEC.md`；桌面证据覆盖资源摘要与 split action，393 × 852 移动证据覆盖单行页头及三组服务操作菜单。

## References

- `./SPEC.md`
- `./HISTORY.md`
- `./contracts/http-api.md`
