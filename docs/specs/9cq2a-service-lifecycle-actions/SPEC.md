# Dockrev：服务详情操作下拉与生命周期任务（#9cq2a）

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，关键演进原因见 `./HISTORY.md`。

## 目标 / 非目标

### Goals

- 将服务详情顶部的更新动作收束为一个 split dropdown：有候选时默认“更新”，无候选时默认“回滚”，菜单始终可发现“预览更新 / 更新 / 回滚”。
- 提供服务级 `启动 / 停止 / 重启` split dropdown，并把真实执行过程记录为可审计的队列任务。
- 在 AppShell 顶栏显示当前服务名，并在名称与操作组之间显示服务资源摘要；正文不再重复服务标题或资源摘要。资源摘要按 `CPU + 内存`、`磁盘读 + 写`、`下载 + 上传` 三个不可拆分组随可用宽度逐级隐藏。
- 生命周期任务与同一服务的 update、rollback 完全串行，并显示在服务操作历史中。

### Non-goals

- 不新增 stack 级或全局生命周期操作。
- 不改变既有 update、rollback 或 Dockrev Supervisor 自升级的执行协议。
- 不自动修复部分副本运行或未知运行态。

## 接口契约

完整 HTTP 定义见 [contracts/http-api.md](./contracts/http-api.md)。

- `GET /api/services/{serviceId}/lifecycle-status` 读取该服务的实时 Compose 生命周期状态和会阻塞操作的活跃任务。
- `POST /api/services/{serviceId}/lifecycle` 接收 `{ "action": "start" | "stop" | "restart" }`，成功返回 `{ "jobId": "..." }`。
- 任务类型为 `service_lifecycle`；摘要必须携带动作，供队列、详情和服务历史展示。

## 行为规格

- `running` 默认主动作是“停止”；`stopped` 默认主动作是“启动”。
- `partial` 表示多副本服务仅部分运行；`unknown` 表示 Compose 查询失败或结果无法判定。两种状态都保持菜单可见但不可执行；原因只在悬浮时显示为浮动提示，点击不可执行项时以 toast 提示。
- 启动直接提交；停止和重启必须经现有确认交互确认后才创建任务。
- 启动在所有受支持的 Compose CLI 执行 `up -d --pull never <service>`，不得拉取或替换镜像；不支持该选项的 CLI 必须失败而不能降级为可能拉取的启动。停止执行对应 `stop <service>`；重启执行对应 `restart <service>`。
- 同服务的 update、rollback、service_lifecycle 若有 queued 或 running 任务，新的同服务操作必须以 `409` 返回既有任务 ID。服务 status 响应也必须暴露该活动任务，前端可直接跳转详情。
- Dockrev 自身服务不显示 lifecycle 菜单，继续只使用既有 Supervisor 自升级入口。
- 当前默认主动作仅由 split button 的主按钮表达；不可用动作仍可发现，但不在菜单内展示原因。
- 操作历史应包含 update、rollback 和 service_lifecycle；生命周期项显示“启动 / 停止 / 重启”而不是泛化类型名。

## 验收标准

- Given 服务有候选版本，When 打开详情页，Then 更新 split button 的主动作是“更新”；没有候选时主动作是“回滚”，且菜单始终含三项更新动作。
- Given 服务详情已加载，When 查看页面，Then 当前服务名显示在 AppShell 顶栏，正文不再重复渲染服务标题。
- Given 服务详情顶栏空间缩小时，When 资源摘要无法完整容纳，Then 先整体隐藏网络组，再隐藏磁盘组，最后才隐藏 CPU 与内存组；任一组内的两个指标不得拆开、折行或造成横向溢出。
- Given 服务正在运行，When 打开详情页，Then 生命周期 split button 主动作为“停止”；服务停止时为“启动”。
- Given 状态为 partial 或 unknown，When 打开详情页，Then 生命周期菜单保留可见但不可执行，且原因可读取。
- Given 用户点击停止或重启，When 未确认或关闭确认对话框，Then 不创建生命周期任务。
- Given 某服务已有 queued/running update、rollback 或 lifecycle 任务，When 再提交该服务的任一此类操作，Then 后端返回冲突和既有任务 ID。
- Given Dockrev 自身服务，When 打开详情页，Then 不显示 lifecycle menu。
- Given 任务完成，When 服务详情刷新，Then 生命周期状态用一次服务级 Compose 查询收敛，不触发全量 runtime scan。

## 验证

