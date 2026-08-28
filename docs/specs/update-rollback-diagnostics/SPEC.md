# Dockrev：自动更新回滚诊断

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，主题局部演进见 `./HISTORY.md`，持久决策的完整取舍见关联 ADR。

## 背景 / 问题陈述

自动更新会在候选容器未变为 `healthy` 时自动回滚。当前实现固定等待 90 秒，只观察 health status；回滚替换候选容器后，候选日志与进程状态不再可用。因此无法区分候选崩溃重启、health command 失败和应用 readiness 未完成。

Docker 的 health policy 由镜像的 `HEALTHCHECK` 定义，也可由 Compose `healthcheck` 覆盖。固定 90 秒忽略了候选容器的实际 policy，且可能在 Docker 尚未判定 `unhealthy` 时回滚。

## 目标 / 非目标

### Goals

- 在任何健康检查触发的自动回滚前保留 candidate container 的诊断证据。
- 使用候选容器的有效 health policy 推导 health-policy deadline，让项目开发者的镜像定义和运维的 Compose 覆盖都实际生效。
- 让批量更新中的每个失败服务拥有独立、完整行截断的原始日志文件，并作为一个压缩二进制字段随 update job 保存。
- 维持自动回滚优先于诊断可用性的语义。

### Non-goals

- 不增加 Dockrev 全局、Stack 或 Service 的健康检查期限覆盖配置。
- 不改变没有 healthcheck 的服务的既有更新语义。
- 不重试生产更新、不部署生产环境，也不以历史 SQLite trigger 异常推定候选失败根因。
- 不将候选原始输出写入通用 job log、SSE 或 jobs 列表。

## 范围（Scope）

### In scope

- Update apply 的健康等待、健康失败前的证据采集与自动回滚状态机。
- 候选容器有效 health policy 的读取和期限推导。
- 私有临时证据 spool、`tar.zst` 归档、jobs BLOB 迁移与启动恢复。
- Update summary、受现有授权保护的证据下载接口，以及 Job Detail 的下载入口。
- 单元、API 和共享 Docker 测试机的回归验证。

### Out of scope

- 读取或保存候选容器环境变量、完整 Docker inspect 文档或 Compose 文件内容。
- 变更现有 auth allowlist/group 模型，或把证据下发到未授权请求。
- 变更既有 30 天终态 job 保留期。

## Related ADRs

- [Store Rollback Evidence with Its Update Job](../../adr/0002-update-rollback-evidence-storage.md)

## 需求（Requirements）

### MUST

- Dockrev 必须在 Compose 更新创建 candidate container 后，从该容器的 `.Config.Healthcheck` 读取有效 health policy；不得仅从镜像 manifest 推导。
- 推导必须采用 Docker 默认值：`interval=30s`、`timeout=30s`、`startPeriod=0s`、`startInterval=5s`、`retries=3`，并使用候选实际配置覆盖相应默认值。
- health-policy deadline 必须是保守上界：`startPeriod + max(interval, startInterval) + retries * (interval + timeout) + pollInterval`。`pollInterval` 是当前健康状态观察间隔。
- health status 为 `healthy` 时必须接受候选；为 `unhealthy` 时必须立即进入证据采集后回滚；持续 `starting` 至 health-policy deadline 时必须以 deadline 为失败原因进入证据采集后回滚。
- 每个失败 candidate container 的原始 `docker logs --timestamps` 必须从首条日志开始，按完整行保留至最多 `1 MiB`。下一完整行超过上限时不得写入部分行，且必须记录 `logsTruncated=true`；首行超过上限时同样记录截断且不写入部分首行。
- 每个失败候选必须在回滚前写入该 job 的私有临时 spool：候选 ID、服务 ID、健康期限与最后 health status、`State.Status`、`State.Error`、`ExitCode`、`RestartCount`、`State.Health.Log` 和该服务日志文件。
- 已捕获的日志和 health log 必须原文保存，不做脱敏或内容变换。示例、测试夹具、UI 演示和文档不得包含真实凭据或敏感环境变量。
- 一个 update job 的 archive 必须是单一 `tar.zst` BLOB；每个失败服务拥有独立 archive directory，服务之间不得混合日志。
- spool 文件必须在候选自动回滚前以原子写入完成，并仅允许 Dockrev 运行用户读取。候选删除、证据采集失败、spool 失败、归档失败或 BLOB 持久化失败都不得阻止既有自动回滚。
- 只有 BLOB 与终态 summary 均提交成功后才能删除对应 spool。启动恢复必须重新归档遗留 spool，或明确记录归档失败而不静默删除原始证据。
- 终态 job 的既有保留期清理必须同时删除与该 job 对应的遗留 spool；这属于 job 到期删除，不得产生无主原始日志文件。
- jobs 列表、通用 job log、SSE 和实时终端不得包含 archive 内容；完整 archive 仅可由现有 `require_user` 授权路径读取。

