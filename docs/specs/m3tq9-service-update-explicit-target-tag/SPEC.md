# Dockrev：Service Update 禁止版本号反推 tag，改为显式 targetTag（#m3tq9）

## 状态

- Status: 已完成
- Created: 2026-03-02
- Last: 2026-03-02

## 背景 / 问题陈述

- `scope=service` 更新在执行后会从 `org.opencontainers.image.version` 反推 semver tag 并补拉 `repo:<semver>`。
- 当 registry 不存在该 tag 时，任务会因 `semver_pull` 失败而终态 `failed`，与“点击升级时已明确目标”的操作语义冲突。
- 该行为会把“额外推断步骤”失败混入 service update 主链路，造成误报和排障噪音。

## 目标 / 非目标

### Goals

- `scope=service` 更新必须在点击时显式锁定目标 tag + digest，不再做版本号反推。
- 收紧 API 契约：service update 强制要求 `targetTag` 与 `targetDigest` 同时提供。
- 维持现有 summary 结构兼容（保留 `semverPulled` / `semverPullWarnings` 字段）。

### Non-goals

- 不修改 `scope=stack/all` 的 semver pull 行为。
- 不修改 supervisor 自升级链路。
- 不新增配置开关。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/updater.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/pages/OverviewPage.tsx`
- `web/src/pages/ServicesPage.tsx`
- `docs-site/docs/api-reference.md`
- `docs-site/docs/en/api-reference.md`
- `docs-site/docs/zh/api-reference.md`
- `docs/specs/README.md`

### Out of scope

- `crates/dockrev-supervisor/**`
- `scope=stack/all` 任务语义调整

## 需求（Requirements）

### MUST

- `POST /api/updates` 在 `scope=service` 时，`targetTag` 必填（非空），否则返回 `400 invalid_argument`。
- `POST /api/updates` 在 `scope=service` 时，`targetDigest` 继续保持必填。
- `scope=service` 更新过程中不得执行 semver tag 反推与 `semver_pull`。
- Web 三个 service 更新入口必须始终上送 `targetTag + targetDigest`。

### SHOULD

- 保持已有 summary 字段兼容，避免外部消费方 JSON 解析回归。

## 功能与行为规格（Functional/Behavior Spec）

- service update：
  - 入口校验：`targetTag` + `targetDigest` 必填。
  - 执行链路：pull/up/health/inspect 完成后直接结束，不再调用 semver pull 分支。
- stack/all update：
  - 保持现状（本规格不变更）。

## 验收标准（Acceptance Criteria）

- Given `scope=service` 且缺失 `targetTag`，When 调用 `/api/updates`，Then 返回 `400 invalid_argument`。
- Given `scope=service` 且 `targetTag` 与服务当前 tag 不一致，When 调用 `/api/updates`，Then 返回 `400 invalid_argument`（cross-tag guard 保持）。
- Given `scope=service` 更新成功路径，When job 完成，Then 不会因 `failureStep=semver_pull` 失败。
- Given Web 端从 Overview/Services/ServiceDetail 触发 service update，When 发起请求，Then payload 包含 `targetTag + targetDigest`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo fmt --all`
- `cargo test -p dockrev-api`

## 风险 / 假设

- 假设：service update 由官方 Web UI 或受控客户端触发，可按新契约传入 `targetTag`。
- 风险：遗留外部客户端若未传 `targetTag`，将收到 `400`，需同步契约。

## 变更记录（Change log）

- 2026-03-02: 新建规格并冻结“service update 显式目标 + 禁止版本号反推 tag”语义。
- 2026-03-02: 完成后端/前端实现与回归测试。
