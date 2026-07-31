# Dockrev：服务详情操作下拉与生命周期任务 演进记录（#9cq2a）

## Decisions

- 生命周期状态由服务级 Compose 查询提供，不复用或触发全量 runtime scan。
- `partial` 和 `unknown` 都是人为介入状态，不允许自动修复或提交新的生命周期操作。
- `start` 不需要确认；`stop` 与 `restart` 需要确认。
- Dockrev 自身服务排除在普通生命周期入口之外。
- 只读的 `dry-run` 更新不占用服务运行态串行锁；只有实际 apply 更新、回滚和生命周期任务互相排队，避免预览阻塞紧急恢复操作。
- 自动更新遇到同服务操作锁时释放 pending claim 供后续调度重试；候选失效等非暂时冲突仍保留既有跳过语义。
- no-pull 约束优先于旧 Compose CLI 兼容：无法显式传递 `--pull never` 时允许任务失败，但不得以默认 pull policy 启动服务。
- 服务详情的视觉证据必须在完成 live refresh 后捕获；只读缓存快照会收束写操作，不能作为运行态操作栏的验收图。
- 服务详情操作菜单采用项目的 shadcn/Radix `ButtonGroup` 与 `DropdownMenu`，由组件库处理焦点、方向键和 Escape 关闭语义；仅保留与 Dockrev 操作栏对齐的紧凑尺寸样式。
- 顶部操作栏的旧主按钮固定宽度规则不适用于 split dropdown 的图标 trigger；该 trigger 固定为 36px，主按钮仍沿用既有宽度。
- split dropdown 的展开图标固定为 16px，并根据 Radix trigger 的 `data-state` 在菜单打开时翻转为向上，提供明确的展开反馈。
- split dropdown 的箭头按钮不继承 primary 左边框或 inset highlight；两个动作区在静态状态以居中的 16px 中性短线分隔，保持连续表面。
- split dropdown 的主动作及其菜单项都携带 Lucide 语义图标；图标辅助文字辨识而不替代文字标签，加载态沿用既有进度反馈。
- 停止操作使用实心方块，避免与缺失文本的占位符混淆；菜单图标与包含辅助说明的文字块沿 Y 轴居中。
- split dropdown 不显示额外的“默认”标记，当前可执行操作由主按钮本身表达。
- 不可执行菜单项的原因不占用菜单布局：悬浮使用浮动 Tooltip，点击使用 Toast；菜单宽度由最长操作项自然撑开，并受视口安全上限约束。
- 服务详情的 apply 动作使用简洁标签“更新”；主按钮、下拉项和确认按钮保持一致，技术执行语义不变。
- split action 的主动作不继承顶部普通 primary button 的固定宽度，而是按图标与标签自然收缩；展开 trigger 仍固定为 36px。
- 服务详情的页面状态标题改为渲染在 AppShell 顶栏；正文移除重复的服务标题。
- 服务资源摘要与服务名共享 AppShell 顶栏，置于名称与操作组之间；按 CPU/内存、磁盘读/写、下载/上传分组，容器变窄时只整体移除网络、磁盘、CPU/内存三组，避免折行或拆开成对指标。
- 移动端服务详情页头保持单行，承载全局导航、图标 Logo、当前服务名和 44px 服务操作入口；资源摘要不进入移动端页头。服务操作入口使用 shadcn/Radix DropdownMenu，将更新、生命周期与 Stack 三组动作直接平铺，并使用库内水平分隔线表达组边界。