### SHOULD

- summary 必须包含 `rollbackEvidence` 元数据：状态、失败候选数、archive format、compression、每服务截断标记、归档大小和采集/归档错误。它不得包含原始日志正文。
- 状态与日志采集应并行执行，并沿用现有单命令 10 秒时限；任一路失败必须独立记录，另一路仍可被归档。
- 已有 job 记录、没有 evidence 的 job 和没有 healthcheck 的服务必须保持 API 兼容。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

1. Compose 创建 candidate container 后，Dockrev 读取该候选的有效 health policy。项目开发者通过 Dockerfile `HEALTHCHECK` 调整 policy；运维通过 Compose `healthcheck` 覆盖该 policy。由于读取目标是实际创建的候选容器，两者都影响 deadline。
2. Dockrev 观察 candidate container 的 health status。`healthy` 接受更新；`unhealthy` 或在推导 deadline 时仍为 `starting` 都进入同一失败处理。
3. 失败处理并行读取有限状态字段和首部日志，将服务专属文件原子写入 job spool，然后继续既有自动回滚。捕获与写盘错误记入该服务的 evidence metadata，不改变回滚决定。
4. 任务结束时，Dockrev 将 spool 组装为 `tar.zst`。archive 含有无敏感样例的 manifest 和每服务的状态、health log、container log 文件。归档 BLOB 和 summary 在同一 jobs 更新中保存。
5. Job Detail 读取 summary 以显示 evidence 可用性；可用时，经现有授权下载原始 `tar.zst`。终态 job 被既有 GC 删除时，BLOB 与证据一并删除。

### Edge cases / errors

- Docker healthcheck 不存在时，沿用当前无需健康等待的接受路径，不生成 health rollback evidence。
- Docker Engine 或 Compose 版本不提供 `startInterval` 时，使用 Docker 默认 `5s`；它只参与 deadline 推导，不要求改写镜像或 Compose 文件。
- 候选存在 health status 但有效 policy 读取失败时，不套用固定期限；Dockrev 仅等待 Docker 明确报告 `healthy` 或 `unhealthy`，并在失败证据 metadata 中保留缺失的 policy/deadline 状态。
- 采集命令超时、候选在采集时消失、磁盘写入失败或 archive 持久化失败时，任务仍继续 rollback。若 rollback 成功，job status 仍为 `rolled_back`；`rollbackEvidence.status` 使用 `available`、`incomplete` 或 `absent` 说明归档状态。
- 若进程在 spool 成功、archive 完成前退出，启动恢复依据 job ID 和 spool 状态尝试归档。恢复不得把未成功归档的 spool 当作可删除文件。
- 一个批量 update job 可以包含多份失败证据。每个服务都有 `1 MiB` 日志上限；archive 总大小随失败服务数增长，并随 job 的既有保留期删除。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| jobs rollback evidence storage | database | internal | Modify | ./contracts/db.md | dockrev-api | updater, job history GC | one nullable BLOB on jobs |
| `GET /api/jobs/{job_id}` | HTTP API | external | Modify | ./contracts/http-api.md | dockrev-api | Job Detail | metadata only |
| `GET /api/jobs/{job_id}/rollback-evidence` | HTTP API | external | New | ./contracts/http-api.md | dockrev-api | Job Detail, operators | authorized archive download |

### 契约文档（按 Kind 拆分）

- [Database contract](./contracts/db.md)
- [HTTP API contract](./contracts/http-api.md)

## 验收标准（Acceptance Criteria）

