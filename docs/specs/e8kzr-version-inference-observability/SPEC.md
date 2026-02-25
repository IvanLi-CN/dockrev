# Dockrev：版本推测可观测性与缓存状态补齐（#e8kzr）

## 状态

- Status: 已完成（本地已验证）
- Created: 2026-02-25
- Last: 2026-02-25

## 背景 / 问题陈述

- 现有版本推测已经解耦为异步 worker，但缺少统一总览，无法在 UI 中追踪“排队中/执行中/缓存可用/缓存异常”的完整链路。
- 现有队列页只关注 jobs，不包含版本推测缓存与 worker 状态，用户无法快速判断“哪些镜像已缓存、哪些即将缓存、哪些需要补拉”。
- 版本推测缓存会持续累积，缺少长期回收机制。

## 目标 / 非目标

### Goals

- 新增版本推测总览 API：统一返回 worker、GC、summary、tasks、rows + 分页信息。
- worker 提供镜像级实时进度快照（phase + message + current/total + percent）。
- 新增 30 天缓存回收（启动一次 + 每 24h）。
- 前端新增独立页面 `/queue/version-inference`；队列页新增状态卡片并可跳转。

### Non-goals

- 不把版本推测并入现有 jobs 队列。
- 不新增“手动清缓存/批量重跑”运维入口。
- 不改变 7 天推测 TTL 判定语义（仅新增 30 天历史回收）。
- 不新增缓存管理写操作（手动清理/批量重跑）。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/version_inference_worker.rs`
- `crates/dockrev-api/src/db.rs`
- `crates/dockrev-api/src/api/types.rs`
- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/main.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `web/src/api.ts`
- `web/src/routes.ts`
- `web/src/App.tsx`
- `web/src/pages/VersionInferencePage.tsx`
- `web/src/pages/QueuePage.tsx`
- `web/src/stories/mocks/dockrevMockApi.ts`

### Out of scope

- 侧边栏新增一级导航入口。
- 手动触发 GC。
- 多实例共享 ring buffer。

## 接口契约（Interfaces & Contracts）

### HTTP

- `GET /api/version-inference/overview`
  - Query：`q`、`status`、`page`、`perPage`
  - `status` 仅支持：`all | queued | running | ready | stale | all_failed`
  - 返回：`worker`、`gc`、`summary`、`tasks`、`rows`、`page`、`perPage`、`total`

## 行为规格（Functional / Behavior Spec）

- 总览 rows 只展示“已缓存 + 即将缓存（queued/running）”，不含缺失项。
- `rows[].status` 枚举：`queued | running | ready | stale | all_failed`。
- 状态判定优先级：`running > queued > all_failed > stale > ready`。
- `summary` 口径：
  - `snapshotsTotal`：缓存快照总量（仅已落库快照）。
  - `queued/running/ready/stale/allFailed`：按 rows 状态聚合。
- GC 回收删除 `checkedAt` 超过 30 天的缓存项（不区分是否仍被服务引用）；记录最后执行结果和错误。

## 验收标准（Acceptance Criteria）

- Overview：`queued/running/ready/stale/all_failed` 判定正确，过滤与分页正确。
- Overview：列表口径为“缓存 + in-flight”，且不出现 `missing`。
- GC：启动后首次执行 + 每 24h 周期执行；`checkedAt > 30 天` 数据被删除；错误写入 `gc.lastError`。
- 前端：队列页状态卡片可跳转；独立页在 `queued/running` 时高频刷新、空闲时低频刷新。
- 回归：`GET /api/stacks/{id}` 的 `versionInference pending/ready` 行为不回归。

## 非功能性验收 / 质量门槛（Quality Gates）

- `cargo test -p dockrev-api`
- `bun run --cwd web lint`
- `bun run --cwd web build`

## 里程碑（Milestones / checklist）

- [x] M1: API 契约与状态口径冻结（overview）。
- [x] M2: worker 任务状态机 + 镜像级 progress 快照打通。
- [x] M3: 30 天 GC（启动 + 24h）与可观测字段打通。
- [x] M4: 前端独立页面 `/queue/version-inference` + 队列入口卡片。
- [x] M5: 回归测试、Storybook 与文档收口。

## 风险 / 假设

- 假设：部署拓扑为单实例，in-memory 聚合可满足当前规模。
- 风险：列表规模随缓存增长放大，依赖 30 天 GC 控制上限。

## 变更记录

- 2026-02-25: 口径重置为“overview + 队列入口 + 缓存列表 + 30 天 GC”，移除 missing/SSE 作为验收前置。
- 2026-02-25: 完成后端 overview 契约收敛（去 missing 口径、summary=snapshotsTotal+状态计数）、worker/DB GC 能力与前端路由迁移（`/queue/version-inference`）、页面轮询策略、队列卡片与 Storybook 场景补齐；通过 `cargo test -p dockrev-api`、`bun run --cwd web lint`、`bun run --cwd web build`。
