# Dockrev：GHCR Webhook 自动任务化 + 队列可见 + SSE 进度 + 24h 巡检（#g5m9c）

## 状态

- Status: 已完成
- Created: 2026-02-28
- Last: 2026-02-28

## 背景 / 问题陈述

- GHCR webhook 的注册/反注册原先包含同步路径，缺少统一可追踪的 Job 生命周期。
- Settings 页面无法稳定展示 webhook 状态漂移、重试进度与当前执行任务。
- 缺少固定周期巡检，GitHub 侧外部变更会导致状态静默漂移。

## 目标 / 非目标

### Goals

- 将 GHCR webhook 注册 / 反注册 / 巡检统一收敛为 `github_packages_webhook` Job（`queued -> running -> success/failed`）。
- Repo 级状态与 Job 进度通过 SSE（复用 `/api/jobs/events`）在 Settings/Queue 侧实时可见。
- 提供专门 GHCR Webhook 队列入口页，展示 tracked 状态、任务统计与最近任务追溯。
- 引入 24h `audit_all` 巡检任务，仅检测并标记 `ok/missing/conflict/error`，不自动修复。
- Recoverable GitHub API 错误在单 Job 内自动重试 3 次（指数退避 + jitter）。

### Non-goals

- 不自动清理冲突 webhook（`conflict` 仅标记并提示人工处理后重试）。
- 不在巡检中自动重建缺失 webhook（由“重试注册”触发新 Job）。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/ghcr_webhook_jobs.rs`
- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/api/types.rs`
- `crates/dockrev-api/src/db.rs`
- `crates/dockrev-api/src/config.rs`
- `crates/dockrev-api/src/main.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `web/src/api.ts`
- `web/src/pages/SettingsPage.tsx`
- `web/src/pages/QueuePage.tsx`
- `web/src/pages/GhcrWebhookQueuePage.tsx`
- `web/src/routes.ts`
- `web/src/App.tsx`
- `web/src/Shell.tsx`
- `web/src/stories/mocks/dockrevMockApi.ts`

### Out of scope

- GitHub 侧 webhook 冲突自动清理策略。
- 非 GHCR 类型 Job 的执行模型改造。

## 需求（Requirements）

### MUST

- 新增 `JobType::GitHubPackagesWebhook`，并让 Queue/JobDetail 支持 `queued` 状态显示。
- `POST /api/github-packages/repos/selected` 在 `selected=true` 且 GHCR 设置可用时返回 `jobId` 并入队 register Job。
- `POST /api/github-packages/repos/delete` 改为入队 unregister Job（不再同步删除），返回 `jobId`。
- 新增 `GET /api/github-packages/webhook/overview` 聚合 tracked summary + queued/running jobs + runningJobId + lastAuditAt。
- `github_packages_repos` 新增并维护 `webhook_state/webhook_job_id/last_audit_at/last_op`。
- 启动恢复逻辑对 `queued` 保持不变，仅将中断 `running` 标记为 failed（既有策略延续）。
- 24h 定时入队 `audit_all`。

### SHOULD

- Settings GHCR 区域显示当前 GHCR 任务进度文案（jobId/phase/current/target/message）。
- Settings 行级操作提供“查看任务 / 重试注册 / 重试删除”，并按状态确定性渲染。
- Queue 页面新增 “GHCR Webhook 状态” tile 并可跳转专门页面。

## 验收标准（Acceptance Criteria）

- Given 选中 repo 且 GHCR enabled + callbackUrl 有效，When 调用 selected 接口，Then 返回 `jobId` 且 repo 状态进入 `queued`。
- Given 点击删除 tracked repo，When 调用 delete 接口，Then 返回 `jobId` 且 repo 行先保留为 `queued`（待 worker 完成后删除）。
- Given worker 处理中存在 recoverable 错误，When 重试后成功，Then Job 最终 success 且日志含 attempt 信息。
- Given 启动恢复执行，When 存在 `queued` 与 `running` 未完成任务，Then `queued` 保持 queued，`running` 标记 failed。
- Given 专门 overview 接口被调用，Then 返回 tracked 状态聚合、jobsQueued/jobsRunning 与 runningJobId。

## 里程碑（Milestones / checklist）

- [x] M1: 后端 Job 化落地（JobType/queued/worker/scheduler/retry/audit）。
- [x] M2: GHCR API 语义切换（selected/delete/sync + overview）。
- [x] M3: 前端 Settings/Queue/专门页联动与 SSE 刷新。
- [x] M4: DB 字段扩展与恢复语义修正。
- [x] M5: 回归测试补齐（queued 恢复 + selected/delete enqueue + overview）。

## 风险 / 假设

- 风险：GitHub API 速率限制在大量 repo 场景下可能导致 job 延迟，依赖单 Job 重试与 UI 可见性缓解。
- 假设：SSE 事件流可用；若中断，前端 fallback polling 可保持可见性。

## 变更记录（Change log）

- 2026-02-28: 新建规格，冻结 GHCR webhook Job 化范围、状态机与 UI 入口。
- 2026-02-28: 完成后端 worker/scheduler、API 语义切换、DB 字段扩展与前端三页面联动。
- 2026-02-28: 新增/更新测试覆盖 queued 恢复、enqueue 行为与 overview 聚合返回。
