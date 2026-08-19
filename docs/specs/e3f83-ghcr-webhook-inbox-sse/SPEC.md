# Dockrev：GHCR Webhook 收件箱 SSE 实时更新（#e3f83）

## 状态

- Status: 已完成
- Created: 2026-03-09
- Last: 2026-03-09

## 背景 / 问题陈述

- 现有 `/queue/ghcr-webhook-inbox` 只支持首屏拉取与手动刷新，新 delivery 到达后页面不会自动更新。
- GHCR webhook 处理链路已经会写入 `github_packages_deliveries`，但没有专门给收件箱消费的事件流，用户需要反复点刷新确认回调是否到达。
- 现有 `/api/jobs/events` 无法稳定表达 delivery 列表摘要、重复投递次数与最终处理结果变更，不适合作为收件箱的真实源。

## 目标 / 非目标

### Goals

- 为 GHCR webhook 收件箱新增专用 delivery SSE 通道，支持 `afterId` / `Last-Event-ID` 续传与默认 tail-follow。
- 新增单调递增 delivery 事件日志，覆盖三类用户可见变化：新记录、重复投递 attempt 增长、最终 outcome 更新。
- 收件箱页面改为“REST 首屏 + SSE 触发重新拉取当前列表/summary”，保持现有分页、筛选、搜索语义稳定。
- API 文档与测试同步覆盖新事件流契约。

### Non-goals

- 不改 GHCR 状态页与 Settings 入口的实时联动。
- 不在前端做行级 optimistic merge 或本地状态推演。
- 不改变现有 jobs / version inference SSE 的事件名或行为。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/db/mod.rs`
- `crates/dockrev-api/src/db/github_packages.rs`
- `crates/dockrev-api/src/models/github_packages.rs`
- `crates/dockrev-api/src/api/github_packages.rs`
- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/api/types/github_packages.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `web/src/api.ts`
- `web/src/pages/GhcrWebhookInboxPage.tsx`
- `docs-site/docs/api-reference.md`

### Out of scope

- `web/src/pages/GhcrWebhookQueuePage.tsx` 与 `web/src/pages/SettingsPage.tsx` 的实时刷新逻辑。
- 新增 Storybook 场景。
- 任何依赖 `docs/specs/` 的运行时代码。

## 需求（Requirements）

### MUST

- 新增 `GET /api/github-packages/webhook/deliveries/events`，返回 `text/event-stream`，事件名固定为 `github_packages_delivery_event` 与 `github_packages_delivery_events_error`。
- SSE cursor 使用单调递增事件 `id`，支持 query `afterId` 与 `Last-Event-ID`，默认行为为 tail-follow（只追随未来事件）。
- 新 delivery 在形成用户可见记录时发送事件；重复投递 `attemptCount` 增长时发送事件；最终 `decision/reason/responseStatus/jobId/jobIds` 落库时发送事件。
- 前端收件箱在保持当前 `page/perPage/filter/query` 的前提下，收到 SSE 后防抖重新拉取当前列表与 summary。
- 未鉴权访问 SSE 路由必须返回 `401`。

### SHOULD

- SSE 错误事件与网络错误都触发页面尽快 refresh，一次连接内避免高频重复拉取。
- 事件 payload 直接携带当前 delivery 关键字段，便于未来扩展非整页刷新。

### COULD

- 后续 GHCR 状态页复用该事件源，但本计划不落地。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 用户打开收件箱页：先调用现有 deliveries 列表接口加载当前页，再建立 delivery SSE 连接。
- 新 delivery 到达且进入收件箱：后端写入 delivery 记录后立即写一条事件日志；前端收到 `github_packages_delivery_event` 后 250ms 内刷新当前列表。
- 同一 `deliveryId` 重复投递：后端仅增加 attempt 并发出事件，不覆盖既有 outcome；前端刷新后可看到新的 `attemptCount`。
- 处理链路完成并写回 `processed/ignored/rejected`：后端更新 outcome 后发事件；前端刷新后显示最新 decision / reason / responseStatus / job links。

### Edge cases / errors

- SSE `afterId` 过旧但仍能从事件表读取时，按顺序回放；当前计划不引入专门 resync-required 事件。
- SSE 查询为空时保持连接存活，使用 keep-alive；连接错误时前端依赖浏览器自动重连，并额外触发一次 refresh。
- 若 webhook 处理失败且 delivery 占位记录被删除，则不发送“占位插入”事件，避免页面闪现不可见脏数据。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `GET /api/github-packages/webhook/deliveries/events` | HTTP API | internal | New | ./contracts/http-apis.md | backend | web inbox | GHCR delivery SSE |
| `GitHubPackagesWebhookDeliveryEvent` | Event payload | internal | New | ./contracts/events.md | backend | web inbox | SSE data shape |
| `github_packages_delivery_events` | DB table | internal | New | ./contracts/db.md | backend | api SSE | monotonic cursor store |
| `newGitHubPackagesWebhookDeliveriesEventsSource` | TS SDK | internal | New | ./contracts/http-apis.md | web | `GhcrWebhookInboxPage` | EventSource helper |

### 契约文档（按 Kind 拆分）

- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/events.md](./contracts/events.md)
- [contracts/db.md](./contracts/db.md)

## 验收标准（Acceptance Criteria）

- Given 收件箱页已打开，When 新 GHCR webhook delivery 到达，Then 页面无需手动点击刷新即可更新列表与 summary。
- Given 同一 `deliveryId` 重复投递，When attempt 被增加，Then 当前页刷新后显示新的 `attemptCount`，且原有 `decision/reason/jobId` 不被错误清空。
- Given delivery 最终 outcome 被写回，When SSE 事件到达，Then 页面刷新后显示新的 `decision/reason/responseStatus/jobId/jobIds`。
- Given SSE 连接中断后浏览器重连，When 服务端按 `Last-Event-ID` / `afterId` 续传，Then 不漏掉断线期间的新 delivery 事件，也不触发无限 replay。
- Given 未提供用户头且匿名访问受限，When 请求 deliveries SSE，Then 返回 `401`。

## 实现前置条件（Definition of Ready / Preconditions）

- 目标/非目标与边界已锁定。
- 现有收件箱列表 API 保持兼容，不改分页/搜索契约。
- 新 SSE 与事件表命名已冻结，可直接实现与测试。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: `cargo test -p dockrev-api github_packages_webhook`
- Integration tests: GHCR delivery SSE push / replay / auth coverage
- E2E tests (if applicable): None

### UI / Storybook (if applicable)

- Stories to add/update: None
- Visual regression baseline changes (if any): None

### Quality checks

- Lint / typecheck / formatting: `bun run --cwd web lint`, `bun run --cwd web build`, `cargo fmt --check`, `cargo test -p dockrev-api github_packages_webhook`, `bun run docs:build`

## 文档更新（Docs to Update）

- `docs-site/docs/api-reference.md`: 补 deliveries SSE 路由、事件名、cursor 语义与鉴权说明
- `docs/specs/README.md`: 新增 spec 索引并随实现进度更新状态

## 计划资产（Plan assets）

- Directory: `docs/specs/e3f83-ghcr-webhook-inbox-sse/assets/`
- In-plan references: `![...](./assets/<file>.png)`
- PR visual evidence source: maintain `## Visual Evidence (PR)` in this spec when PR screenshots are needed.
- If an asset must be used in impl (runtime/test/official docs), list it in `资产晋升（Asset promotion）` and promote it to a stable project path during implementation.

