# Dockrev：检查更新并行提升到 7（registry per-host 维持 5）（#dvxvx）

## 状态

- Status: 已完成
- Created: 2026-03-04
- Last: 2026-03-04

## 背景 / 问题陈述

- 现有检查任务固定并发为 `5`，在服务数量较多时调度吞吐受限。
- 本次目标是提升 `check` 任务调度吞吐，同时保持既有 API、SSE 与进度模型不变。
- 由于 owner 明确要求“仅提升 check 并发”，需要接受 `check=7` 与 `registry per-host=5` 的非对齐状态，并在文档中清晰声明。

## 目标 / 非目标

### Goals

- 将 `FIXED_CHECK_PARALLELISM` 从 `5` 提升到 `7`。
- 保持 `CHECK_SPAWN_STAGGER=1s` 不变，继续错峰启动 worker。
- 保持 `FIXED_REGISTRY_PER_HOST_CONCURRENCY=5` 不变。
- 更新并发相关测试断言与文档口径，明确 `check=7`、`registry=5`。

### Non-goals

- 不修改 `JobProgress` 结构、API 路由、SSE 事件格式。
- 不调整 registry retry/backoff 策略。
- 不引入新的数据库 schema 或迁移。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/config.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `.env.example`
- `README.md`
- `docs-site/docs/en/config.md`
- `docs-site/docs/config.md`
- `docs-site/docs/zh/config.md`
- `docs/specs/README.md`
- `docs/specs/dvxvx-check-parallelism-7-registry-5/SPEC.md`

### Out of scope

- `crates/dockrev-api/src/registry.rs`
- `crates/dockrev-api/src/api/types.rs`
- `web/**`

## 接口与契约变更（Interfaces & Contracts）

- 对外接口无新增/删除字段：
  - `GET /api/jobs`
  - `GET /api/jobs/{id}`
  - `GET /api/jobs/{id}/events`
- 变更仅限调度行为：`check` 最大并发上限从 `5` 提升到 `7`。
- 保持兼容性：调用方无需修改请求参数或响应解析逻辑。

## 功能与行为规格（Functional/Behavior Spec）

### Check 调度

- 并发槽固定为 `7`。
- 每次启动新 worker 前，必须与上次启动保持 `>= 1s` 间隔。
- 当有已完成 worker 时，优先回收并刷新已完成进度。

### Registry 并发

- per-host 并发固定 `5`，作为独立固定上限。
- `check` worker 可能在 registry semaphore 前排队，属于预期行为。

### 文档口径

- 所有运行配置文档统一声明：
  - Check 并发固定 `7`
  - Worker 启动错峰固定 `1s`
  - Registry per-host 并发固定 `5`

## 验收标准（Acceptance Criteria）

- Given 至少 8 个待检查服务，When 触发 check，Then 观测到最大在飞检查数 `<= 7` 且 `>= 2`。
- Given check 任务运行中，When 采样相邻 worker 启动时间，Then 启动间隔约 `>= 900ms`。
- Given check 任务运行中，When 查询 jobs API 或 SSE，Then `plannedCurrent >= current` 且结束时两者相等。
- Given 查看配置文档，When 核对并发描述，Then 明确为 check=7、registry=5，且均为固定策略。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-api check_uses_fixed_parallelism_stagger_and_dual_progress -- --nocapture`
- `cargo test -p dockrev-api`

### Quality checks

- 并发测试断言与常量配置一致（`<= 7`）。
- 文档口径无“check 与 registry 仍对齐为 5”的残留描述。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新建 spec，冻结目标、范围与验收口径。
- [x] M2: 后端 check 并发常量提升到 7，保持 1s 错峰和 registry=5。
- [x] M3: 更新并发测试断言与文档说明。
- [x] M4: 执行后端测试验证并收敛 review 反馈。
- [x] M5: 快车道交付（PR + checks + review-loop）。

## 风险 / 假设

- 风险：`check=7` 与 `registry=5` 不对齐时，部分 worker 会等待 registry 许可，收益受镜像分布影响。
- 假设：当前部署可接受该非对齐策略，优先提升调度吞吐。
- 假设：1 秒错峰策略继续适用于更高并发上限。

## 变更记录（Change log）

- 2026-03-04: 创建规格并冻结“check=7、registry=5”的执行口径。
- 2026-03-04: 完成后端并发常量、测试断言与文档口径更新，进入快车道验证与收敛流程。
