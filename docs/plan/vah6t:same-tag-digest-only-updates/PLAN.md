# Dockrev: 仅“同 tag 的新 digest”更新（移除版本候选/选择 + 更新锁定扫描结果）

## 状态

- Status: 已完成
- Created: 2026-02-20
- Last: 2026-02-20

## Change log

- 2026-02-20: 实现“同 tag 的新 digest”更新口径；移除 `/candidates`；更新请求锁定扫描 digest（409）；Web 去掉版本选择；PR #74

## 背景 / 问题陈述

当前 Dockrev 会在扫描时基于 registry tags 选择“更高版本”的候选 tag（candidate），这会导致：

- 对于带前缀数字的 tag（例如 `8-alpine`），可能被错误升级到更高主版本（例如 `9.0.2`），出现“跨标签版本”提示。
- 更新确认弹窗存在“目标版本选择”能力，且其数据源与列表不一致，会带来误更新风险。

本计划冻结一个更明确的产品口径：**Dockrev 仅支持“同 tag 的 digest 更新”**，不支持跨 tag 变更。

## 目标（必须满足）

1. **扫描结果定义**：candidate 仅表示“当前配置 tag（raw tag）对应的最新 registry digest”与“当前 runtime digest”不同 ⇒ **同 tag 的新 digest**。
2. **UI 行为**：移除版本选择控件；确认弹窗数据与列表完全一致（只展示并使用 `svc.candidate`）。
3. **一致性保证**：执行更新时必须锁定扫描时确定的 digest；不一致则返回 **409 Conflict**，要求用户重新扫描/刷新。
4. **删除候选接口**：移除 `GET /api/services/:id/candidates`，前端不再调用。
5. **测试**：除本地测试外，必须在 `codex-testbox` 上用 Docker/Compose 搭环境做集成验证（按 shared host 规则隔离与清理）。

## 非目标（明确不做）

- 不支持跨 tag 更新（例如 `8-alpine -> 9.0.2` 或 `15-alpine -> 15.6-alpine`）。
- 不支持“docker/compose pull 发现不到的版本”的兜底/猜测；若 registry 查不到当前 tag 的 manifest，则不产生 candidate。
- 不新增/改动 UI 结构与交互（除去掉版本选择控件、去掉跨标签相关展示）。

## 口径与行为（冻结）

- “新版本”定义：同一个 raw tag 下，registry 返回的 digest 与当前运行态 digest 不一致（digest-only）。
- 更新的目标：始终以 `repo@sha256:<digest>` pin 的方式执行更新，digest 来自最近一次扫描持久化的 `svc.candidate.digest`。
- 不允许通过 API/UI 指定一个不同 tag 来更新（即使同主版本也不允许）。

## 对外接口变更

### API

- **移除**：`GET /api/services/{service_id}/candidates`
- **收紧**：`POST /api/updates`（`scope=service`）
  - `targetTag`：若提供则必须等于该服务 `image.tag`，否则 400。
  - `targetDigest`：前端必须提供；服务端必须校验其与 DB 中 `svc.candidate.digest` 一致，否则 409（带 details：`serviceId/expectedDigest/gotDigest`）。

### Web

- 删除 candidates 相关类型与请求封装（`ServiceCandidateOption`、`listServiceCandidates` 等）。
- 服务级别更新请求统一携带 `targetDigest = svc.candidate.digest`（来自列表数据）。

## 验收标准（Given / When / Then）

1. Given 服务配置 `valkey/valkey:8-alpine` 且 registry 存在更高 semver tags（例如 `9.0.2`），When 扫描完成，Then：
   - `svc.candidate` 要么不存在（无更新），要么 `svc.candidate.tag == "8-alpine"`（绝不跨 tag）。
2. Given 列表显示 candidate digest 为 `D_scan`，When 用户确认更新并发出请求，Then：
   - 请求携带 `targetDigest == D_scan`；
   - 服务端若当前 candidate digest != `D_scan` 则返回 409 conflict，并提示刷新/重扫。
3. When 执行更新，Then 实际更新使用 `repo@sha256:<digest>` pin（与扫描 digest 一致），不允许“偷偷升级到别的 tag”。
4. `GET /api/services/:id/candidates` 不再存在（404），前端不再发起该请求。
5. 在 `codex-testbox` 的 Docker/Compose 集成验证通过，并留下可复现命令序列（写入 PR Test Plan）。

## 测试计划（必须做）

### 本地

- Rust：`cargo test -p dockrev-api`
- Web：`cd web && bun run build`（至少 typecheck + build）

### codex-testbox 集成验证（必须）

按 `shared-testbox-runner` 的 RUN_ID/COMPOSE_PROJECT 规范执行，并满足：

- Dockrev 自身启动后能 discovery/scan 注册 fixture compose；
- fixture 服务为 `valkey/valkey:8-alpine`，扫描后不产生跨 tag candidate；
- 错误 `targetDigest` 返回 409；正确 digest 返回 200 jobId；
- 清理仅删除本次 run 的 compose 项目资源与 run 目录（禁止全局 prune）。

## 里程碑（Milestones）

1. 后端：扫描逻辑改为同 tag digest-only candidate；更新接口加 409 digest 锁定；移除 `/candidates` endpoint；补齐/更新单测。（已完成）
2. 前端：移除 `UpdateTargetSelect` 与 `/candidates` 调用；去掉跨标签展示；更新请求必带 digest；冲突错误提示。（已完成）
3. 集成验证：在 `codex-testbox` 上完成按规则隔离的 Docker/Compose 验证并记录 Test Plan。（已完成）

## 风险与回滚

- `/candidates` 为破坏性 API 变更：通过同仓库 Web 同步移除调用来消解风险；如有外部客户端需另行评估。
- runtime digest 缺失时无法比较：默认不产生 candidate（宁可不报，也不误报）。
