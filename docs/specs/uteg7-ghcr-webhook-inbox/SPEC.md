# Dockrev：GHCR Webhook Push Inbox（7 天展示 + 30 天/2000 保留）（#uteg7）

## 状态

- Status: 待实现
- Created: 2026-03-01
- Last: 2026-03-01

## 背景 / 问题陈述

- Dockrev 已接入 GitHub Packages (GHCR) 的 webhook（`POST /api/webhooks/github-packages`），但缺少对 webhook 推送的可观测性：运维侧无法回答“最近有哪些推送？哪些触发了扫描？为什么被忽略？”。
- 当前仅能在 Job/Queue 侧看到扫描任务结果，缺少 webhook 层的 Inbox 作为排查入口与审计线索。
- 数据库存储需要有界：保留最近 30 天或最多 2000 条，避免表无限增长。

## 目标 / 非目标

### Goals

- 新增一页 `GHCR Webhook Inbox`：展示最近 7 天（来自 DB）的 webhook push 记录。
- 在 `GHCR Webhook` 页面新增按钮可打开 Inbox。
- 后端在验签通过且 `X-GitHub-Event=package` 且 `action=published` 的请求上落库记录：
  - `outcome`：`triggered|ignored|unknown` 等
  - `reason`：忽略/触发原因（例如 `repo_not_selected`）
  - `jobId`：触发扫描时关联 discovery job id
- 通过 prune 策略确保数据库最多保留最近 30 天或 2000 条记录。

### Non-goals

- 不存储完整 webhook payload（仅存元数据与 outcome/reason/jobId）。
- 不对验签失败、disabled、非 package、非 published 的请求进行落库审计（避免噪音）。
- 不实现搜索/分页/导出（先固定“最近 7 天”展示）。

## 范围（Scope）

### In scope

- Backend
  - DB：扩展 `github_packages_deliveries` 表结构，新增 inbox 所需字段与索引，并加入 migration
  - Webhook：在验证通过的 `package/published` 请求路径中写入 delivery 记录（含触发/忽略）
  - API：新增 `GET /api/github-packages/webhook/inbox`
  - Tests：覆盖入库语义、jobId 关联、unselected 入库、prune 行为
- Web
  - 路由：`/queue/ghcr-webhook-inbox`
  - 页面：`GhcrWebhookInboxPage`
  - GHCR Webhook 页面入口按钮
  - Storybook mock 增补该 API

### Out of scope

- “通用 webhook 审计系统”或其它 webhook 类型的 inbox。
- Inbox 详情页/展开 payload。

## 需求（Requirements）

### MUST

- 新增 API：`GET /api/github-packages/webhook/inbox` 返回最近 7 天记录，最多 2000 条。
- 入库条件：仅当请求通过验签且 `event=package` 且 `action=published` 时写入 delivery 记录。
- 当 repo/owner 未被选择时：
  - 不触发扫描
  - 仍写入 `outcome=ignored` 且 `reason=repo_not_selected`
- 当触发扫描时：
  - 仍保持 delivery 去重（按 `deliveryId`）
  - delivery 记录关联 `jobId`
- retention：每次成功插入后执行 prune：删除 30 天前记录，并裁剪到最新 2000 条以内。

### SHOULD

- UI 可从 delivery 记录直接跳到 `JobDetail`（当 `jobId` 存在时）。
- UI 可显示空态与错误态。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| GHCR Webhook Inbox | HTTP API | internal | New | None | dockrev-api | dockrev-web | `GET /api/github-packages/webhook/inbox` |
| github_packages_deliveries 扩展 | DB schema | internal | Modify | None | dockrev-api | dockrev-api | 新增 `outcome/reason/job_id` + index |

## 验收标准（Acceptance Criteria）

- Given 已有 webhook 推送落库，When 打开 Inbox 页面，Then 可展示最近 7 天记录并能刷新。
- Given 验签通过且 `package/published` 且 repo 未选中，When 接收 webhook，Then 返回 `ignored/repo_not_selected` 且 Inbox 里可见对应记录（`outcome=ignored`）。
- Given 验签通过且 `package/published` 且 repo 已选中，When 接收 webhook，Then 返回 `jobId` 且 Inbox 记录关联同一 `jobId`，并可从 UI 跳转到任务详情。
- Given 连续插入 >2000 条或包含 >30 天前记录，When 插入触发 prune，Then 表内仅保留最近 30 天且最多 2000 条。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: `crates/dockrev-api/src/api/tests.rs` 覆盖 webhook 入库/忽略原因/jobId 关联/inbox 列表/retention

### UI / Storybook

- Storybook mock 增补 `/api/github-packages/webhook/inbox`

### Quality checks

- `cargo test -p dockrev-api`
- `bun --cwd web run build`

## 实现里程碑（Milestones / Delivery checklist）

- [ ] M1: DB 扩展与 migration（deliveries 字段 + 索引 + prune）
- [ ] M2: Webhook 写入与 API（inbox endpoint）
- [ ] M3: Web UI 路由/页面 + GHCR Webhook 页入口按钮 + Storybook mock
- [ ] M4: 测试与构建回归通过

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 假设：`received_at` 按 RFC3339(UTC) 存储，字符串比较可用于时间范围筛选与排序。
- 风险：高频 webhook 时每次插入都 prune 可能带来额外写放大（预计低频可接受；未来可改为后台定时清理）。

## 变更记录（Change log）

- 2026-03-01: 新建规格，冻结 inbox 范围、入库条件、接口与 retention 策略。

