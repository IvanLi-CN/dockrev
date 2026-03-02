# Dockrev：修复 Update Job 失败误报成功 + 幂等步骤重试（#69hb2）

## 状态

- Status: 已完成
- Created: 2026-03-01
- Last: 2026-03-01

## 背景 / 问题陈述

- 线上出现 update 任务主流程已有失败信号，但任务状态仍显示 `success` 的误导性结果。
- `update` 链路缺少统一的幂等步骤重试，网络抖动或短时 Docker API 失败会直接终止流程。
- Job summary 缺少结构化失败定位信息，排障需要反复翻阅日志。

## 目标 / 非目标

### Goals

- 将幂等关键步骤重试耗尽后的结果统一归类为 `failed`，不再归类为 `success`。
- 为幂等步骤补充指数退避重试（含轻量抖动）。
- 在 update 失败 summary 中输出结构化字段：`failureStep`、`retry`、`lastError`。

### Non-goals

- 不新增 Job 状态枚举（继续使用 `queued|running|success|failed|rolled_back`）。
- 不对 `docker compose up -d` 等非幂等变更动作引入自动重试。
- 不调整备份 fail-closed 既有语义。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/config.rs`
- `crates/dockrev-api/src/updater.rs`
- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `.env.example`
- `docs/specs/README.md`

### Out of scope

- `web/` 页面结构与状态类型扩展。
- 数据库 schema 变更。

## 需求（Requirements）

### MUST

- 幂等步骤失败达到最大重试次数后，update 任务必须进入 `failed`。
- 重试策略支持配置：
  - `DOCKREV_UPDATE_IDEMPOTENT_RETRY_MAX_ATTEMPTS`（默认 3）
  - `DOCKREV_UPDATE_IDEMPOTENT_RETRY_BASE_MS`（默认 300）
  - `DOCKREV_UPDATE_IDEMPOTENT_RETRY_MAX_MS`（默认 3000）
- update 失败 summary 必须包含 `failureStep`、`retry`、`lastError`（当失败来源可识别时）。

### SHOULD

- 重试逻辑尽量复用统一 helper，避免分散实现。
- 重试失败信息可直接用于 job log/summary 排障。

## 功能与行为规格（Functional/Behavior Spec）

### 纳入自动重试的步骤

- `docker compose pull <service>`
- `docker inspect` 读取类步骤（image id / healthcheck / health status）
- `docker tag <old> <target>`
- `docker pull <repo>:<semver>`

### 不纳入自动重试的步骤

- `docker compose up -d`（非幂等）
- 备份执行主体

### 失败归类

- 若失败来源是 `UpdateStepFailure`：summary 增加结构化字段并终态 `failed`。
- 若失败来源为其它未结构化错误：保留原 `error` 字段并终态 `failed`。

## 验收标准（Acceptance Criteria）

- Given 幂等关键步骤持续失败并耗尽重试，When update job 完成，Then job 状态为 `failed`。
- Given 幂等步骤前两次失败第三次成功，When update job 完成，Then job 可为 `success`。
- Given `docker compose up -d` 失败，When update job 执行，Then 不进行自动重试。
- Given 幂等步骤失败，When 查询 `/api/jobs/{id}`，Then `summary.stacks[*].update` 含 `failureStep/retry/lastError`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --locked --all-features`

## 风险 / 假设

- 风险：将 semver pull 失败提升为任务失败后，某些 registry 权限异常会更早暴露为阻断失败。
- 假设：该失败语义提升符合生产排障需求，优先保证状态真实性。

## 变更记录（Change log）

- 2026-03-01: 新建规格，冻结“幂等步骤重试耗尽 => failed”语义。
- 2026-03-01: 完成后端实现、测试补齐与验证闭环。
- 2026-03-02: `scope=service` 语义由 #m3tq9 收紧为“显式 targetTag + 禁止版本号反推 tag”；本规格中 semver pull 阻断语义继续适用于非 service scope。
