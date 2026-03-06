# Dockrev：通知事件开关 + 新版本发现通知 + GHCR Webhook 异常通知（#p2n8k）

## 状态

- Status: 已完成
- Created: 2026-03-06
- Last: 2026-03-06

## 背景 / 问题陈述

- 现有通知只有“渠道开关”（Email/Webhook/Telegram/Web Push），无法按通知事件类型做独立控制。
- 定时检查更新任务发现新版本后，没有聚合消息通知，操作者需要手动查看任务详情。
- GHCR Webhook 巡检（audit_all）发现异常仓库后，没有单独的异常聚合通知。
- 需要让设置页可以分别控制：
  - 更新完成通知
  - 发现新版本通知
  - GitHub Webhook 异常通知

## 目标 / 非目标

### Goals

- 在通知设置中新增事件级开关，并落库到后端。
- 新增 `new_version_discovered` 通知类型：
  - 由定时检查任务触发；
  - 按一次检查任务聚合发送；
  - 包含可点击链接（服务详情 / 任务详情）。
- 新增 `ghcr_webhook_anomaly` 通知类型：
  - 由定时 GHCR audit_all 触发；
  - 聚合异常仓库（missing/conflict/error）；
  - 包含可点击链接（设置页 / 任务详情）。
- 事件开关与渠道开关同时生效（双开才发送）。

### Non-goals

- 不新增新的通知渠道 provider。
- 不新增更多通知触发源（仅覆盖本需求中的两个新来源）。
- 不改动已有服务详情与任务详情路由结构。

## 范围（Scope）

### In scope

- Backend:
  - `notification_settings` 增加 3 个事件开关列（默认开启）；
  - 通知配置 API 扩展 `events` 字段；
  - `job_finished` 发送路径增加事件开关 gating；
  - 新增两类通知 payload + 渠道渲染（Webhook/Telegram/Email/Web Push）；
  - 定时检查与 GHCR audit 的触发链路接入新通知。
- Frontend:
  - `Settings > 通知` 增加三项事件开关；
  - 保持自动保存机制；
  - 类型定义与兼容处理更新。
- Docs:
  - 中英文 notifications 文档增加新通知类型与事件开关说明。

### Out of scope

- 不做额外路由页新增；
- 不做通知模板自定义编辑器；
- 不做新通知类型的手工触发 API。

## 需求（Requirements）

### MUST

- 通知事件开关：
  - `update` 控制 `job_finished`；
  - `newVersion` 控制 `new_version_discovered`；
  - `ghcrWebhookAnomaly` 控制 `ghcr_webhook_anomaly`。
- `new_version_discovered`：
  - schema 固定为 `dockrev.notification.new_version_discovered.v2`；
  - 仅在“定时检查”发现新版本服务数 > 0 时发送；
  - `human.summary` 必须直接包含服务名预览（不只给数量）；
  - `primaryUrl`：单服务时指向服务详情，多服务时指向任务详情。
- `ghcr_webhook_anomaly`：
  - schema 固定为 `dockrev.notification.ghcr_webhook_anomaly.v2`；
  - 仅在 schedule audit_all 且 anomaly 总数 > 0 时发送；
  - 聚合异常仓库列表，主跳转指向设置页。
- 新旧客户端兼容：
  - 若 `PUT /api/notifications` 未提交 `events` 字段，后端保持已有事件开关不变。

### SHOULD

- Telegram/Email 文案保持人类可读，不退化为原始 JSON。
- 用户可读通知默认不展示内部 `jobId`，改用“任务详情/检查任务/巡检任务”等动作文案 + 可点击链接。
- 列表有截断策略并写入 omitted 计数，避免超长消息。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GET /api/notifications` | HTTP API | external | Modify | None | dockrev-api | web | 新增 `events` 字段 |
| `PUT /api/notifications` | HTTP API | external | Modify | None | dockrev-api | web | 支持事件开关 + 兼容旧请求 |
| webhook payload: `dockrev.notification.new_version_discovered.v2` | webhook schema | external | Add | None | dockrev-api | webhook consumers | 新增通知类型 |
| webhook payload: `dockrev.notification.ghcr_webhook_anomaly.v2` | webhook schema | external | Add | None | dockrev-api | webhook consumers | 新增通知类型 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 设置页通知区域，When 切换三类事件开关并保存，Then 服务端配置与 UI 状态一致。
- Given 定时检查发现新版本服务，When 检查任务完成，Then 各已开启渠道收到 `new_version_discovered` 通知。
- Given 定时 GHCR audit_all 发现 missing/conflict/error 仓库，When 任务完成，Then 各已开启渠道收到 `ghcr_webhook_anomaly` 通知。
- Given 事件开关关闭，When 触发对应事件，Then 不发送该事件到任意渠道。
- Given Web Push 收到上述通知，When 用户点击，Then 按 payload.url 打开对应页面。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: DB schema & migration 增加 3 个事件开关列
- [x] M2: API types & put/get notifications 扩展 events（含兼容逻辑）
- [x] M3: `job_finished` 路径接入 update 事件开关
- [x] M4: 新增 `new_version_discovered` 通知模型、渲染与发送
- [x] M5: 定时检查链路接入新版本通知触发
- [x] M6: 新增 `ghcr_webhook_anomaly` 通知模型、渲染与发送
- [x] M7: GHCR schedule audit 链路接入异常通知触发
- [x] M8: Settings 页面新增三项事件开关
- [x] M9: 中英文通知文档更新
- [x] M10: 回归验证（Rust check/test + Web build）

## 验证记录

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo check -p dockrev-api`
- `cargo test -p dockrev-api notify::tests`
- `cd web && bun run build`

## Change log

- 2026-03-06：根据可读性反馈，`new_version_discovered` 摘要改为直接包含服务名预览（不再只显示数量）。
- 2026-03-06：根据可读性反馈，用户可读渠道去掉内部 jobId 展示；统一改为“任务详情/检查任务/巡检任务”动作文案，Web Push body 同步简化。
- 2026-03-06：CI clippy 要求收紧后，将 GHCR 异常通知参数重构为事件对象，行为保持不变，仅消除 `too_many_arguments` 告警。
