# Dockrev：Cleanup apply 指纹稳定化与 stale 诊断补强（#hw3pb）

## 状态

- Status: 已完成
- Created: 2026-04-11
- Last: 2026-04-11
- Notes: fast-track（implementation + cleanup tests + local browser proof + stale diagnostics）

## 背景 / 问题陈述

- 线上 `/cleanup` 的“确认清理”会反复弹出 `候选已变化`，即使候选集合没有真实变化，也无法成功进入 `cleanup_apply` job。
- 根因是 cleanup confirm/apply 的 `confirmationFingerprint` 把 `scannedAt` 纳入哈希输入；`apply` 侧重算 plan 时会刷新时间戳，导致同一批候选也恒定失配。
- 现有 `409 cleanup_snapshot_stale` 只把最新 confirm payload 回给前端，没有在服务端日志里留下足够的诊断字段，线上排查成本高。

## 目标 / 非目标

### Goals

- 让 `POST /api/cleanups/apply` 在候选集合、归属、估算值与 unknown 语义未变化时成功创建 `cleanup_apply` job。
- 保留真实 stale 防护：只有候选 identity / ownership / estimate / unknown 语义变化时才返回 `409 cleanup_snapshot_stale`。
- 在 stale 分支补充服务端 tracing 诊断，记录 `principal`、`preset`、`scope`、`stackId/serviceId`、提交/最新指纹、候选数与估算摘要。
- 以新的 follow-up spec 承载本次修复，不回写既有 cleanup 主 spec 作为唯一实施记录。

### Non-goals

