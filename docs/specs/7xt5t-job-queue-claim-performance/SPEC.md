# Dockrev：Jobs 队列领取索引与慢告警（#7xt5t）

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，关键演进原因见 `./HISTORY.md`。

## 背景 / 问题陈述

- 101 服务器上约 50,450 条 `jobs` 记录使按 `type + queued` 领取任务的查询退化为全表扫描和临时排序。
- GHCR 与 repo-link workers 在空闲时仍按固定间隔轮询，约每秒触发 18.8 次领取查询，放大了 SQLite 单连接的排队与 CPU 消耗。
- 该主题定义领取查询的索引契约和低噪声退化诊断，避免相同问题在后续 worker 或 schema 调整中回归。

## 目标 / 非目标

### Goals

- 让 `type = ? AND status = 'queued' ORDER BY created_at, id LIMIT 1` 使用精确复合索引。
- 保持现有 FIFO、任务状态转换和 worker 轮询语义不变。
- 对超过 25 ms 的成功领取操作提供按任务类型限频的结构化 WARN。

### Non-goals

- 不调整 GHCR/repo-link 的轮询周期、worker 数量或调度策略。
- 不引入事件唤醒、退避、通用数据库指标平台或新的 API。
- 不处理 `jobs` / `job_logs` 保留策略、资源样本容量或 registry 429。

## 范围（Scope）

### In scope

- `jobs(type, status, created_at, id)` 复合索引。
- 领取 SQL 的单一内部常量、查询计划回归测试与 FIFO 行为测试。
- `slow queued job claim` 限频 WARN。

### Out of scope

- HTTP、SSE、JSON、`JobType` 和 worker 公开契约。
- 101 的部署、发布或持久化数据清理。

## 需求（Requirements）

### MUST

- schema 必须声明 `idx_jobs_type_status_created_at_id(type, status, created_at, id)`，并保留现有 `jobs` 索引。
- 领取 SQL 必须按 `created_at ASC, id ASC` 保持 FIFO，并只匹配指定 `type` 的 `queued` 任务。
- 慢领取计时必须覆盖共享 SQLite 连接等待与事务执行；仅成功调用可发出 WARN。
- WARN 必须包含 `job_type`、`duration_ms`、`claimed`、`threshold_ms`，阈值为 25 ms，同一 `job_type` 60 秒内最多一条。
- 所有 `Db` clone 必须共享同一份限频状态。

### SHOULD

- 查询计划测试应明确拒绝 `SCAN jobs` 和 `USE TEMP B-TREE`。

### COULD

- 发布后用只读检查确认索引、查询计划和空闲 CPU 已恢复。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 应用启动时，既有 SQLite 数据库通过幂等 schema 初始化创建新索引。
- worker 调用领取接口时，数据库按索引找到最早的匹配 queued job，并在同一立即事务内改为 `running`。
- 领取成功且总耗时达到 25 ms 时，若该任务类型上次告警已超过 60 秒，则记录一次 WARN；否则静默。

### Edge cases / errors

- 领取返回空结果同样可被诊断为慢调用，但不改变返回语义。
- 领取错误继续沿用现有错误传播与 worker 日志，不重复发出慢调用 WARN。
- 限频器 mutex 中毒时恢复其中状态，诊断机制不得使任务领取失败。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `jobs(type,status,created_at,id)` | DB index | internal | New | None | dockrev-api | job workers | 覆盖领取筛选与排序 |
| `slow queued job claim` | tracing event | internal | New | None | dockrev-api | operators | 只在成功慢领取时限频输出 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given 已初始化的 SQLite schema，When 对领取 SELECT 执行 `EXPLAIN QUERY PLAN`，Then 使用 `idx_jobs_type_status_created_at_id`，且不出现全表扫描或临时排序。
- Given 不同类型、状态及相同创建时间的 queued jobs，When 领取指定类型，Then 只领取匹配 queued job，并按 `created_at ASC, id ASC` 原子更新为 `running`。
- Given 慢领取诊断，When 同类型在 60 秒内重复慢调用，Then 只允许第一次通过；不同类型和窗口到期后的调用可独立通过。

## 验收清单（Acceptance checklist）

- [x] 核心路径的长期行为已被明确描述。
- [x] 关键边界/错误场景已被覆盖。
- [x] 涉及的接口/契约已写清楚或明确为 `None`。
- [x] 相关验收条件已经可以用于实现与 review 对齐。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: 查询计划、FIFO/状态过滤和限频器边界。
- Integration tests: existing `Db::open` schema 初始化路径覆盖既有数据库索引创建。
- E2E tests (if applicable): None

### UI / Storybook (if applicable)

- Not applicable; no UI-affecting change.

### Quality checks

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --locked --all-features`
- `python3 ./.github/scripts/check-file-budgets.py`
- `git diff --check`

## Visual Evidence

PR: none

## Related PRs

- None

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：首次启动会构建一次索引；当前 jobs 表规模下预期为短暂启动期开销。
- 需要决策的问题：若发布后 CPU 仍不满足目标，另开 follow-up 评估事件唤醒或退避。
- 假设：SQLite `SCHEMA` 初始化会在服务开始处理请求前完成。

## 参考（References）

- [g5m9c GHCR Webhook Jobization](../g5m9c-ghcr-webhook-jobization/SPEC.md)
- [2m9ge repo URL Auto Backfill](../2m9ge-repo-url-auto-backfill/SPEC.md)
