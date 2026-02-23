# Dockrev: check 提速（无更新跳过 snapshot）+ digest tags 异步去重采集（#4cn9r）

## 状态

- Status: 已完成
- Created: 2026-02-23
- Last: 2026-02-23

## 背景 / 问题陈述

- 当前 check 主链路会在每个 service 检查后持久化 digest-tags snapshot，即使该 service 没有可更新候选也会执行这部分采集。
- 这会把“候选判定”与“当前版本 tags 展示数据”耦合在同一路径上，增加 check 耗时并放大 registry 请求压力。
- 目标是把 snapshot 采集改为独立异步任务，同时保留 UI 在 snapshot 缺失时的可恢复体验。

## 目标 / 非目标

### Goals

- check 主链路不再执行 digest-tags snapshot 采集（无更新路径优先提速）。
- snapshot 采集拆分为独立任务，并按 `image_repo + digest + host_platform` 做全局去重。
- 程序启动后对当前已知 digest 触发一次预热采集；后续仅在 digest 变化时触发增量采集。
- snapshot API 支持 pending 响应，前端 popover 在 pending 状态下自动轮询直到 ready。

### Non-goals

- 不改变 candidate 选择与更新执行语义。
- 不引入跨实例分布式任务系统（本计划仅覆盖单进程去重 + DB 幂等）。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/service_check.rs`
  - check 链路移除 snapshot 持久化调用。
- `crates/dockrev-api/src/db.rs`
  - 新增全局 snapshot 存储（按 image_repo+digest+host_platform）。
- `crates/dockrev-api/src/api/mod.rs`
  - `digest-tags-snapshot` 返回 `ready/pending` 两态，并在缺失时 enqueue。
- `crates/dockrev-api/src/runtime_scan.rs` / update 路径 / startup
  - 在 digest 变化与启动预热时触发采集任务。
- `web/src/components/*Version*Popover.tsx` 与 `web/src/api.ts`
  - pending 展示 + 轮询。
- 对应 API/单元测试与前端测试更新。

### Out of scope

- 旧 `service_digest_tags_snapshots` 历史数据迁移工具链。
- 新增管理页配置项（例如 worker 队列可视化）。

## 验收标准（Acceptance Criteria）

- Given 触发 `check all` 且 service 无 digest 更新，When check 运行，Then 不再执行 snapshot 扫描逻辑。
- Given 两个 service 指向同一 `image_repo + digest + host_platform`，When 同时触发采集，Then 实际只执行一次采集任务。
- Given 程序启动，When 存在 `current_digest` 服务记录，Then 会触发一次预热采集。
- Given update/runtime-scan 导致 digest 变化，When 任务完成，Then 会自动触发新 digest 的 snapshot 采集。
- Given popover 请求 snapshot 且数据暂缺，When API 返回 pending，Then 前端显示“采集中”并轮询，直至 ready。

## 测试（Testing）

- `cargo test -p dockrev-api`
- `bun run --cwd web lint`
- `bun run --cwd web build`
- 新增/更新测试覆盖：
  - check 不再触发 snapshot 持久化。
  - snapshot pending -> ready。
  - 全局去重键生效（多 service 同 key 单次采集）。
  - 启动预热与 digest 变化触发采集。

## 风险与缓解（Risks）

- 风险：pending 轮询造成瞬时请求增多。
  - 缓解：返回 `retryAfterMs`，前端遵循间隔轮询并在 popover 关闭时停止。
- 风险：单实例去重无法完全覆盖多实例并发。
  - 缓解：DB 主键幂等 + 任务重复触发可接受（以最终一致为准）。

## 里程碑（Milestones）

- [x] M1: check 链路与 snapshot 采集解耦。
- [x] M2: 全局 snapshot 存储 + 去重 worker。
- [x] M3: startup 预热 + digest 变化触发。
- [x] M4: API pending + 前端轮询交互。
- [x] M5: 自动化验证通过并提交 PR。

## 变更记录（Change log）

- 2026-02-23: 创建计划并冻结范围、验收与测试口径（Status=待实现）。
- 2026-02-23: 完成实现与验证，进入快车道 PR 提交流程（Status=已完成）。
