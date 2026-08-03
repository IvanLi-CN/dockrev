# Dockrev: 任务日志实时输出与事件可见性

## 背景

任务详情页当前通过数据库轮询的 SSE 展示 `job_logs`。服务操作执行器虽然能逐行读取 Docker/Compose stdout/stderr，但只在命令结束后写入聚合摘要，因此运行中的原始输出会一次性出现。

## 目标

- 为更新、回滚、启动、停止、重启等服务操作增加无持久化的 `job_live_log` SSE 输出，按逻辑行实时展示 stdout/stderr。
- 保持现有 `job_logs` 聚合摘要、断线恢复、Last-Event-ID 和审计内容不变。
- 增加短暂的 `job_live_command_complete` 事件，使前端在同一连接内抑制已实时展示过输出的后续命令摘要。
- 在任务日志工具栏增加 `显示 EVEN` 开关。`level=event` 的持久化记录默认隐藏，偏好通过当前浏览器 `localStorage` 记忆。

## 范围与非目标

实时输出覆盖所有经过服务操作执行器的 lifecycle/update/rollback 命令。原始输出不逐行写入数据库，不断线缓存、不补播；连接断开或刷新后只恢复数据库中的历史摘要。数据库日志 REST 结构不增加 `commandId`，进度计算与任务执行语义不变。

## 行为契约

### SSE

- `job_live_log` 是仅内存广播的 per-job SSE 事件，不设置 SSE `id`，payload 至少包含 `ts`、`stream`（`stdout|stderr`）和 `msg`。
- stdout/stderr 按逻辑行发送；命令末尾的无换行残片作为最后一行发送。
- `job_live_command_complete` 是仅内存广播的短暂完成标记，包含本次命令是否产生实时输出以及是否已写入聚合摘要（`hadOutput`、`summaryPersisted`）。它不设置 SSE `id`，不会影响 Last-Event-ID；只有 `summaryPersisted=true` 时前端才会抑制后续摘要。
- hub 在任务终态释放；没有断线补播或历史缓存。
- 既有带数据库 id 的 `job_log`、命名事件和断线恢复保持兼容。

### 前端日志

- 当前 EventSource 连接中，收到实时行后，下一条匹配的 `status=... stdout=... stderr=...` 持久化摘要只渲染一次；刷新或重连恢复的数据库摘要不做推断去重。
- `level=event` 记录只有在“显示 EVEN”打开时渲染；开关默认关闭，读取或写入 `localStorage` 失败时安全回退为关闭。
- 开关跨任务详情复用同一浏览器偏好。

## 验收标准

- 运行服务操作时原始 stdout/stderr 逐行即时到达任务详情，结束后不在数据库生成额外逐行记录。
- 同一未刷新连接不重复显示实时输出和命令摘要；刷新/重连后历史摘要完整可见。
- EVEN 默认不可见，开关立即生效并跨任务、刷新保留；存储不可用时不影响日志页面。
- Rust SSE/hub 生命周期、无持久化、Web 去重/筛选、Storybook play 和 ui_demo 逐行增长/开关/跟随行为均有验证。

## 参考

- Legacy plan: `docs/plan/0001:dockrev-compose-updater/PLAN.md`
- Legacy event contract: `docs/plan/0001:dockrev-compose-updater/contracts/events.md`
- Legacy HTTP contract: `docs/plan/0001:dockrev-compose-updater/contracts/http-apis.md`
- Legacy DB contract: `docs/plan/0001:dockrev-compose-updater/contracts/db.md`

## Visual Evidence

- 来源：现有 mock-only `ui_demo`（`queue-long-logs`），未使用真实后端或登录态。
- 桌面证据覆盖实时行增长、日志区域自动跟随和默认关闭的“显示 EVEN”开关：[desktop](assets/job-detail-live-desktop-trimmed.png)。
- `393x852` 证据覆盖刷新后的数据库历史摘要恢复、默认关闭的开关和窄屏布局：[mobile](assets/job-detail-live-mobile-trimmed.png)。
- 图片经 `trim_whitespace.py --margin-policy trim_only` 处理，结果为 `unchanged`；Storybook 交互场景为 `Pages/JobDetailPage/LiveOutputAndEventToggle`，覆盖默认隐藏、打开开关、实时输出与摘要去重。
- 主人验收使用的不可变快照：`/Users/ivan/.codex/user-inline-assets/dockrev__f83adb76/2026/08/03/20260803T040418Z-job-detail-live-desktop-trimmed-9d3dbda5.png`、`/Users/ivan/.codex/user-inline-assets/dockrev__f83adb76/2026/08/03/20260803T040418Z-job-detail-live-mobile-trimmed-cbe165ac.png`。
