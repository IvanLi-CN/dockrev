# Dockrev：服务资源监控（SSE 实时推送 + 历史持久化 + 图表）（#kbz3z）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-07-15

## 背景 / 问题陈述

- 现有 Service Detail 缺少持续可视化资源监控，无法同时观察短时波动与历史趋势。
- 用户已锁定首版要求：历史低频持久化、实时 1s SSE 推送、图表展示、磁盘 I/O 必须纳入。

## 目标 / 非目标

### Goals

- 系统设置支持资源监控开关与采样频率（5/10/30/60/300，默认 5）。
- 历史采样按服务持久化，保留 30 天并自动清理。
- 服务详情页提供 1s SSE 实时流与趋势图（CPU/内存/网络/磁盘 I/O/PIDs）。
- 服务详情资源监控面板强化头部层级、SSE 状态可见性、统一工具栏与移动端数值排版。
- 资源监控关闭时，历史与实时接口统一返回 `409 resource_monitor_disabled`。

### Non-goals

- 服务详情页保留完整趋势图与 1s SSE；Overview 仅允许展示聚合最新摘要，不扩展为列表页图表或逐卡片 SSE。
- 不引入告警规则、阈值通知、自动扩缩容。
- 不实现多实例分布式采样去重。

## 范围（Scope）

### In scope

- 后端：DB schema/migration、历史采样任务、SSE endpoint、settings 读写扩展。
- 前端：Settings 资源监控配置、Service Detail 监控卡片与自研 SVG 图表。
- Storybook：服务详情资源监控典型场景与异常场景。

### Out of scope

- 第三方图表依赖引入。
- 默认不新增 PR/spec 图片证据；若需要提交图片，先由主人确认。

## 接口契约（Interfaces & Contracts）

- `GET /api/settings`：新增 `resourceMonitor`。
- `PUT /api/settings`：支持写入 `resourceMonitor.enabled` 与 `resourceMonitor.sampleIntervalSeconds`。
- `GET /api/services/{service_id}/resource-usage/history?window=3m|1h|24h`：返回历史样本序列。
- `GET /api/services/{service_id}/resource-usage/events`：SSE 事件
  - `resource_usage_snapshot`
  - `resource_usage_tick`
  - `resource_usage_error`
- `GET /api/services/resource-usage/overview?window=3m|1h|24h`：Overview 聚合最新摘要，只返回 CPU、内存、网络 RX/TX、样本时间、stale 与样本数量，不提供图表序列或 SSE。
- 监控关闭统一错误：`409` + `details.reason=resource_monitor_disabled`。
  - 例外：Overview 聚合摘要接口返回 `200 enabled=false`，用于导航页非阻塞降级。

## 数据与运行时设计

- `settings` 表新增：
  - `resource_monitor_enabled INTEGER NOT NULL DEFAULT 1`
  - `resource_sample_interval_seconds INTEGER NOT NULL DEFAULT 5`
- 历史采样频率合法值为 `5/10/30/60/300`；已有合法 `10` 继续保留，不做隐式迁移。
- 新增 `service_resource_samples` 表与索引：
  - `(service_id, sampled_at)`
  - `(sampled_at)`
- 后台历史采样任务：
  - 仅在开关开启时运行。
  - 按设置频率采样并入库。
  - 每小时 GC，删除 30 天前样本。
- 资源采集通道：
  - 资源监控历史采样与详情页实时采样统一直接读取 Docker Engine API。
  - 默认通过挂载的 `/var/run/docker.sock` 访问；若部署改走 `docker-socket-proxy`，则复用现有 `DOCKER_HOST=tcp://docker-socket-proxy:2375` 入口。
  - 本次仅替换资源监控采集路径；日志、更新、cleanup 等其它 Docker 操作仍可继续使用现有 CLI 路径。
- 实时采样 Hub：
  - 同服务多 SSE 连接复用单个 1s 采样器。
  - 活跃 SSE 连接必须持有订阅 guard，避免被 10s idle 回收误判为无订阅。
  - 最后订阅断开后 10s 回收。

## UI 规格（Service Detail）

- 顶部 Hero：标题、副说明、实时状态 badge 与窗口/样本/最近更新时间 facts 同屏可见。
- 实时指标卡：CPU、内存作为主指标卡，网络速率、磁盘 I/O、PIDs 作为次级摘要卡。
- 图表工具栏：同一区域内提供指标 tabs（CPU/内存/网络/磁盘 I/O/PIDs）与时间窗口切换（3m/1h/24h，默认 1h）。
- 图表舞台：自研 SVG 趋势图保留单线/双线逻辑，并增强末端锚点、图例当前值与空/错态。所有指标将每个原始样本保持到下一次采样，以 right-continuous 阶梯表达变化，不平均数值、不生成斜线；CPU、内存、网络与磁盘 I/O 仅在阶梯拐角加极小圆角，PIDs 保持严格直角。单线面积填充必须复用对应阶梯路径且保持低视觉权重，双线图不绘制面积。
- SSE：页面可见时订阅，断线退避重连（1s→2s→5s）。