- Given 一个候选在当前 90 秒点仍为 `starting`，且其有效 policy 推导 deadline 更晚，When 执行 update，Then Dockrev 不得在 90 秒回滚，并继续观察至 `healthy`、`unhealthy` 或推导 deadline。
- Given Compose 覆盖了镜像 healthcheck，When Dockrev 读取候选容器 policy，Then summary 的 policy 与 deadline 必须反映候选有效配置而非镜像默认配置。
- Given 健康 status 为 `unhealthy` 或 deadline 到期仍为 `starting`，When 自动回滚开始，Then 对应服务的 spool 已包含状态与最多 `1 MiB` 的首部完整行日志。
- Given 一条下一日志会超过 `1 MiB`，When 采集该服务日志，Then archive 中不包含部分行，且 metadata 标记截断。
- Given 批量更新中两个服务分别失败，When 下载 archive，Then 两组日志与状态位于独立服务目录，且每组独立遵守 `1 MiB` 限制。
- Given 状态采集、日志采集、spool、归档或 BLOB 写入失败，When 服务需要回滚，Then 自动回滚仍会执行；成功回滚的 job 保持 `rolled_back`，且 summary 精确说明 evidence 不可用原因。
- Given archive 已提交，When 调用 jobs 列表、查看通用 job log 或订阅 SSE，Then 响应不包含 archive 正文；已授权用户下载专用 endpoint 时获得原始 `tar.zst`。
- Given Dockrev 在 archive 提交前中断，When 服务恢复启动，Then 遗留 spool 被归档或被明确标记失败，且不会被静默删除。
- Given 终态 job 超过既有保留期，When job GC 删除该 job，Then evidence BLOB 与同 job 的遗留 spool 一同删除。

## 验收清单（Acceptance Checklist）

- [x] 健康期限来自 candidate container 的有效 policy。
- [x] 健康失败和 deadline 失败均在回滚前保存证据。
- [x] 单服务日志首部完整行上限为 `1 MiB`。
- [x] 批量更新的服务证据在 archive 中分离。
- [x] 失败证据不会改变自动回滚语义。
- [x] archive 的授权、下载与保留期边界清晰。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Focused updater tests with paused Tokio time: 90 秒 `starting` 回归、有效 policy 推导、`unhealthy`、deadline 和 evidence-before-rollback 顺序。
- Database migration and recovery tests: BLOB 兼容、archive/summary 原子提交、终态 job GC、遗留 spool 恢复。
- API tests: 未授权拒绝、详情仅含 metadata、列表和 SSE 不含正文、授权下载保持 archive bytes。
- Shared Docker testbox integration: Dockerfile healthcheck 与 Compose override 分别改变候选实际 policy，批量服务的 archive entries 可解包验证。

### UI / Storybook

- Job Detail 增加 evidence 下载可用、不可用和不存在状态的 Storybook 场景。
- 下载入口的 interaction 覆盖授权 metadata、可用 archive 与不可用提示。
- 实现阶段按项目视觉证据流程验证 Job Detail 桌面和移动布局。

## Visual Evidence

- Storybook canvas: `pages-jobdetailpage--health-rollback`.
- Confirmed desktop capture: [`rollback-evidence-desktop.png`](./assets/rollback-evidence-desktop.png), viewport `1440x900`.
- Confirmed mobile capture: [`rollback-evidence-mobile.png`](./assets/rollback-evidence-mobile.png), viewport `393x852`; evidence metadata and download action remain on one row without horizontal overflow.
- Captures are mock-only, contain no production data, and were confirmed by the owner before persistence.

### Quality checks

- `cargo test -p dockrev-api`
- 相关 Rust formatting and lint checks
- Web typecheck, Storybook build and targeted interaction tests
- Rust transition tests and jobs schema migration checks live in sibling modules so the repository file-budget gate remains within its 1500-line limit.

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：完整候选日志可能包含敏感内容。已确认不做脱敏，因此其可见范围严格复用当前 jobs 的授权边界，且不进入直播或列表数据。
- 风险：批量失败服务数较多时 archive 增长。每服务 `1 MiB` 原始日志上限与既有 job GC 是唯一的空间边界，不引入额外健康等待上限。
- 假设：Dockrev 的私有数据目录支持 owner-only 的原子文件创建和启动恢复扫描。
- 已决：不把 SQLite trigger 碰撞作为此次候选回滚根因；archive 只为后续实际诊断保留证据。

## 参考（References）

- [Dockerfile HEALTHCHECK reference](https://docs.docker.com/reference/dockerfile/#healthcheck)
- [Compose healthcheck reference](https://docs.docker.com/reference/compose-file/services/#healthcheck)
- [Store Rollback Evidence with Its Update Job](../../adr/0002-update-rollback-evidence-storage.md)
