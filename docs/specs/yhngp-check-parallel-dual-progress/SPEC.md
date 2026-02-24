# Dockrev：固定 5 并行检查 + 双层任务进度（#yhngp）

## 状态

- Status: 部分完成（4/5）
- Created: 2026-02-24
- Last: 2026-02-24

## 背景 / 问题陈述

- 当前 `check all` 在镜像数量较多时体感偏慢，且任务进度难以区分“已安排”与“已完成”。
- 现有进度模型仅有单层 `current/total/percent`，在并发调度场景下无法表达调度推进情况。
- 前端任务列表与详情页仅显示单层进度条，无法同时反映调度与执行。

## 目标 / 非目标

### Goals

- `check` 任务固定为 5 并行槽位，并强制 1 秒错峰启动。
- registry per-host 并发与 check 并发对齐为固定 5，确保有效并行能力一致。
- 扩展统一 `JobProgress`：新增 `plannedCurrent/plannedTotal/plannedPercent`，保持旧字段兼容。
- `/queue` 与 `/queue/:jobId` 对 running 任务展示“安排进度 + 完成进度”双层进度条。

### Non-goals

- 不调整候选版本判定、回滚语义、registry retry 算法。
- 不引入新的数据库表或 schema 迁移。
- 不新增独立任务类型或新页面。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/api/types.rs`
- `crates/dockrev-api/src/discovery.rs`
- `crates/dockrev-api/src/runtime_scan.rs`
- `crates/dockrev-api/src/config.rs`
- `web/src/api.ts`
- `web/src/pages/QueuePage.tsx`
- `web/src/pages/JobDetailPage.tsx`
- `web/src/App.css`
- `README.md`
- `.env.example`

### Out of scope

- `update` / `runtime_scan` 调度算法改造（仅进度模型对齐）。
- Deploy/Supervisor 功能改造。

## 接口与契约变更（Interfaces & Contracts）

### HTTP / SSE

- `GET /api/jobs`
- `GET /api/jobs/{id}`
- `GET /api/jobs/{id}/events`

`job_progress` 增量字段（camelCase，可选）：

- `plannedCurrent`
- `plannedTotal`
- `plannedPercent`

兼容口径：

- 既有 `current/total/percent` 继续表示“完成进度”。
- 若调用方未使用 `planned*`，行为保持不变。

## 功能与行为规格（Functional/Behavior Spec）

### Check 调度

- 并发槽固定为 `5`。
- 每次启动新 worker 前，必须满足与上次启动间隔 `>= 1s`。
- 若有已完成 worker，优先回收并更新“完成进度”，避免进度滞后。

### 进度模型

- `check`：
  - `planned*` 表示已安排（已启动）任务进度。
  - `current/total/percent` 表示实际完成进度。
- `update/runtime_scan/discovery`：
  - 默认 `planned* = completed*`，保证前端双层展示一致。

### 前端展示

- Queue 行内展示两层条：安排（上）+ 完成（下）。
- Job 详情页进度卡展示两层条与对应计数/百分比。
- 历史任务若缺失 `planned*`，前端回退为 `planned=completed`。

## 验收标准（Acceptance Criteria）

- Given 同 registry 下至少 6 个待检查服务，When 触发 `check all`，Then 在飞检查数不超过 5，且连续启动间隔约 1 秒。
- Given `check` 任务 running，When 读取 jobs API 或 SSE，Then `plannedCurrent >= current` 且两者单调递增。
- Given 任务处于 running，When 查看 `/queue` 与 `/queue/:jobId`，Then 均可见双层进度条。
- Given 历史任务无 `planned*`，When 前端渲染，Then 不报错且双层展示正常回退。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-api`
- `bun run --cwd web lint`
- `bun run --cwd web build`

### Quality checks

- 新增/更新测试覆盖：并发上限、错峰启动、progress 字段与 SSE 载荷。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增 spec 并冻结验收口径。
- [x] M2: check 固定并发 + 1 秒错峰调度落地。
- [x] M3: JobProgress 双层字段与 SSE/API 对齐。
- [x] M4: Queue/JobDetail 双层进度条完成并兼容历史数据。
- [ ] M5: 验证通过并快车道交付（PR + checks + review-loop）。

## 风险 / 假设

- 风险：固定 1 秒错峰会拉长小批量任务总时长。
- 风险：外部调用方若严格校验 `job_progress` 字段白名单，可能需同步更新。
- 假设：当前部署不依赖 `DOCKREV_CHECK_CONCURRENCY` 与 `DOCKREV_REGISTRY_PER_HOST_CONCURRENCY` 的动态调参。

## 变更记录（Change log）

- 2026-02-24: 创建规格并冻结范围与验收口径。
- 2026-02-24: 完成实现与本地验证（cargo test + web lint/build），进入快车道交付阶段。
