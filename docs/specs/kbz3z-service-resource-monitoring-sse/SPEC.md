# Dockrev：服务资源监控（SSE 实时推送 + 历史持久化 + 图表）（#kbz3z）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-03-03

## 背景 / 问题陈述

- 现有 Service Detail 缺少持续可视化资源监控，无法同时观察短时波动与历史趋势。
- 用户已锁定首版要求：历史低频持久化、实时 1s SSE 推送、图表展示、磁盘 I/O 必须纳入。

## 目标 / 非目标

### Goals

- 系统设置支持资源监控开关与采样频率（10/30/60/300，默认 30）。
- 历史采样按服务持久化，保留 30 天并自动清理。
- 服务详情页提供 1s SSE 实时流与趋势图（CPU/内存/网络/磁盘 I/O/PIDs）。
- 资源监控关闭时，历史与实时接口统一返回 `409 resource_monitor_disabled`。

### Non-goals

- 不扩展到 Overview/Services 列表页。
- 不引入告警规则、阈值通知、自动扩缩容。
- 不实现多实例分布式采样去重。

## 范围（Scope）

### In scope

- 后端：DB schema/migration、历史采样任务、SSE endpoint、settings 读写扩展。
- 前端：Settings 资源监控配置、Service Detail 监控卡片与自研 SVG 图表。
- Storybook：服务详情资源监控典型场景与异常场景。

### Out of scope

- 第三方图表依赖引入。

## 接口契约（Interfaces & Contracts）

- `GET /api/settings`：新增 `resourceMonitor`。
- `PUT /api/settings`：支持写入 `resourceMonitor.enabled` 与 `resourceMonitor.sampleIntervalSeconds`。
- `GET /api/services/{service_id}/resource-usage/history?window=15m|1h|6h`：返回历史样本序列。
- `GET /api/services/{service_id}/resource-usage/events`：SSE 事件
  - `resource_usage_snapshot`
  - `resource_usage_tick`
  - `resource_usage_error`
- 监控关闭统一错误：`409` + `details.reason=resource_monitor_disabled`。

## 数据与运行时设计

- `settings` 表新增：
  - `resource_monitor_enabled INTEGER NOT NULL DEFAULT 1`
  - `resource_sample_interval_seconds INTEGER NOT NULL DEFAULT 30`
- 新增 `service_resource_samples` 表与索引：
  - `(service_id, sampled_at)`
  - `(sampled_at)`
- 后台历史采样任务：
  - 仅在开关开启时运行。
  - 按设置频率采样并入库。
  - 每小时 GC，删除 30 天前样本。
- 实时采样 Hub：
  - 同服务多 SSE 连接复用单个 1s 采样器。
  - 最后订阅断开后 10s 回收。

## UI 规格（Service Detail）

- 顶部实时指标卡：CPU、内存、网络速率、磁盘 I/O 速率、PIDs。
- 中部图表：指标 tabs（CPU/内存/网络/磁盘 I/O/PIDs），网络/磁盘双线。
- 底部窗口切换：15m/1h/6h（默认 1h）。
- SSE：页面可见时订阅，断线退避重连（1s→2s→5s）。

## 验收标准（Acceptance Criteria）

- 设置页可读写资源监控开关与采样频率，默认开启且默认 30 秒。
- 监控关闭后，history/events 返回 `409 resource_monitor_disabled`，前端展示禁用态。
- 服务详情页在开启状态可看到历史曲线与实时滚动。
- 图表支持指标切换与窗口切换，空数据/错误态有明确提示。
- 磁盘 I/O、网络 I/O 均以速率形式展示。

## 质量门槛（Quality Gates）

- `cargo test -p dockrev-api`
- `bun run --cwd web build`
- `bun run --cwd web build-storybook`

## 变更记录（Change log）

- 2026-03-03: 完成后端采样与 SSE 能力、前端设置与服务详情图表、Storybook 监控场景。

## 参考（References）

- `crates/dockrev-api/src/resource_usage.rs`
- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/db.rs`
- `web/src/components/ServiceResourcePanel.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/pages/SettingsPage.tsx`
