# Dockrev：GHCR Repos 区域 Inbox 入口 + Webhook Delivery 记录页（#p7k2m）

## 状态

- Status: 已完成
- Created: 2026-03-01
- Last: 2026-03-01

## 背景 / 问题陈述

- Settings 页 GHCR Repos 区域右侧当前展示“匹配 / 已跟踪”统计文案，不符合当前操作优先级。
- 现有 GHCR 页面偏向“注册/反注册任务与状态”，缺少“Webhook 到达记录（delivery）”的独立视角。
- 需要一个更直接的 Inbox 入口用于查看 webhook 触发记录，便于快速确认 GitHub 回调是否到达。

## 目标 / 非目标

### Goals

- 在 GHCR Repos 过滤行移除“匹配 / 已跟踪”统计文案。
- 在同位置新增 `Inbox` 按钮，跳转到新的 Webhook Inbox 页面。
- 新增后端分页 API：列出 `github_packages_deliveries` 到达记录。
- 新增前端页面展示 `receivedAt / owner-repo / deliveryId`，支持刷新与分页。

### Non-goals

- 不做 delivery 与扫描任务 `jobId` 的关联展示。
- 不做 ignored reason / 处理结果扩展。
- 不改动 GHCR Webhook 状态页（注册/反注册任务语义保持不变）。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/api/types.rs`
- `crates/dockrev-api/src/db.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `web/src/routes.ts`
- `web/src/Shell.tsx`
- `web/src/App.tsx`
- `web/src/api.ts`
- `web/src/pages/SettingsPage.tsx`
- `web/src/pages/GhcrWebhookInboxPage.tsx`

### Out of scope

- DB schema 变更或数据迁移。
- GHCR Webhook worker/job 流程调整。
- 新的权限模型与多用户鉴权模型重构。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GET /api/github-packages/webhook/deliveries` | HTTP API | internal | New | None | api | web settings/inbox | 分页查询 delivery 记录 |
| `listGitHubPackagesWebhookDeliveries` | TS API SDK | internal | New | None | web | `GhcrWebhookInboxPage` | 与新后端 API 对齐 |
| `Route.name = ghcr-webhook-inbox` | Route | internal | New | None | web | Settings / App / Shell | Inbox 页面导航入口 |

## 验收标准（Acceptance Criteria）

- Given Settings GHCR Repos 过滤行，When 页面渲染，Then 不再显示“匹配 / 已跟踪”统计文案。
- Given Settings GHCR Repos 过滤行，When 点击 `Inbox`，Then 跳转到 `/queue/ghcr-webhook-inbox`。
- Given delivery 表有数据，When 请求 `GET /api/github-packages/webhook/deliveries?page=1&perPage=2`，Then 返回按 `receivedAt DESC, deliveryId DESC` 排序的数据与正确分页元信息。
- Given delivery 表为空，When 请求列表接口，Then 返回 `total=0` 且 `deliveries=[]`。
- Given `auth_allow_anonymous_in_dev=false` 且未提供用户头，When 请求列表接口，Then 返回 `401`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-api github_packages_webhook_deliveries`
- `bun run --cwd web lint`
- `bun run --cwd web build`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 后端新增 delivery 分页查询（types + db + route handler）。
- [x] M2: 后端 API tests 覆盖排序/分页、空列表、鉴权拒绝。
- [x] M3: 前端新增 Inbox 路由与页面，并接入新 API。
- [x] M4: Settings GHCR Repos 区域替换统计文案为 `Inbox` 按钮。
- [x] M5: web lint/build 与 API 相关测试通过。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 假设：`received_at` 字段可稳定用于倒序分页；当时间相同以 `delivery_id` 二次排序。
- 风险：当前仅提供“到达记录”视角，不包含任务关联，排障时可能仍需手动切换任务页面。