## Visual Evidence (PR)

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 后端新增 delivery 事件日志表、DB helper 与 SSE 路由
- [x] M2: webhook 路径在新建 / duplicate attempt / outcome update 三种场景发出 delivery 事件
- [x] M3: 收件箱页面接入 SSE 并保持分页/筛选/搜索状态稳定
- [x] M4: API 文档与自动化测试覆盖新 SSE 契约

## 方案概述（Approach, high-level）

- 后端仿照现有 jobs/version-inference SSE 模式，使用数据库事件表作为 cursor 来源，但只在用户可见状态成型后写事件。
- 前端继续以列表接口作为展示真相源，SSE 仅作为刷新触发器，降低排序/摘要漂移风险。
- 文档与测试与实现同轮落地，避免 PR 阶段出现 spec drift。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若 webhook 短时间高频到达，整页 refresh 次数过多；通过 250ms 防抖降低抖动。
- 风险：事件表需要额外存储空间；通过保守字段集与只记录用户可见变更控制增长。
- 需要决策的问题：None
- 假设（需主人确认）：None

## 变更记录（Change log）

- 2026-03-09: 新建 follow-up spec，冻结 delivery SSE、前端 refresh 策略与验证口径。
- 2026-03-09: 完成后端 delivery SSE、收件箱前端自动刷新、API 文档与本地验证；等待 fast-track PR / review-loop 收敛。
- 2026-03-09: 完成 delivery 事件日志、收件箱 SSE 自动刷新、API 文档与自动化验证。

## 参考（References）

- `docs/specs/p7k2m-ghcr-webhook-inbox/SPEC.md`
- `docs/specs/g5m9c-ghcr-webhook-jobization/SPEC.md`
- `docs/specs/async-data-continuity/SPEC.md`
