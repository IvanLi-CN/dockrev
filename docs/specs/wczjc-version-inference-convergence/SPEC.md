# Dockrev：版本推测收敛（digest snapshot 单数据源，功能不减）（#wczjc）

## 状态

- Status: 已完成
- Created: 2026-02-26
- Last: 2026-02-26

## 背景 / 问题陈述

- 版本推测长期存在双链路：`image_digest_tags_snapshots` 与 `image_version_inference_snapshots` 并存，导致主列表、详情与版本气泡可能出现“latest / semver”不一致。
- `/queue/version-inference` 与 `/api/version-inference/*` 需要保留异步可观测能力，但其实现应收敛到 digest snapshot worker，避免数据分叉。
- 历史上下文参考：PR `#80/#83/#89/#93` 逐步引入了队列观测、缓存门控与前端展示能力，本次在不减功能前提下完成统一。

## 目标 / 非目标

### Goals

- 统一 `resolvedTag` 推断来源：主列表/详情/popover 优先基于 `image_digest_tags_snapshots`。
- 保留对外合同：`/queue/version-inference` 页面、`/api/version-inference/*` 路由和主要字段不变。
- 删除旧链路：`image_version_inference_snapshots` 表与 `version_inference_worker` 装配代码。
- 固化运行契约：
  - 并行执行数 `4`
  - 去重键 `image_repo + digest + host_platform`
  - TTL `7` 天
  - `all_failed` 冷却 `10` 分钟
  - pending 重试提示 `retryAfterMs=800`

### Non-goals

- 本次不引入跨实例分布式去重（默认单实例语义）。
- 不改变现有路由路径、SSE 事件名与前端页面入口。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/snapshot_worker.rs`
- `crates/dockrev-api/src/runtime_scan.rs`
- `crates/dockrev-api/src/db.rs`
- `crates/dockrev-api/src/main.rs`
- `crates/dockrev-api/src/state.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `web/src/pages/VersionInferencePage.tsx`（合同保持，无功能删减）
- `web/src/pages/QueuePage.tsx`（合同保持）

### Out of scope

- 新增跨集群任务协调与分布式锁。
- 新增路由或替换现有页面信息架构。

## 接口契约（Interfaces & Contracts）

### 对外保持不变

- `GET /api/version-inference/overview`
- `GET /api/version-inference/events`
- `POST /api/services/{service_id}/version-inference/refresh`
- 页面：`/queue/version-inference`

### 内部语义收敛

- `GET /api/stacks/{id}` 中 `services[].image.resolvedTag` 与 `services[].candidate.resolvedTag` 由 digest snapshot 推断优先。
- `/api/version-inference/overview` 改为 snapshot worker 运行态 + digest snapshot 聚合。
- `/api/version-inference/events` 改为 snapshot worker 事件 ring 发布，事件名保持 `version_inference_event`。
- `refresh` 语义改为触发 current/candidate digest 的 snapshot 异步刷新，返回结构保持 pending 兼容。

## 验收标准（Acceptance Criteria）

- `candidate.tag=latest` 且 candidate digest snapshot 含 `0.13.0` 时，主列显示 `0.13.0`（不再是 `latest`）。
- 同一 digest 下主列与气泡版本显示一致。
- cache miss 返回 pending 并触发 enqueue；完成后可转 ready。
- all_failed 在 10 分钟内不重复派发，超过 10 分钟可重试。
- `/queue/version-inference` 持续显示 queued/running/ready/stale/all_failed，SSE 增量刷新可用。
- 强制刷新入口行为保持不变，可观察异步状态流转。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-api`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- `bun run --cwd web test-storybook`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: `api/mod.rs` 使用 digest snapshot 统一 stack enrich 与 refresh 触发逻辑。
- [x] M2: `snapshot_worker` 提供固定并行、任务运行态、SSE ring、overview 聚合输入与 GC 状态。
- [x] M3: `/api/version-inference/overview` 与 `/events` 迁移到 snapshot runtime，保持外部合同。
- [x] M4: 删除旧表与旧 worker 装配（`image_version_inference_snapshots` / `version_inference_worker`）。
- [x] M5: 回归测试与前端质量门槛通过。

## 风险 / 假设（Risks, Assumptions）

- 假设当前为单实例部署，内存级 in-flight 去重满足线上需求。
- 风险：若部署为多实例，需补充分布式任务去重与事件一致性方案。

## 变更记录（Change log）

- 2026-02-26: 创建并冻结规格，明确并行/派发/缓存契约与兼容 API。
- 2026-02-26: 完成后端数据源收敛到 digest snapshot，移除旧链路，保持 queue/UI 功能不减。
- 2026-02-26: 通过 `dockrev-api` 全量测试及 web `lint/build/test-storybook` 质量门槛。
