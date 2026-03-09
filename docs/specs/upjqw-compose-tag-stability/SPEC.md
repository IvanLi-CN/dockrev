# Dockrev：更新后本地 Compose Tag 稳定化，消除手动 `docker compose up -d` 回退（#upjqw）

## 状态

- Status: 已完成
- Created: 2026-03-08
- Last: 2026-03-08

## 背景 / 问题陈述

- Dockrev 当前更新 service 或 supervisor 时，会通过 override compose 将目标镜像临时锁到 digest，再执行 `docker compose pull/up`。
- 更新成功后，运行态虽然已经切到新 digest，但 compose 原始 `image: repo:tag` 对应的本地 Docker tag 仍可能停留在旧镜像。
- 生产环境里若运维随后直接执行 `docker compose up -d`，Compose 会复用本地旧 tag，导致服务意外回退，与手动操作习惯冲突。

## 目标 / 非目标

### Goals

- 将“升级成功”的语义收紧为：新镜像已健康通过，且 compose 原始 tag 已同步到当前运行镜像。
- 覆盖 Dockrev 普通更新链路（`service/stack/all`）与 supervisor 自升级链路。
- 失败时不得报 success，需复用既有回滚闭环恢复运行态与本地 tag 一致性。

### Non-goals

- 不改写生产 compose 文件内容。
- 不保证手工 `docker compose pull` 或 `docker compose up --pull always` 继续停留在当前 digest。
- 不改变 `scope=service` 显式 `targetTag + targetDigest` 契约与非 service scope 的 semver pull 基本规则。

## 范围（Scope）

### In scope

- 对 tag-based compose 镜像引用，在更新成功收尾阶段执行本地 tag 对齐。
- 普通更新链路中为 tag 对齐失败定义独立失败阶段 `sync_configured_tag`。
- supervisor 自升级链路中增加可观察的 sync tag 进度与日志。
- 补充单元/流程测试，覆盖成功同步、同步失败回滚、skip 条件与顺序约束。

### Out of scope

- 对 digest-pinned `image: repo@sha256:...` 执行本地 tag 同步。
- 修改 Web/API 外部请求与响应 schema。
- 新增独立 UI 入口或用户配置项。

## 需求（Requirements）

### MUST

- 普通更新对每个实际切换到新镜像的 tag-based service，在 health 通过后、success 前执行 `docker image tag <new_image_id> <svc.image.reference>`。
- 若普通更新的 tag 同步失败，任务不得返回 `success`；必须先尝试恢复旧 tag 并执行既有 `up --pull never` 回滚。
- supervisor apply 成功路径在最终 `succeeded` 前，将 `target_image_repo:<request.tag>` 指向当前运行镜像；失败时复用 `rollback_on_failure`。
- digest-pinned 目标与 dry-run 均不得写入本地 tag。

### SHOULD

- 日志/状态要能明确显示当前处于 sync tag 阶段，便于定位失败点。
- 非 service scope 的 semver pull 仅在 tag 对齐成功后执行。

### COULD

- 在 summary/log 中记录跳过 tag 对齐的原因（例如 digest-pinned）。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 普通更新：`pull -> up -> health/inspect -> sync configured tag -> (非 service scope) semver pull -> success`。
- 普通更新回滚：若 health 或 sync configured tag 失败，先将旧镜像重新打回 compose 原始 tag，再执行 `up --pull never` 恢复旧运行态。
- supervisor apply：`pull -> compose up -> health -> postcheck -> sync request tag -> success`。

### Edge cases / errors

- 若 service 的 compose 原始镜像引用已经是 digest pin，则普通更新跳过 sync configured tag，不报错。
- 若 supervisor 请求显式带 digest，则跳过 sync request tag。
- 若 sync configured tag 失败且回滚也失败，最终状态为 `failed`。
- 若 `rollback_on_failure=false` 且 supervisor sync request tag 失败，则直接 `failed`，不自动回滚。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Update job progress/summary internals | internal | internal | Modify | None | backend | backend/web | 仅新增内部失败阶段与进度口径 |
| Supervisor self-upgrade state/logs | internal | internal | Modify | None | supervisor | supervisor UI | 仅新增内部 step/log 文案 |

