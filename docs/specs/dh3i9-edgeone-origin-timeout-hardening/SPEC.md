# Dockrev：EdgeOne Origin Timeout Hardening（#dh3i9）

## 状态

- Status: 进行中
- Created: 2026-06-26
- Last: 2026-06-26

## 背景 / 问题陈述

- EdgeOne 默认非 HTTP/2 origin-pull 的 HTTP 回源响应超时窗口是 15 秒；请求链路超过该窗口且中途无持续回包时，会在边缘侧落成 524/554。
- `/cleanup` 首屏当前会同步触发全局 aggressive scan，后端再串行跑大量 Docker CLI，是最容易复现 524 的页面入口。
- 现有 SSE heartbeat 也使用 15 秒间隔，存在与边缘 idle window 正面碰撞的风险。

## 目标 / 非目标

### Goals

- 所有 owner-facing HTTP 请求都不再依赖超过 15 秒的无字节等待。
- `/cleanup` 首屏改为 snapshot-backed 读路径，旧 snapshot 可先显示，后台异步刷新。
- cleanup confirm/apply 不再在请求链路里重扫 Docker，改为 fresh snapshot + fingerprint 校验。
- `/deploy-check` 改为 cached read + async refresh。
- Web release drawer 不再依赖 `/github-releases/locate`。
- 所有 SSE 路由统一改为 5 秒 heartbeat，并在连接建立时立即 flush 一次 keepalive comment。

### Non-goals

- 不把修复绑定在 EdgeOne 控制台配置变更上。
- 不在本轮引入通用后台任务框架。
- 不重写 cleanup ownership 归属算法。

## 兼容性 / 覆盖声明

- 本 spec 覆盖 [fmcxc-snapshot-scan-conservative-filter](/Users/ivan/.codex/worktrees/aeb5/dockrev/docs/specs/fmcxc-snapshot-scan-conservative-filter/SPEC.md) 中“live `/digest-tags` 不改动”的旧非目标；为消除 EdgeOne 长同步风险，本轮允许 live `/digest-tags` 退出 owner-facing 主路径。
- 本 spec 覆盖 [qynjg-docker-prune-cleanup-console/contracts/http-apis.md](/Users/ivan/.codex/worktrees/aeb5/dockrev/docs/specs/qynjg-docker-prune-cleanup-console/contracts/http-apis.md) 中“同步 cleanup scan”假设；cleanup 改为 snapshot-backed async contract。

## 需求

### MUST

- cleanup page request:
  - 有 cached snapshot 时立即返回 ready payload，并标记 `refreshing=true`。
  - 无 cached snapshot 时返回 pending，并给出 `retryAfterMs`。
- cleanup confirm request:
  - 只有当 latest snapshot 年龄 `<=30s` 且无 refresh in-flight 时，才返回 ready confirm payload。
  - 否则返回 pending，前端必须 poll 到 ready 后再允许确认。
- cleanup apply:
  - 禁止内联全量重扫。
  - 若 fingerprint 失效，继续返回 `409 cleanup_snapshot_stale + latest payload`。
- deploy-check:
  - GET `/api/deploy-check/report` 必须支持 cached report ready 返回与 pending 返回。
  - POST `/api/deploy-check/report/refresh` 只 enqueue，不同步构建 report。
  - 启动期发现 `missing` 的 Compose project 时，必须同步归档其仍 active 的关联 Stack；即使 discovery 记录已由旧版本归档，遗留的 active Stack 也必须补齐 `auto_archive_on_restart` 状态。
  - 该修复只能更新归档元数据，不得删除 Stack、服务、Compose 文件、容器或运行时资源。
- GitHub release drawer:
  - 打开时只请求 page 1。
  - 若指定 targetVersion，则以前端分页渐进加载定位并高亮，不依赖 `/locate`。
- SSE:
  - heartbeat 常量统一为 5 秒。
  - 连接建立时立即发一条 keepalive/comment。

### SHOULD

- deploy-check 中本地命令 probe 采用并行执行。
- GitHub client 请求 timeout 收敛到 8 秒。
- deploy-check local command timeout 默认值收敛到 8 秒。

## 验收标准

