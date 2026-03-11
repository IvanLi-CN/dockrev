# Dockrev：撤销 #162，改为显式 tag 驱动的更新契约（#99egq）

## 状态

- Status: 已完成
- Created: 2026-03-11
- Last: 2026-03-11

## 背景 / 问题陈述

- `scope=stack/all` 更新仍会在主链路后基于 OCI label 推断补拉 tag；PR #162 仅修复了 `vX.Y.Z -> [vX.Y.Z, X.Y.Z]` 的一半场景。
- 当镜像 label、registry 实际发布 tag、以及调用方预期不一致时，更新已经成功切到新 digest，job 仍会因为服务端“猜 tag”失败而被误判为失败。
- 现有 aggregate update 与 webhook update 也没有显式声明每个服务的目标 tag / 兼容补拉 tags，导致服务端不得不继续推断。

## 目标 / 非目标

### Goals

- 用“调用方显式提供目标 tag + 兼容补拉 tags”替代 update 链路中的 tag 推断。
- 统一 `POST /api/updates` 与 webhook update 契约：所有 scope 都必须锁定 digest，且 tag 必须由发起方显式给出。
- 把 `targetTag` 设为阻断步骤，把 `pullTags[]` 设为 warn-only 兼容补拉步骤。
- 保持旧 summary 字段兼容：`semverPulled` / `semverPullWarnings` 继续存在，但固定为空值一版。

### Non-goals