- `cargo fmt --all --check`
- `cargo test -p dockrev-api`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`
- `bun run --cwd web test-storybook`

## Visual Evidence

PR: none

- source_type: `storybook_canvas`
- target_program: `mock-only`
- capture_scope: `browser-viewport`
- requested_viewport: `1600x900`
- viewport_strategy: `browser-resize-fallback` (isolated iframe capture; no Storybook chrome)
- margin_policy: `trim_only`
- evidence_surface: `page`
- sensitive_exclusion: `N/A`
- submission_gate: `pending-owner-approval`
- story_id_or_title: `Pages/ServiceDetailPage/LifecycleRunning`
- state: `wide topbar, all metric groups`
- evidence_note: `服务名位于 AppShell 顶栏，资源摘要紧随其后并处于操作组之前；宽屏容纳 CPU/内存、磁盘读/写与下载/上传三个完整指标组，正文不再保留重复摘要。边缘空白检查无需裁剪。`

![Lifecycle running topbar metrics wide](./assets/lifecycle-running-topbar-metrics-wide.png)

- source_type: `storybook_canvas`
- target_program: `mock-only`
- capture_scope: `browser-viewport`
- requested_viewport: `1440x900`
- viewport_strategy: `browser-resize-fallback` (isolated iframe capture; Storybook has no desktop-width story variant)
- margin_policy: `trim_only`
- evidence_surface: `page`
- sensitive_exclusion: `N/A`
- submission_gate: `pending-owner-approval`
- story_id_or_title: `Pages/ServiceDetailPage/LifecycleRunning`
- state: `compact desktop, network hidden`
- evidence_note: `顶栏可用宽度不足时先完整隐藏下载/上传组；CPU/内存与磁盘读/写仍作为完整成对组显示，且顶栏没有横向溢出。边缘空白检查无需裁剪。`

![Lifecycle running topbar metrics compact](./assets/lifecycle-running-topbar-metrics-compact.png)

- source_type: `storybook_canvas`
- target_program: `mock-only`
- capture_scope: `browser-viewport`
- requested_viewport: `mobile1 (320x568)`
- viewport_strategy: `devtools-emulate` (matches the story-bound `mobile1` viewport; direct canvas capture avoids Storybook chrome)
- margin_policy: `trim_only`
- evidence_surface: `page`
- sensitive_exclusion: `N/A`
- submission_gate: `pending-owner-approval`
- story_id_or_title: `Pages/ServiceDetailPage/LifecycleRunningMobile`
- state: `mobile icon-logo header, monitor summary hidden`
- evidence_note: `移动端页头首行使用图标 Logo，当前服务名紧邻 Logo 右侧并沿 Y 轴居中；第二行承载更新、生命周期和 Stack 详情操作，所有操作保持 44px 触摸高度，且页面没有横向溢出。边缘空白检查无需裁剪。`

![Lifecycle running topbar metrics narrow](./assets/lifecycle-running-topbar-metrics-narrow.png)

- source_type: `storybook_canvas`
- target_program: `mock-only`
- capture_scope: `browser-viewport`
- requested_viewport: `420x820`
- viewport_strategy: `browser-resize-fallback` (isolated iframe capture; story also declares mobile1)
- margin_policy: `trim_only`
- evidence_surface: `page`
- sensitive_exclusion: `N/A`
- submission_gate: `pending-owner-approval`
- story_id_or_title: `Pages/ServiceDetailPage/LifecycleRunning`
- state: `running`, lifecycle menu open
- evidence_note: `生命周期主按钮和菜单分别使用实心停止方块、启动播放和重启顺时针回转图标；菜单由最长动作项自然撑开，图标与文字沿 Y 轴居中；不可用原因不占用行内布局，菜单不显示额外的默认标记。边缘空白检查无需裁剪。`

![Lifecycle running lifecycle menu narrow](./assets/lifecycle-running-lifecycle-menu-narrow.png)

- source_type: `storybook_canvas`
- target_program: `mock-only`
- capture_scope: `browser-viewport`
- requested_viewport: `420x820`
- viewport_strategy: `browser-resize-fallback` (isolated iframe capture; story also declares mobile1)
- margin_policy: `trim_only`
- evidence_surface: `page`
- sensitive_exclusion: `N/A`
- submission_gate: `pending-owner-approval`
- story_id_or_title: `Pages/ServiceDetailPage/LifecycleUnknown`
- state: `unknown`, unavailable item hovered
- evidence_note: `生命周期菜单宽度由最长操作项自然撑开，只显示图标与动作名；不可执行原因不占用面板行高，悬浮“启动”显示完整浮动 Tooltip。边缘空白检查无需裁剪。`

![Lifecycle unknown tooltip narrow](./assets/lifecycle-unknown-tooltip-page-narrow.png)

- source_type: `storybook_canvas`
- target_program: `mock-only`
- capture_scope: `browser-viewport`
- requested_viewport: `420x820`
- viewport_strategy: `browser-resize-fallback` (isolated iframe capture; story also declares mobile1)
- margin_policy: `trim_only`
- evidence_surface: `page`
- sensitive_exclusion: `N/A`
- submission_gate: `pending-owner-approval`
- story_id_or_title: `Pages/ServiceDetailPage/LifecycleUnknown`
- state: `unknown`, unavailable item clicked
- evidence_note: `点击不可执行的生命周期项关闭菜单并显示同一原因的 Toast；不会提交生命周期任务。边缘空白检查无需裁剪。`

![Lifecycle unknown toast narrow](./assets/lifecycle-unknown-toast-page-narrow.png)

## 变更记录

- 2026-07-30: 创建规格，冻结服务生命周期任务、操作菜单、串行边界、文档和 Storybook 验收契约。
