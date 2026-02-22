# Dockrev API: check 429 限流与中档提速（#gnae4）

## 状态

- Status: 已完成
- Created: 2026-02-22
- Last: 2026-02-22
- Notes: PR #82

## 背景 / 问题陈述

- 线上 `check all` 任务已确认可完成，但耗时偏长（示例任务约 6 分 36 秒），体感明显偏慢。
- 任务日志中存在多条 `registry request failed: 429 Too Many Requests`，导致 registry 查询长尾与重复失败。
- 当前后端已有 `check` 有界并发，但缺少“按 registry 主机限流 + 429 重试 + job 级缓存”的组合策略。

## 目标 / 非目标

### Goals

- 在不改变业务语义的前提下，降低 `429` 对 `check all` 的影响。
- 为 registry 请求增加可控重试与退避策略，避免瞬时限流直接放大为长尾失败。
- 增加按 registry 主机的并发闸门与 job 级缓存，减少重复请求与热点冲击。
- 保持任务可观测性：进度持续推进，最终可达明确终态（`success/failed`）。

### Non-goals

- 不引入新的队列/worker 架构。
- 不改现有 API 返回结构与前端页面交互。
- 不做激进自适应并发算法（本计划采用中档稳妥策略）。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/registry.rs`
  - `429` 感知重试（优先 `Retry-After`，否则指数退避 + 抖动）。
- `crates/dockrev-api/src/config.rs`
  - 新增 check 并发/registry 限流/重试相关配置项与解析。
- `crates/dockrev-api/src/state.rs` / `crates/dockrev-api/src/api/mod.rs` / `crates/dockrev-api/src/service_check.rs`
  - 按 registry 主机的请求并发限制。
  - check job 级缓存（`image -> tags`、`image+tag+platform -> manifest`）。
- 回归测试与配置测试。

### Out of scope

- 新增管理端 UI 开关。
- 更新任务（`update`）与 runtime scan 的策略改造。

## 关键设计决策

1. `CHECK_CONCURRENCY` 从常量改为配置项，默认维持 `8`，保守兼容当前行为。
2. registry 请求新增 per-host semaphore，默认 `3`，避免同 host 瞬时并发冲高。
3. `429` 重试上限默认 `3` 次，遵循：
   - 有 `Retry-After`：按服务端建议等待（上限裁剪）。
   - 无 `Retry-After`：指数退避（base 250ms）+ 抖动（jitter）。
4. cache 生命周期仅限单次 check job，避免跨任务污染与陈旧数据。
5. 单服务失败仍按现有降级策略处理，不得阻塞整任务终态收尾。

## 验收标准（Acceptance Criteria）

1. Given 触发 `check all` 且部分 registry 返回 `429`，When job 运行，Then job 仍可在合理时间内完成并写入终态。
2. Given 同一镜像被多个服务引用，When check 执行，Then `list_tags`/`get_manifest` 总调用次数应减少（可通过 mock 计数断言）。
3. Given 高并发服务集合命中同一 registry host，When check 执行，Then 同时在飞请求数不超过 per-host 配置上限。
4. Given 无 `Retry-After` 的 `429` 响应，When retry 发生，Then 使用指数退避并在达到上限后结束该请求。
5. Given 任务运行中查看进度，When job 处于 `running`，Then `progress.current` 单调递增且最终到 100%（或失败终态）。

## 测试（Testing）

- `cargo test -p dockrev-api`
- 新增/更新以下测试覆盖：
  - `Retry-After` 解析与 backoff 逻辑。
  - per-host 并发闸门。
  - check job 缓存减少重复 registry 请求。
  - `429` 场景下 check 任务终态与收尾行为。

## 风险与缓解（Risks）

- 风险：限流参数过严导致总耗时上升。
  - 缓解：参数全部 env 可调，默认保守并在 PR 中给出调参建议。
- 风险：重试策略引入额外等待。
  - 缓解：重试次数与单次等待上限受配置约束，避免无界等待。
- 风险：缓存键设计不完整导致误命中。
  - 缓解：缓存键包含 registry/name/tag/host_platform。

## 里程碑（Milestones）

- [x] M1: 配置项与 `429` 重试机制落地（含单元测试）。
- [x] M2: per-host 并发闸门接入 registry 请求路径。
- [x] M3: check job 级 tags/manifest 缓存接入与回归测试。
- [x] M4: 最小验证通过并提交 PR（含 checks 结果明确）。

## 变更记录（Change log）

- 2026-02-22: 创建计划并冻结范围、验收与测试口径（Status=待实现）。
- 2026-02-22: 完成实现并提交 PR #82，`PR Label Gate`（run 283）与 `CI (PR)`（run 233）通过（Status=已完成）。