- 不开放 cross-tag update；所有 `targetTag` 仍必须等于服务当前 `image.tag`。
- 不改 supervisor 自升级对外契约。
- 不新增 registry 端“先探测 tag 再决定是否 pull”的新接口。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/api/types/jobs.rs`
- `crates/dockrev-api/src/api/types/github_packages.rs`
- `crates/dockrev-api/src/api/operations.rs`
- `crates/dockrev-api/src/api/webhooks.rs`
- `crates/dockrev-api/src/updater.rs`
- `crates/dockrev-api/src/api/tests.rs`
- `web/src/api.ts`
- `web/src/pages/OverviewPage.tsx`
- `web/src/pages/ServicesPage.tsx`
- `web/src/pages/ServiceDetailPage.tsx`
- `web/src/stories/mocks/dockrevMockApi.ts`
- `docs-site/docs/api-reference.md`
- `docs-site/docs/en/api-reference.md`
- `docs-site/docs/zh/api-reference.md`
- `docs/specs/README.md`
- 本规格文档

### Out of scope

- `crates/dockrev-supervisor/**`
- GitHub Packages webhook audit / registry maintenance 功能语义
- 数据库 schema 变更

## 需求（Requirements）

### MUST

- `POST /api/updates` 在 `scope=service` 时必须显式提供 `serviceId + targetTag + targetDigest + pullTags`；`pullTags` 允许空数组，但字段必须存在。
- `POST /api/updates` 在 `scope=stack|all` 时必须显式提供 `targets[]`，元素固定为 `{ serviceId, targetTag, targetDigest, pullTags }`。
- `targets[]` 必须完整覆盖本次 scope 内实际会执行更新的服务，且不得重复、缺失、越界或漏项。
- webhook `action=update` 必须改用显式 `targets[]` 契约；旧 payload 若未提供 `targets[]` 必须直接拒绝。
- 所有 scope 都必须继续锁定到最新扫描 candidate 的 digest；若 `targetDigest` 不再匹配当前 candidate，返回 `409 conflict`。
- 所有 scope 都必须继续禁止 cross-tag update；若 `targetTag` 与服务当前 `image.tag` 不一致，返回 `400 invalid_argument`。
- update 执行链路必须改为：`pull digest -> up -> health -> pull targetTag (blocking) -> sync configured tag -> pull pullTags[] (warn-only) -> success`。
- `targetTag` 拉取失败必须阻断并走现有 rollback；`pullTags[]` 失败只能写 warning，不得把整次 job 判成失败。
- update 日志 / 命令序列中不得再出现 `org.opencontainers.image.version` inspect、`semver_pull`、或服务端根据 semver 推断出的 tag pull。

### SHOULD

- `pullTags[]` 执行应在单 job 内按 tag ref 去重，避免重复拉取与重复 warning。
- Web UI 在发起更新前应尽力补入 `candidate.resolvedTag` 与 digest snapshot tags；snapshot 缺失、pending 或 404 不应阻塞请求。

## 功能与行为规格（Functional/Behavior Spec）

### API contract

- `scope=service`
  - 请求体：`{ scope, serviceId, targetTag, targetDigest, pullTags, mode, allowArchMismatch, backupMode, reason }`
  - 缺少任一必填字段返回 `400 invalid_argument`。
- `scope=stack|all`
  - 请求体：`{ scope, stackId?, targets, mode, allowArchMismatch, backupMode, reason }`
  - `targets[]` 必须与服务端按现有 selection 规则得到的“actionable services”精确对齐。
- webhook `action=update`
  - 请求体：`{ action:"update", scope, stackId?, serviceId?, targets, allowArchMismatch, backupMode }`
  - 服务端不得再从 webhook payload 之外推断 tag。

### Update execution

- 每个服务的 override image 必须来自显式 `targetDigest`，不再回退到服务端读取到的 candidate / OCI version。
- `targetTag` pull 只验证并拉取调用方明确声明的 tag ref；成功后仍需执行现有 `sync configured tag`，保证本地 compose tag 指向锁定 digest。
- `pullTags[]` 仅拉取调用方提供的兼容 tag refs；失败写入 warning summary，不影响 job success。

### Summary compatibility

- 新增显式字段：
  - `targetTagsPulled: string[]`
  - `pullTagsPulled: string[]`
  - `pullTagWarnings: Array<{ serviceId, tagRef, step?, retry?, lastError?, error? }>`
- 遗留兼容字段：
  - `semverPulled: []`
  - `semverPullWarnings: {}`

## 验收标准（Acceptance Criteria）

- Given `scope=service` 缺少 `targetTag` / `targetDigest` / `pullTags` 任一字段，When 调用 `/api/updates`，Then 返回 `400 invalid_argument`。
- Given `scope=stack|all` 缺少 `targets[]`、目标重复、越界、或未覆盖所有 actionable services，When 调用 `/api/updates`，Then 返回 `400 invalid_argument`。
- Given `targetDigest` 与最新 candidate digest 不一致，When 发起任意 scope 的 update，Then 返回 `409 conflict`。
- Given 服务显式目标为 `targetTag=latest`、`pullTags=["v1.1.2"]`，When update 成功，Then job 日志中会出现真实的 `docker pull repo:latest`；若 `docker pull repo:v1.1.2` 失败，Then job 仍为 `success`，且 summary / logs 有 warning。
- Given `targetTag` 对应 tag 不存在，When update 执行到该步骤，Then job 必须失败并回滚，且不能再出现“服务已升级成功但因为服务端猜错兼容 tag 而 failed”的假失败。
- Given 检查 update job summary，When 读取兼容字段，Then `semverPulled == []` 且 `semverPullWarnings == {}`。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-api`
- `cd web && bun run build`

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新规格落地并替代 `#xyma9`，统一 `/api/updates` 与 webhook update 的显式 tag 契约。
- [x] M2: 后端请求校验与 updater 改造完成，删除 OCI version / semver pull 推断。
- [x] M3: Web UI、mock、API docs 与回归测试同步完成。

## 风险 / 假设

- 假设：调用方能从候选结果 / digest snapshot 组装 `pullTags[]`；服务端不再兜底补猜。
- 风险：仓库外若仍有旧 webhook / API 调用方，升级后会因缺少 `targets[]` 或 `pullTags` 收到 `400`。
- 风险：`targetTag` pull 成功后若 registry tag 漂移，仍需依赖后续 `sync configured tag` 把本地 compose tag 重新对齐到锁定 digest。

## 变更记录（Change log）

- 2026-03-11: 新建规格，冻结“撤销 #162、改为显式 tag 驱动 update 契约”的实施边界。