## 验收标准（Acceptance Criteria）

- 设置页可读写资源监控开关与采样频率，默认开启且默认 5 秒。
- 监控关闭后，history/events 返回 `409 resource_monitor_disabled`，前端展示禁用态。
- 服务详情页在开启状态可看到历史曲线与实时滚动，且实时状态无需查看 footer 即可感知。
- 图表支持指标切换与窗口切换，空数据/错误态有明确提示。
- 前台实时 SSE 继续保持 `1s` 采样 cadence，不受历史采样频率设置影响。
- 既有数据库里保存的合法 `10s` 历史采样配置继续生效，不被自动回写成 `5s`。
- 所有指标以水平保持和垂直跳变经过每个有效采样点、缺口处断线，不生成连续中间值；折线端点平切并保留最新样本锚点。
- 磁盘 I/O、网络 I/O 均以速率形式展示。
- 深色/浅色主题与 375px 宽度下版式稳定，无横向滚动，长数值不炸版。

## 质量门槛（Quality Gates）

- `cargo test -p dockrev-api`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`

## Visual Evidence

- source_type: `storybook_canvas`
  target_program: `mock-only`
  capture_scope: `browser-viewport`
  requested_viewport: `1280x720`
  viewport_strategy: `storybook-viewport`
  sensitive_exclusion: `N/A`
  submission_gate: `owner-approved`
  story_id_or_title: `Components/ServiceResourcePanel/HighVariationCurves`
  state: `high variation with a missing memory sample`
  evidence_note: 固定 1280x720 浏览器视口。资源面板是唯一容器；指标摘要、工具栏、`3m / 1h / 24h` 时间窗口、当前值与图表以无框分区呈现。25 个样本等间隔覆盖一小时，连续指标以双侧微圆角的水平保持和垂直跳变呈现稳定基线与少量可解释波动，PIDs 保持直角阶梯。

PR: include

![服务资源监控单调曲线](./assets/service-resource-monotone-curves.png)

- source_type: `storybook_canvas`
  target_program: `mock-only`
  capture_scope: `browser-viewport`
  requested_viewport: `375x900`
  viewport_strategy: `storybook-viewport`
  sensitive_exclusion: `N/A`
  submission_gate: `owner-approved`
  story_id_or_title: `Components/ServiceResourcePanel/HighVariationCurves`
  state: `high variation mobile CPU view`
  evidence_note: 固定 375x900 浏览器视口。CPU、网络与 PIDs 切换均无横向溢出；`3m / 1h / 24h` 时间窗口、摘要、指标切换、当前值与趋势图在唯一面板内按单列稳定排列，未形成嵌套卡片。

PR: include

![服务资源监控单调曲线移动端](./assets/service-resource-monotone-curves-mobile.png)

- source_type: `storybook_canvas`
  target_program: `mock-only`
  capture_scope: `component-surface`
  requested_viewport: `1280x720`
  viewport_strategy: `storybook-viewport`
  sensitive_exclusion: `N/A`
  submission_gate: `owner-approved`
  story_id_or_title: `Components/ServiceResourcePanel/WindowSwitchContract`
  state: `interactive shared window contract`
  evidence_note: 固定 1280x720 浏览器视口。资源监控工具栏直接展示 `3m / 1h / 24h` 三个共享时间窗口按钮，并通过 Storybook play 覆盖切换到 `24h` 与 `3m`，用于锁定窗口 contract 不回退。

PR: include

![服务资源监控时间窗口 Contract](./assets/service-resource-window-contract.png)

## 变更记录（Change log）

- 2026-03-03: 完成后端采样与 SSE 能力、前端设置与服务详情图表、Storybook 监控场景。
- 2026-03-09: 修复资源监控 SSE 订阅 guard 生命周期，避免活跃连接在约 10 秒后被误回收，并补充持续 streaming 回归测试。
- 2026-03-11: 完成 Service Detail 资源监控面板中等强度视觉升级，统一工具栏并强化实时状态/响应式层级。
- 2026-04-28: 修正 Overview 边界：服务详情继续承载图表/SSE，Overview 仅展示聚合最新资源摘要并允许监控关闭时非阻塞降级。
- 2026-07-15: 历史采样 contract 扩展为 `5/10/30/60/300` 且默认 `5s`，共享窗口 contract 切到 `3m/1h/24h`，前台实时 SSE 继续保持 `1s`；资源监控采集路径切换为直接读取 Docker Engine API（默认 socket，兼容 `DOCKER_HOST`）。

## 参考（References）

- `crates/dockrev-api/src/resource_usage.rs`
- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/db.rs`
- `web/src/components/ServiceResourcePanel.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/pages/SettingsPage.tsx`
