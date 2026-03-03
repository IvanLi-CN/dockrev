# Dockrev：GHCR 状态同步（全量 + 单仓库）与队列并发可视（#x2n6v）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-03-03

## 背景 / 问题陈述

- 维护页仓库状态可能长期停留在 `unknown`，缺少显式“立即同步”入口。
- 既有 `/api/github-packages/sync` 未在维护页提供直接按钮，且任务类型无法区分“全量同步”与“单仓库同步”。
- 需要明确并发与去重边界，避免重复触发与队列不可见。

## 目标 / 非目标

### Goals

- 新增全量同步按钮与单仓库同步按钮，触发后在队列可见任务与进度。
- 新增两个同步 API：
  - `POST /api/github-packages/webhook/sync-all`
  - `POST /api/github-packages/webhook/sync-repo`
- 调度约束：
  - 全量同步任务：同一时间仅允许 1 个未完成任务。
  - 单仓库同步任务：同仓库仅允许 1 个未完成任务。
  - 单仓库同步 worker 并发上限 5。
  - 全量同步任务内并发上限 5。
- 允许全量同步与单仓库同步并行存在。

### Non-goals

- 不实现 GitHub 冲突 webhook 自动清理。
- 不改变现有 unregister/audit 任务语义与路径。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/api/types.rs`
- `crates/dockrev-api/src/db.rs`
- `crates/dockrev-api/src/ghcr_webhook_jobs.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `web/src/api.ts`
- `web/src/pages/GhcrWebhookRegistryPage.tsx`
- `web/src/pages/GhcrWebhookQueuePage.tsx`
- `web/src/jobDisplay.ts`
- `web/src/stories/mocks/dockrevMockApi.ts`
- `docs-site/docs/api-reference.md`
- `docs/specs/README.md`

### Out of scope

- `web/src/pages/SettingsPage.tsx` 预览区交互变更。
- 非 GHCR 任务类型并发模型改造。

## 接口契约（Interfaces & Contracts）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 备注（Notes） |
| --- | --- | --- | --- | --- |
| `POST /api/github-packages/webhook/sync-all` | HTTP API | internal | New | 返回 `{ok, jobId, status, reused}` |
| `POST /api/github-packages/webhook/sync-repo` | HTTP API | internal | New | 请求 `{fullName}`，返回 `{ok, jobId, status, reused}` |
| `JobType::GitHubPackagesWebhookSyncAll` | Job Type | internal | New | machine name: `github_packages_webhook_sync_all` |
| `JobType::GitHubPackagesWebhookSyncRepo` | Job Type | internal | New | machine name: `github_packages_webhook_sync_repo` |
| `GET /api/github-packages/webhook/overview` | HTTP API | internal | Updated | `jobsQueued/jobsRunning` 聚合覆盖全部 GHCR job type |

## 验收标准（Acceptance Criteria）

- Given 点击“全部状态同步”，When 已有未完成全量同步任务，Then 返回复用任务 `reused=true` 且不新建任务。
- Given 点击某仓库“同步状态”，When 同仓库已有未完成单仓库同步任务，Then 返回复用任务 `reused=true`。
- Given 全量同步任务在队列中，When 触发单仓库同步，Then 可新建单仓库同步任务（不被全量任务阻断）。
- Given 进入更新队列或 GHCR 队列页，Then 可看到 `github_packages_webhook_sync_all` / `github_packages_webhook_sync_repo` 任务与进度。
- Given 维护页存在进行中的同步任务，Then 对应按钮显示 queued/running 状态并可跳转任务详情。

## 非功能性验收 / 质量门槛（Quality Gates）

### Backend

- `cargo test -p dockrev-api github_packages_webhook_sync_`
- `cargo test -p dockrev-api github_packages_repo_selected_enqueues_register_job_when_enabled`
- `cargo test -p dockrev-api github_packages_repo_delete_enqueues_unregister_job_and_keeps_row_until_worker_finishes`
- `cargo test -p dockrev-api github_packages_webhook_overview_reports_repo_and_job_summary`

### Web

- `bun run --cwd web lint`
- `bun run --cwd web build`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 后端新增 sync-all/sync-repo API 与 job type。
- [x] M2: 后端实现全量/单仓库任务去重与并发限制。
- [x] M3: 维护页新增全量/单仓库同步按钮与任务态联动。
- [x] M4: 队列页面与任务展示接入新 job type。
- [x] M5: API 文档、mock 与后端测试补齐。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 假设：单进程实例下通过 enqueue 原子检查可满足“未完成任务去重”需求。
- 风险：在极高并发与多实例部署下，仍可能需要 DB 级唯一约束进一步兜底。
- 假设：全量同步以“触发时的已跟踪仓库快照”为执行集合。
