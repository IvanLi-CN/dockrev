# Dockrev：版本推测采集解耦 + 缓存门控 + 前端就绪等待（#kdapc）

## 状态

- Status: 已实现（本地已验证）
- Created: 2026-02-24
- Last: 2026-02-24

## 背景 / 问题陈述

- 现有 `check/runtime-scan` 主链路会同步进行版本推测（`resolvedTag/candidate.resolvedTag`），导致扫描耗时和 registry 压力升高。
- 版本推测是增强信息，不应阻塞“检查更新/执行升级”核心流程。
- 前端当前无法明确表达“版本推测数据尚未就绪”的状态，导致用户看到旧数据时缺乏时效保证。

## 目标 / 非目标

### Goals

- 将版本推测采集解耦为独立 worker，主链路仅负责 digest/candidate 判定。
- 引入镜像级缓存与门控：仅在 `cache miss / cache stale(>7d) / all_failed / new_version / force` 时触发采集。
- 新增服务级 force 刷新 API，并在版本气泡内提供按钮。
- 前端仅在“版本区”显示 pending/ready，不阻塞其他信息与升级按钮。
- 升级流程不依赖版本推测结果。

### Non-goals

- 不改变 candidate 选择语义（仍为同 tag digest-only 口径）。
- 不引入分布式 worker 协调（仅单实例 in-flight 去重）。
- 不新增可配置 TTL（固定 7 天）。

## 范围（Scope）

### In scope

- `crates/dockrev-api/src/version_inference_worker.rs`（新）
- `crates/dockrev-api/src/service_check.rs`
- `crates/dockrev-api/src/runtime_scan.rs`
- `crates/dockrev-api/src/api/mod.rs`
- `crates/dockrev-api/src/api/types.rs`
- `crates/dockrev-api/src/db.rs`
- `crates/dockrev-api/src/main.rs`
- `crates/dockrev-api/src/state.rs`
- `web/src/api.ts`
- `web/src/versionDisplay.ts`
- `web/src/components/CurrentVersionPopover.tsx`
- `web/src/components/VersionTagsPopover.tsx`
- `web/src/pages/OverviewPage.tsx`
- `web/src/pages/ServicesPage.tsx`
- `web/src/pages/ServiceDetailPage.tsx`

### Out of scope

- 旧 `docs/plan/**` 的迁移重写。
- 多实例一致性保证。
- 新管理页（worker 队列可视化）。

## 需求（Requirements）

### MUST

- 主链路不做版本推测网络采集。
- 版本推测缓存按 `image_repo + host_platform` 维护。
- 门控触发严格满足：
  - `无缓存` 或 `checked_at 超过 7 天` 或 `all_failed`。
  - 发现新版本（仅 candidate digest 变化）。
  - force。
- 当缓存存在但同镜像存在新的推测任务 in-flight，版本区视为未就绪（pending）。
- `POST /api/services/{service_id}/version-inference/refresh` 返回 `202`。
- 升级/检查流程在推测 worker 异常时仍成功执行。

### SHOULD

- pending 状态可给出 reason 与 `checkedAt`（若有）。
- 前端自动轮询直到 pending->ready，避免手工刷新。

### COULD

- 在 job/event 日志中记录版本推测 enqueue reason 便于排障。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 读取 stack/service 数据时，按服务所属 image 查询镜像级推测缓存并评估门控。
- 命中门控则 enqueue worker；若 in-flight 存在，返回 `versionInference.status=pending`。
- worker 完成后更新镜像级快照；后续读取返回 `status=ready` 并下发最新推测结果。
- force API 可主动触发该服务对应镜像的推测任务。

### Edge cases / errors

- 服务 image ref 非法：返回 `versionInference.status=ready` + `reason=not_required`，不触发 enqueue。
- registry 访问失败：缓存写入 `all_failed=true`，下次读取可再次触发（受 in-flight 去重）。
- 推测 worker 异常：不影响 check/update/runtime-scan 的 job status。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `services[].versionInference` | HTTP API | external | Modify | ./contracts/http-apis.md | backend | web | pending/ready 状态 |
| `POST /api/services/{service_id}/version-inference/refresh` | HTTP API | external | New | ./contracts/http-apis.md | backend | web/operator | force 采集入口 |
| `image_version_inference_snapshots` | DB | internal | New | ./contracts/db.md | backend | backend | 镜像级缓存 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/db.md](./contracts/db.md)

## 验收标准（Acceptance Criteria）

- Given 无缓存，When 首次打开页面，Then 版本区显示 pending，任务完成后转 ready。
- Given 缓存超过 7 天，When 读取服务列表，Then 版本区先 pending，再展示新推测结果。
- Given candidate digest 变化，When check 完成，Then 对应镜像版本推测任务被 enqueue（reason=`new_version`）。
- Given 用户在版本气泡点击强制刷新，When 请求返回，Then 状态为 pending 且按钮显示 loading。
- Given 推测 worker 持续失败，When 执行 check/update，Then 主流程成功且不依赖推测结果。
- Given 不满足触发条件，When 读取版本信息，Then 不触发采集，仅读缓存。

## 实现前置条件（Definition of Ready / Preconditions）

- 触发条件与 pending 语义已冻结。
- 对外 API 字段命名与兼容性策略已冻结。
- 前端等待范围锁定为“版本区”。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Rust 单测覆盖：门控触发、in-flight pending、force API、主链路解耦。
- 前端构建/类型/lint通过。

### UI / Storybook (if applicable)

- 更新版本气泡故事，覆盖 pending/ready 与 force loading。

### Quality checks

- `cargo test -p dockrev-api`
- `bun run --cwd web lint`
- `bun run --cwd web build`

## 文档更新（Docs to Update）

- `docs/specs/README.md`
- `docs/specs/kdapc-version-inference-decouple/SPEC.md`

## 计划资产（Plan assets）

- None

## 资产晋升（Asset promotion）

- None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新增版本推测 worker 与镜像级缓存表。
- [x] M2: check/runtime-scan 主链路移除版本推测实时采集并接入门控 enqueue。
- [x] M3: 新增 force API 与 `services[].versionInference` 字段。
- [x] M4: 前端版本区 pending/ready 展示 + 气泡强制刷新按钮 + 自动轮询。
- [x] M5: 测试验证通过，快车道推进到 PR + checks + review-loop 收敛。

## 方案概述（Approach, high-level）

- 复用现有 digest tags 扫描算法，把“触发时机”与“读取语义”独立抽象。
- 读取接口负责给出状态（ready/pending），worker 负责更新内容，主链路只更新核心升级数据。
- 通过 image 粒度去重降低同镜像多服务重复采集成本。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：pending 状态下轮询频率过高导致请求放大。
- 风险：单实例 in-flight 在多副本部署下不具备全局唯一性。
- 假设：当前部署为单实例。

## 变更记录（Change log）

- 2026-02-24: 创建规格并冻结本期范围与验收口径。
- 2026-02-24: 完成实现与本地验证，进入快车道交付阶段。

## 参考（References）

- `docs/plan/4cn9r:check-snapshot-decouple-worker/PLAN.md`
- `docs/plan/zdg25:auto-version-inference-ui/PLAN.md`
- `docs/specs/async-data-continuity/SPEC.md`
