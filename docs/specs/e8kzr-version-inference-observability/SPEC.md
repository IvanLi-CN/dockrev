# Dockrev：版本推测可观测性补齐（SSE + 任务总览）（#e8kzr）

## 状态

- Status: 已完成（本地已验证）
- Created: 2026-02-25
- Last: 2026-02-25

## 背景 / 问题陈述

- 现有版本推测已经解耦为异步 worker，但缺少统一总览与实时事件流，无法在 UI 中追踪“排队中/执行中/完成/失败缓存”的完整链路。
- 现有队列页只关注 jobs，不包含版本推测缓存与 worker 状态，用户无法快速判断“哪些镜像已缓存、哪些即将缓存、哪些需要补拉”。
- 版本推测缓存会持续累积，缺少长期回收机制。

## 目标 / 非目标

### Goals

- 新增版本推测总览 API：统一返回 worker、GC、summary、tasks、rows + 分页信息。
- 新增版本推测 SSE 事件流：支持 `Last-Event-Id` / `afterId` 断线续传。
- 事件采用统一 event name：`version_inference_event`，`data.type` 固定枚举。
- worker 提供镜像级进度事件（phase + current/total + percent）。
- 新增 30 天缓存回收（启动一次 + 每 24h）。
- 前端新增独立页面；队列页新增状态卡片并可跳转。

### Non-goals

- 不把版本推测并入现有 jobs 队列。
- 不新增“手动清缓存/批量重跑”运维入口。
- 不改变 7 天推测 TTL 判定语义（仅新增 30 天历史回收）。
- 不做跨实例事件持久化（保持单实例进程内语义）。

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
  - 返回：`worker`、`gc`、`summary`、`tasks`、`rows`、`page`、`perPage`、`total`

### SSE

- `GET /api/version-inference/events`
  - 支持 `Last-Event-Id` 与 `afterId`
  - 响应头：`Cache-Control: no-cache`、`x-accel-buffering: no`、keep-alive
  - event name：`version_inference_event`
  - `id`：单调递增整型
  - ring buffer：进程内固定容量（默认 2000）
  - 若 `afterId` 早于 ring buffer 可回放范围：发送 `resync_required`

### SSE data.type 枚举

- `task_enqueued`
- `task_started`
- `task_progress`
- `task_finished`
- `gc_ran`
- `resync_required`

## 行为规格（Functional / Behavior Spec）

- 总览 rows 展示“已缓存 + 即将缓存（queued/running）+ 缺失（missing）”。
- `rows[].status` 枚举：`missing | queued | running | ready | stale | all_failed`。
- `missing`：服务镜像存在、无缓存、且当前未入队。
- `task_progress` 必须包含镜像级 phase/current/total/percent，并在高频场景轻度限流（默认 250ms）且最终态必达。
- 客户端断线重连时优先按事件增量补发；补发失败时触发 `resync_required`，客户端立即重新拉取 overview。
- GC 回收删除 `checkedAt` 超过 30 天的缓存项；记录最后执行结果和错误。

## 验收标准（Acceptance Criteria）

- SSE 基础：可观察到 `task_enqueued -> task_started -> task_progress -> task_finished`，且 `id` 单调递增。
- SSE 断线：带 `Last-Event-Id`/`afterId` 可增量补发；过旧 offset 返回 `resync_required`。
- Overview：`missing/queued/running/ready/stale/all_failed` 判定正确，过滤与分页正确。
- GC：启动后首次执行 + 每 24h 周期执行；`checkedAt > 30 天` 数据被删除；错误写入 `gc.lastError`。
- 前端：队列页状态卡片实时更新并可跳转；独立页支持 SSE 与 fallback 轮询。
- 回归：`GET /api/stacks/{id}` 的 `versionInference pending/ready` 行为不回归。

## 非功能性验收 / 质量门槛（Quality Gates）

- `cargo test -p dockrev-api`
- `bun run --cwd web lint`
- `bun run --cwd web build`

## 里程碑（Milestones / checklist）

- [x] M1: API 契约与路由骨架冻结（overview + events）。
- [x] M2: worker 任务状态机 + SSE 事件 + ring buffer + progress 打通。
- [x] M3: 30 天 GC（启动 + 24h）与可观测字段打通。
- [x] M4: 前端独立页面 + 队列入口卡片 + SSE/轮询降级。
- [x] M5: 回归测试与文档收口。

## 风险 / 假设

- 假设：部署拓扑为单实例。
- 假设：事件 ring buffer 仅进程内，无重启持久化。
- 风险：progress 高频事件可能导致前后端压力上升（通过 250ms 节流缓解）。

## 变更记录

- 2026-02-25: 创建规格，冻结 SSE 契约、总览模型、GC 回收与验收边界。
- 2026-02-25: 完成后端 overview/events + worker 可观测 + 30 天 GC + 前端独立页/队列卡片 + 回归测试与本地验证。
