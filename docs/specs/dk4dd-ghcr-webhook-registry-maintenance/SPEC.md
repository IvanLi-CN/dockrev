# Dockrev：GHCR Webhook 注册维护专页（Settings 预览 + 专页维护）（#dk4dd）

## 状态

- Status: 待实现
- Created: 2026-03-02
- Last: 2026-03-02

## 背景 / 问题陈述

- 当前 `Settings` 页 GHCR Repos 区域承载了分页、删除、重试注册等高风险操作，信息密度高且易误触。
- 删除动作需要“先反注册 webhook，再删除跟踪 repo”，但现有入口分散在设置页，缺少统一维护视角。
- 需要在保留现有后端语义前提下，把维护型操作集中到专门页面，并将设置页降级为预览入口。

## 目标 / 非目标

### Goals

- 在 `Settings` 页 GHCR Repos 区域改为“预览模式”：最多显示 6 条，不展示分页控件。
- 在预览区提供“查看更多”按钮，跳转到新页面 `/settings/ghcr-webhooks`。
- 新增 GHCR Webhook 注册维护页，统一展示 repo 状态字段与行级操作。
- 删除操作采用“两步确认弹窗 + 二次确认按钮”，仅第二步确认后才调用删除接口。
- 保持后端契约不变，继续使用“删除先反注册，worker 成功后 repo 行消失”的确定性语义。

### Non-goals

- 不新增后端 API。
- 不修改 GHCR worker 行为与 DB schema。
- 不替换 `/queue/ghcr-webhooks` 队列页（保留既有入口与语义）。
- 不改造 Inbox 页面。

## 范围（Scope）

### In scope

- `web/src/pages/GhcrWebhookRegistryPage.tsx`（new）
- `web/src/pages/SettingsPage.tsx`
- `web/src/routes.ts`
- `web/src/App.tsx`
- `web/src/Shell.tsx`
- `web/src/App.css`
- `web/src/stories/pages/GhcrWebhookRegistryPage.stories.tsx`（new）
- `web/src/stories/pages/SettingsPage.stories.tsx`
- `web/src/stories/mocks/dockrevMockApi.ts`
- `docs/specs/README.md`

### Out of scope

- `crates/dockrev-api/**` 实现逻辑变更。
- 新鉴权模型、多用户模型调整。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `Route.name = ghcr-webhook-registry` | Route | internal | New | None | web | Settings / App / Shell | Settings 维护主入口 |
| `GET /api/github-packages/repos` | HTTP API | internal | No change | None | api | web settings/registry | 维护页列表与预览 |
| `POST /api/github-packages/repos/selected` | HTTP API | internal | No change | None | api | web registry | 重新注册 |
| `POST /api/github-packages/repos/delete` | HTTP API | internal | No change | None | api | web registry | 删除（反注册任务） |
| `GET /api/github-packages/webhook/overview` | HTTP API | internal | No change | None | api | web registry | 汇总状态 |

## 验收标准（Acceptance Criteria）

- Given `Settings > GHCR Repos`，When 页面渲染，Then 不显示分页控件，最多展示 6 条记录。
- Given 记录总数大于 6，When 用户点击“查看更多”，Then 跳转到 `/settings/ghcr-webhooks`。
- Given 维护页记录状态允许重试注册，When 点击“重新注册”，Then 调用 selected 接口并将状态推进到 `queued/running`。
- Given 用户点击“删除”，When 仅完成第一步确认，Then 不触发删除接口。
- Given 用户点击“删除”且完成第二步确认，When 调用删除接口后 worker 失败，Then 记录保留并显示错误信息。
- Given 用户点击“删除”且完成第二步确认，When worker 成功，Then 该 repo 记录从列表中消失。
- Given 维护页渲染，Then 每行可见字段包含 `repo/state/hookId/lastOp/lastSyncAt/lastAuditAt`，有错误时显示 `lastError`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bun run --cwd web lint`
- `bun run --cwd web build`
- `cargo test -p dockrev-api github_packages_repo_delete_enqueues_unregister_job_and_keeps_row_until_worker_finishes`
- `cargo test -p dockrev-api github_packages_repo_selected_enqueues_register_job_when_enabled`

## 实现里程碑（Milestones / Delivery checklist）

- [ ] M1: 新增 GHCR Webhook 维护页（列表/筛选/行级操作/刷新）。
- [ ] M2: 路由、App、Shell 接线完成，`/settings/ghcr-webhooks` 可访问且归属 settings 导航。
- [ ] M3: Settings GHCR Repos 区改为 6 条预览 + “查看更多”。
- [ ] M4: 删除动作二次确认升级为“两步确认弹窗 + 二次按钮”。
- [ ] M5: Storybook stories/mocks 覆盖新页面与预览态。
- [ ] M6: lint/build/API 关键测试通过。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 假设：维护页不新增后端字段，`lastError` 即可满足排障一线信息需求。
- 风险：SSE 中断时可能导致 UI 更新不及时，需要保留定时轮询降级。
- 假设：删除成功判定以服务端最终状态（repo 行消失）为准，不采用前端乐观移除。