- 不新增 cleanup API 字段，不改变 `409 cleanup_snapshot_stale` 响应形状。
- 不改 cleanup 页面 copy / 视觉结构，不做额外 UX 重构。
- 不执行线上热修或直接部署；本次终点是 merge-ready。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/cleanup.rs`：稳定化 `confirmationFingerprint` 语义，剥离易变扫描时间戳。
- `crates/dockrev-api/src/api/cleanup_routes.rs`：在 stale 分支补 tracing 诊断字段。
- `crates/dockrev-api/src/cleanup/tests.rs` 与 `crates/dockrev-api/src/api/tests/suite_18.rs`：补“纯时间变化不再 stale”“未变化时 apply 成功建 job”“真实变化仍返回 409”回归。
- `docs/specs/qynjg-docker-prune-cleanup-console/contracts/http-apis.md`：记录 fingerprint 稳定语义。

### Out of scope

- `web/src/pages/CleanupPage.tsx` 的交互/文案/布局改动。
- cleanup job summary schema 变更。
- 线上环境手工任务补偿。

## 需求（Requirements）

### MUST

- `confirmationFingerprint` 必须忽略 `scannedAt` 这类与清理候选无关的易变扫描元数据。
- fingerprint 仍必须覆盖：`preset`、`scope`、`stackId/serviceId`、候选 identity/ownership/category、候选估算值、`estimateUnknown`、聚合 `estimatedReclaimableBytes` 与 `hasUnknownSize`。
- stale 返回前，服务端日志必须输出本次 apply 的关键诊断字段，但不得泄露额外敏感环境信息。
- cleanup 定向 Rust 测试必须覆盖“未变化 -> 成功建 job”和“真实变化 -> 409 stale”。

### SHOULD

- 本地浏览器 proof 直接证明 `/cleanup` 顶部 `全部 -> 确认清理` 会跳到 job 详情，而不是停留在 stale 循环弹窗。
- stale 诊断日志与 `cleanup_snapshot_stale` 响应里的最新 fingerprint 能在同一环境中对上。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- confirm-scan 继续返回最新 `scannedAt`，供前端确认弹窗展示“最新扫描时间”。
- apply 侧继续做二次 confirm-scan，但 fingerprint 重算只比较“清理语义稳定快照”，不把时间戳当作 stale 条件。
- 当本轮 apply 发现真实 stale 时，除了返回 `409 cleanup_snapshot_stale` 与 `latest` payload，还要在 tracing 中留下 principal / scope / fingerprint / target_count / estimate 摘要。

### Edge cases / errors

- 若只有 `scannedAt` 变化，apply 不得返回 stale。
- 若本轮 confirm-scan 新增/删除候选、候选归属变化、估算值变化，或 `hasUnknownSize/estimateUnknown` 改变，apply 仍必须返回 stale。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `POST /api/cleanups/apply` | HTTP API | external | Modify | `../qynjg-docker-prune-cleanup-console/contracts/http-apis.md` | dockrev-api | web | fingerprint 语义稳定化，stale 响应 shape 不变 |
| `Cleanup confirmationFingerprint` | HTTP API | internal | Modify | `../qynjg-docker-prune-cleanup-console/contracts/http-apis.md` | dockrev-api | web | 剥离 `scannedAt`，保留候选/估算语义 |

## 验收标准（Acceptance Criteria）

- Given 同一 scope/preset 的 confirm-scan 只变更 `scannedAt`，When 立刻提交 apply，Then 返回 `200` 且落库 `cleanup_apply` job。
- Given confirm 与 apply 之间真实新增/删除当前 scope 的候选，When 提交 apply，Then 返回 `409 cleanup_snapshot_stale` 与最新 confirm payload。
- Given 以伪造 fingerprint 提交 apply，When 命中 stale 分支，Then 服务端日志包含 principal、preset、scope、stack/service 维度、提交/最新指纹、target_count 与估算摘要。
- Given 本地 `/cleanup` proof 环境，When 点击 `全部 -> 确认清理`，Then 页面跳转到对应 job 详情页，而不是循环回 stale 弹窗。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `cargo test -p dockrev-api cleanup -- --nocapture`

### Quality checks

- `cargo fmt --all`
- `cargo test -p dockrev-api cleanup -- --nocapture`

## 文档更新（Docs to Update）

- `/Users/ivan/.codex/worktrees/ceaf/dockrev/docs/specs/README.md`
- `/Users/ivan/.codex/worktrees/ceaf/dockrev/docs/specs/qynjg-docker-prune-cleanup-console/contracts/http-apis.md`

## Visual Evidence

- source_type: `local_browser_proof`
  target_program: `Dockrev embedded UI`
  capture_scope: `browser-viewport`
  sensitive_exclusion: `isolated local proof DB + fake docker runner`
  submission_gate: `chat-only`
  state: `cleanup confirm navigates to cleanup_apply job detail`
  evidence_note: 在隔离 proof 环境中，`/cleanup` 顶部 `全部` 进入确认弹窗后点击 `确认清理`，页面直接跳转到 `cleanup_apply` job 详情页，日志可见 `cleanup started` 与删除记录；随后用伪造 fingerprint 再次请求 apply，后端 tracing 输出 principal/preset/scope/submitted_fingerprint/latest_fingerprint/target_count/estimated_reclaimable_bytes/has_unknown_size 诊断字段。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 稳定化 cleanup fingerprint 语义，移除 `scannedAt` 对 stale 判定的误伤
- [x] M2: 增加 cleanup stale tracing 诊断字段
- [x] M3: 补齐 cleanup 定向测试与本地浏览器 proof

## 方案概述（Approach, high-level）

- 保持现有 cleanup scan/apply schema，不改前端 stale 处理分支，只修 fingerprint 输入语义。
- 让 confirm-scan 继续承担“给人看最新扫描时间”的职责，而 fingerprint 只表达“本次确认究竟同意删除哪些东西”。
- 用一个隔离的本地 proof 环境（fake docker runner + seeded SQLite）跑真浏览器路径，证明修复确实落到了用户可见流程上。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：cleanup stale 仍依赖 confirm-scan 的候选投影语义；后续若新增会影响删除目标的字段，必须同步纳入 fingerprint。
- 风险：本次 tracing 诊断依赖当前文本 subscriber 格式；若未来统一改结构化日志，需要保留这些字段名。
- 需要决策的问题：None
- 假设（需主人确认）：cleanup stale 的判定仍以“删除语义是否变化”为主，而不是“扫描时间是否变化”。

## 变更记录（Change log）

- 2026-04-11：创建 follow-up spec，冻结 cleanup apply 指纹稳定化与 stale 诊断补强范围。
- 2026-04-11：完成后端修复与回归测试，确认 `scannedAt` 不再触发伪 stale，真实候选变化仍返回 `409 cleanup_snapshot_stale`。
- 2026-04-11：完成本地浏览器 proof 与 stale 诊断日志验证。

## 参考（References）

- `/Users/ivan/.codex/worktrees/ceaf/dockrev/docs/specs/qynjg-docker-prune-cleanup-console/SPEC.md`
- `/Users/ivan/.codex/worktrees/ceaf/dockrev/crates/dockrev-api/src/cleanup.rs`
- `/Users/ivan/.codex/worktrees/ceaf/dockrev/crates/dockrev-api/src/api/cleanup_routes.rs`
- `/Users/ivan/.codex/worktrees/ceaf/dockrev/crates/dockrev-api/src/api/tests/suite_18.rs`