- 访问 `/cleanup` 时，不再因为首屏同步 Docker scan 触发 524。
- cleanup confirm 在 snapshot stale 或 refresh in-flight 时只返回 pending，不直接给旧 confirm payload。
- `/deploy-check` 有 cached report 时可立即展示，refresh 不阻塞首屏。
- Given 一个已归档或未归档的 `missing` discovery 项仍关联 active Stack，When Dockrev 启动并完成数据库初始化，Then 该 Stack 必须被标记为 `auto_archive_on_restart`，其失效 Compose 路径不得继续阻断 deploy-check。
- Given 一个 active discovery 项关联 active Stack，When Dockrev 启动，Then 该 Stack 不得被自动归档。
- 应用首次加载与恢复前台必须刷新并等待最新 deploy-check report；任一 required core check FAIL 或报告不可用时强制进入 `/deploy-check`，`neverAutoOpen` 不得绕过，失败页不得进入 Dashboard。只有全部 required core check PASS 才放行。
- release drawer 在不调用 `/github-releases/locate` 的前提下仍可定位目标版本。
- 任一 SSE 连接在 EdgeOne 前方空闲超过 20 秒时，不会因 15 秒 idle window 被断开。

## Visual Evidence

- source_type: `storybook_canvas`
  target_program: `mock-only`
  capture_scope: `element`
  requested_viewport: `1440x960`
  viewport_strategy: `devtools-emulate`
  sensitive_exclusion: `N/A`
  submission_gate: `pending-owner-approval`
  story_id_or_title: `Pages/CleanupPage/ScanningState`
  state: `snapshot pending on first page load`
  evidence_note: 验证 `/cleanup` 首屏无 ready snapshot 时只显示 pending shell，顶部动作进入 `等待扫描 / 扫描中…` 禁用态，不再阻塞在同步全量扫描上。

![Cleanup scanning state](./assets/cleanup-scanning-state.png)

- source_type: `storybook_canvas`
  target_program: `mock-only`
  capture_scope: `element`
  requested_viewport: `1440x960`
  viewport_strategy: `devtools-emulate`
  sensitive_exclusion: `N/A`
  submission_gate: `pending-owner-approval`
  story_id_or_title: `Pages/DeployWelcomePage/CachedReportRefreshing`
  state: `cached report visible while background refresh runs`
  evidence_note: 验证 `/deploy-check` 在已有 cached report 时直接展示 checklist，并额外标出“正在后台刷新最新检查结果…”，不再把 refresh 放进首屏阻塞链路。

![Deploy check cached refreshing](./assets/deploy-check-cached-refreshing.png)

- source_type: `storybook_canvas`
- target_program: `mock-only`
- capture_scope: `element`
- requested_viewport: `1440x900`
- viewport_strategy: `storybook-viewport`
- margin_policy: `trim_only`
- evidence_surface: `page`
- sensitive_exclusion: `N/A`
- submission_gate: `pending-owner-approval`
- story_id_or_title: `Pages/DeployWelcomePage/Default`
- state: `all required checks pass`
- evidence_note: 验证全部 required 核心检查通过时显示 PASS，且“进入 Dashboard”按钮可用。

![Deploy check pass](./assets/deploy-check-pass-desktop.png)

- source_type: `storybook_canvas`
- target_program: `mock-only`
- capture_scope: `element`
- requested_viewport: `1440x900`
- viewport_strategy: `storybook-viewport`
- margin_policy: `trim_only`
- evidence_surface: `page`
- sensitive_exclusion: `N/A`
- submission_gate: `pending-owner-approval`
- story_id_or_title: `Pages/DeployWelcomePage/BlockedCoreFailure`
- state: `required core failure blocks Dashboard entry`
- evidence_note: 验证 required core FAIL 时进入 BLOCKING 门禁页，`neverAutoOpen` 仍不能绕过，Dashboard 入口保持禁用。

![Deploy check blocked](./assets/deploy-check-blocked-desktop.png)

- source_type: `storybook_canvas`
- target_program: `mock-only`
- capture_scope: `element`
- requested_viewport: `393x852 CSS px`
- viewport_strategy: `storybook-viewport`
- margin_policy: `trim_only`
- evidence_surface: `page`
- sensitive_exclusion: `N/A`
- submission_gate: `pending-owner-approval`
- story_id_or_title: `Pages/DeployWelcomePage/BlockedCoreFailureMobile`
- state: `required core failure on mobile`
- evidence_note: 验证 `393x852` 移动视口下故障门禁、核心失败项和禁用 Dashboard 入口均不溢出或重叠。

![Deploy check blocked mobile](./assets/deploy-check-blocked-mobile.png)

## 变更记录

- 2026-06-26: 新建 spec，冻结 EdgeOne 15 秒约束、cleanup async snapshot、deploy-check cached-read、release drawer fallback 与 SSE heartbeat 口径。
- 缺失 discovery 项与关联 Stack 的归档状态必须在启动期保持一致，避免历史失效 Compose 路径错误阻断 deploy-check。