### 契约文档（按 Kind 拆分）

None

## 验收标准（Acceptance Criteria）

- Given tag-based service 通过 Dockrev 更新到新 digest，When 更新成功结束，Then `docker image inspect <compose 原始 repo:tag>` 解析到当前运行镜像，随后直接执行 `docker compose up -d` 不会切回旧镜像。
- Given `scope=stack/all` 更新多个 service，When 每个 service 完成更新，Then 所有实际变更的 tag-based service 都满足相同的不回退保证，且 semver pull 只发生在 tag 对齐成功之后。
- Given 普通更新在 health 通过后、sync configured tag 阶段失败，When 任务结束，Then 状态不是 `success`，且会先尝试恢复旧 tag + `up --pull never` 回滚。
- Given supervisor apply 成功路径，When 最终状态变为 `succeeded`，Then `target_image_repo:<request.tag>` 已指向当前运行镜像。
- Given supervisor apply 在 sync request tag 阶段失败且 `rollback_on_failure=true`，When 任务结束，Then 会自动回滚而不是留在伪 success。

## 实现前置条件（Definition of Ready / Preconditions）

- 目标/非目标、范围（in/out）、约束已明确。
- 验收标准覆盖普通更新与 supervisor 两条主链路，以及 sync tag 失败分支。
- 接口契约明确为 internal-only 口径调整，无外部 schema 变更。
- 关键取舍已冻结：采用“本地 tag 对齐”，不改写 compose 文件。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: `cargo test -p dockrev-api`
- Integration tests: `cargo test -p dockrev-supervisor`
- E2E tests (if applicable): None

### UI / Storybook (if applicable)

- Stories to add/update: None
- Visual regression baseline changes (if any): None

### Quality checks

- Formatting / lint as repo existing baseline requires for touched Rust crates

## 文档更新（Docs to Update）

- `docs/specs/README.md`: 新增本规格索引项并在进度推进时同步状态。

## 计划资产（Plan assets）

- Directory: `docs/specs/upjqw-compose-tag-stability/assets/`
- In-plan references: `![...](./assets/<file>.png)`
- PR visual evidence source: maintain `## Visual Evidence (PR)` in this spec when PR screenshots are needed.
- If an asset must be used in impl (runtime/test/official docs), list it in `资产晋升（Asset promotion）` and promote it to a stable project path during implementation.

## Visual Evidence (PR)

## 资产晋升（Asset promotion）

None

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 普通更新链路补齐 post-health 本地 tag 对齐与失败阶段 `sync_configured_tag`
- [x] M2: supervisor 自升级链路补齐 sync request tag 与失败处理
- [x] M3: 回归测试、review-loop 收敛与快车道交付

## 方案概述（Approach, high-level）

- 复用现有 `docker image tag` 能力，不引入新的 Docker 操作原语。
- 普通更新沿用当前“旧 tag 恢复 + `up --pull never`”的回滚方式，把 sync configured tag 失败纳入同一闭环。
- supervisor 自升级在成功路径补一段最小同步步骤；如失败则沿用现有 `fail_and_maybe_rollback` 决策入口。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若外部系统依赖 update failure step 的旧枚举，新增 `sync_configured_tag` 可能需要同步适配。
- 需要决策的问题：None
- 假设（需主人确认）：生产环境主要冲突源是手工 `docker compose up -d` 复用旧本地 tag，而不是手工 pull。

## 变更记录（Change log）

- 2026-03-08: 新建规格，冻结“本地 compose tag 稳定化”目标、边界与验收口径。
- 2026-03-08: 完成普通更新与 supervisor 自升级的本地 tag 对齐、失败回滚与回归测试。

## 参考（References）

- `docs/specs/69hb2-update-job-idempotent-retry/SPEC.md`
- `docs/specs/m3tq9-service-update-explicit-target-tag/SPEC.md`
- `docs/specs/ttq9u-supervisor-self-upgrade-running-buttons/SPEC.md`
