# Dockrev：Web PWA 离线壳、更新提示与分级缓存（#r8kpa）

## 状态

- Status: 部分完成（5/6）
- Last: 2026-07-08

## 背景 / 问题陈述

- 当前前端只在通知设置场景里临时注册 `public/sw.js`，它只承载 Web Push 与通知点击跳转，不具备真正的 PWA app shell、离线启动、安装能力或受控更新体验。
- 首页已有 `localStorage` 级快照，但缓存范围只覆盖 homepage launcher，其他主要只读页刷新后仍完全依赖联网，离线首开与弱网恢复都不稳定。
- 更新检测当前主要依赖页面 resume refresh 逻辑与 `/api/version` 展示，不存在以 service worker 就绪状态为真相源的全局升级提示。

## 目标 / 非目标

### Goals

- 把现有 Web 前端升级为真正可安装的 PWA：提供有效 `manifest.webmanifest`、安装图标、app shell precache、SPA navigation fallback，并保留现有 Web Push 行为。
- 引入统一的客户端只读快照缓存层：静态资源走 precache，持久只读快照落 IndexedDB，易失上下文仅保留内存态。
- 离线覆盖精确限定为主要只读页：`/`、`/overview`、`/services`、stack detail、service detail 的只读子页、`/queue`、`/queue/version-inference`。
- 更新检查以 service worker 更新为主：页面激活时检查，页面可见时每 1 小时低频检查；发现新版本时只做非阻塞提示，需用户确认后才切换。
- 首页旧 `localStorage` 快照在首轮读取时迁移到统一持久缓存层。

### Non-goals

- 不实现离线写操作、离线任务提交、离线设置保存、离线日志 tail、离线 cleanup/ghcr/admin 管理页。
- 不引入页面关闭后的后台周期更新、Periodic Background Sync、离线消息同步或后台唤醒。
- 不修改后端业务 API 语义、鉴权模型或 supervisor 部署方式。

## 范围（Scope）

### In scope

- `docs/specs/r8kpa-web-pwa-offline-shell/**`
- `web/package.json`
- `web/vite.config.ts`
- `web/index.html`
- `web/public/**`
- `web/src/main.tsx`
- `web/src/sw.ts`
- `web/src/App.tsx`
- `web/src/Shell.tsx`
- `web/src/api.ts`
- `web/src/pages/**`
- `web/src/components/**`
- `web/src/stories/**`
- `web/tests/**`

### Out of scope

- Rust 服务端、数据库 schema、API contract 变更。
- 推送通知业务语义与通知事件配置模型。
- Cleanup、Settings、Deploy Welcome、GHCR Registry 维护等联网必需页的离线化。

## 需求 / 行为合同

- service worker 使用 `vite-plugin-pwa` `injectManifest` 路线统一生成，并在同一个 worker 中同时承载 precache、SPA navigation fallback、Push、通知点击跳转与手动 `skipWaiting`。
- 应用启动即注册 service worker；通知设置页改为复用全局注册结果，不再自行注册单独 worker。
- 持久快照统一记录 `fetchedAt`、`staleAt`、`expireAt`、`schemaVersion`、`sourceVersion`；只有仍处于 `fresh` 窗口内的快照允许展示，超过新鲜窗口或超过 7 天都必须回退为需联网态。
- 纳入持久缓存的 read model 固定为：首页 launcher、概览/服务列表、stack detail、service detail 的 overview/history/monitoring/backup 只读摘要、队列列表、版本推测总览。
- service detail 的 history 仅回放现有 60 秒 fresh snapshot 中的 jobs；离线时不得建立 jobs SSE，联网且 history 激活时才允许实时刷新。
- 日志内容、认证态、通知敏感字段、设置表单值、写操作上下文与高时效流式数据不得落持久缓存。
- 离线时页面只允许展示仍处于 `fresh` 窗口内的本地只读数据，并把所有需要联网的写操作显式禁用或隐藏；不可继续展示非新鲜快照，也不得向用户暴露“数据过时”类提示。
- 更新检测触发器固定为：`focus`、`visibilitychange -> visible`、`pageshow(persisted)` 与页面可见期间每 1 小时轮询；同一版本 ready 期间全局提示去重。
- 新版本就绪判定以 service worker 更新结果为准；`/api/version` 仅继续用于展示文本，不作为切换真相源。

## 验收标准（Acceptance Criteria）

- Given 用户首次联网访问并完成资源缓存，When 随后断网刷新应用，Then app shell 仍可启动并进入纳入范围的只读路由。
- Given 离线进入缓存命中的只读页，When 数据来自本地快照，Then 页面只显示仍处于 `fresh` 窗口内的缓存数据，且所有写操作不会被误导性地保留为可点击。
- Given 本地快照已经离开 `fresh` 窗口或超过 7 天，When 用户离线进入对应页面，Then 页面不再展示该快照，而是直接回到需联网态。
- Given Chromium PWA installability 检查，When 页面具备有效 manifest、icons 与 service worker，Then 用户可安装到桌面/主屏。
- Given 发布了新的前端构建，When 页面在激活事件或 1 小时轮询中检测到新版本，Then 只出现单一全局提示，且只有用户确认后才切换到新资源。
- Given 现有 Web Push 已启用，When 升级到新的 worker 实现，Then 订阅、通知展示与通知点击跳转行为保持可用。

## Visual Evidence

视觉证据在实现完成后补充到本 spec 的 `assets/` 目录，并绑定到 Storybook mock-only 场景。

### Update Prompt Banner

![PWA update prompt banner](assets/pwa-update-banner.png)

- source_type: storybook_canvas
- target_program: mock-only
- capture_scope: `.shellStatusBanner-update`
- requested_viewport: 1440x900
- viewport_strategy: storybook-static
- PR: include

### Offline Snapshot Notice

![Offline snapshot notice](assets/offline-snapshot-notice.png)

- source_type: storybook_canvas
- target_program: mock-only
- capture_scope: `.readonlySnapshotNotice-warn`
- requested_viewport: 1440x900
- viewport_strategy: storybook-static
- PR: include

### Service Monitoring Offline Snapshot

![Service monitoring offline snapshot](assets/service-resource-offline-snapshot.png)

- source_type: storybook_canvas
- target_program: mock-only
- capture_scope: `.svcResourceCard`
- requested_viewport: 1440x900
- viewport_strategy: storybook-static
- PR: include

### Service Update History Fresh Snapshot

![Service update history fresh snapshot (page-level)](assets/service-detail-update-history-snapshot.png)

- source_type: storybook_canvas
- target_program: mock-only
- capture_scope: `browser-viewport`
- requested_viewport: 1440x1200
- viewport_strategy: storybook-viewport
- state: page-level history deep link with persisted fresh jobs read model
- PR: include

### Service Settings Offline Gate

![Service settings offline gate](assets/service-detail-settings-offline-readonly.png)

- source_type: storybook_canvas
- target_program: mock-only
- capture_scope: `.page`
- requested_viewport: 1440x1080
- viewport_strategy: storybook-static
- PR: include
