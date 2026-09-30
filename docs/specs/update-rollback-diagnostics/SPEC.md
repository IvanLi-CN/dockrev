# Dockrev：自动更新回滚诊断

> 当前有效规范以本文为准；实现覆盖与当前状态见 `./IMPLEMENTATION.md`，主题局部演进见 `./HISTORY.md`，持久决策的完整取舍见关联 ADR。

## Context and Scope

自动更新会在候选容器未变为 `healthy` 时自动回滚。当前实现固定等待 90 秒，只观察 health status；回滚替换候选容器后，候选日志与进程状态不再可用。因此无法区分候选崩溃重启、health command 失败和服务尚未完成启动。

Docker 的 health policy 由镜像的 `HEALTHCHECK` 定义，也可由 Compose `healthcheck` 覆盖。固定 90 秒忽略了候选容器的实际 policy，且可能在 Docker 尚未判定 `unhealthy` 时回滚。

### Goals and Non-goals

### Goals

- 在任何健康检查触发的自动回滚前保留 candidate container 的诊断证据。
- 使用候选容器的有效 health policy 推导 health-policy deadline，让项目开发者的镜像定义和运维的 Compose 覆盖都实际生效。
- 让批量更新中的每个失败服务拥有独立的原始容器运行日志文件，并作为一个压缩二进制字段随 update job 保存；采集范围是 Docker 在回滚前当前可提供的日志，Dockrev 不解析、不脱敏、不主动截断日志。
- 维持自动回滚优先于诊断可用性的语义。

### Non-goals

- 不增加 Dockrev 全局、Stack 或 Service 的健康检查期限覆盖配置。
- 不改变没有 healthcheck 的服务的既有更新语义。
- 不重试生产更新、不部署生产环境，也不以历史 SQLite trigger 异常推定候选失败根因。
- 不将候选原始输出写入通用 job log、SSE 或 jobs 列表。

### Topic Boundaries

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

## Requirements

### MUST

### REQ-ROLLBACK-001
- Dockrev 必须在 Compose 更新创建 candidate container 后，从该容器的 `.Config.Healthcheck` 读取有效 health policy；不得仅从镜像 manifest 推导。
### REQ-ROLLBACK-002
- 推导必须采用 Docker 默认值：`interval=30s`、`timeout=30s`、`startPeriod=0s`、`startInterval=5s`、`retries=3`，并使用候选实际配置覆盖相应默认值。
### REQ-ROLLBACK-003
- health-policy deadline 必须是保守上界：`startPeriod + max(interval, startInterval) + retries * (interval + timeout) + pollInterval`。`pollInterval` 是当前健康状态观察间隔。
### REQ-ROLLBACK-004
- health status 为 `healthy` 时必须接受候选；为 `unhealthy` 时必须立即进入证据采集后回滚；持续 `starting` 至 health-policy deadline 时必须以 deadline 为失败原因进入证据采集后回滚。
### REQ-ROLLBACK-005
- 只有候选进入自动回滚路径时才创建 rollback evidence spool、归档和下载附件。候选健康通过并完成更新时，不创建这些 Dockrev 证据文件；Docker 自己管理的正常容器日志不属于本证据附件。
### REQ-ROLLBACK-006
- 每个失败 candidate container 的原始 `docker logs --timestamps` stdout 与 stderr 必须按 Docker CLI 输出顺序合并到同一个 `container.log`，从 Docker 当前仍可提供的首条日志开始，以磁盘流式方式保存到 EOF；不得设置应用侧字节或行数上限，不得解析、脱敏、重编码或主动截断。日志命令使用 300 秒 watchdog；若命令未能读到 EOF、watchdog 到期、命令失败或写盘失败，必须保留已写入的部分文件并明确记录证据不完整及原因，不得将部分内容标记为完整。Dockrev 只能判断本次 Docker 命令是否完整读到 EOF，不能检测或恢复 Docker 在本次采集前已轮转、删除的历史日志；证据不声称包含 Docker 已不可提供的内容。
### REQ-ROLLBACK-007
- 每个失败候选必须在回滚前写入该 job 的私有临时 spool：候选 ID、服务 ID、健康期限与最后 health status、`State.Status`、`State.Error`、`ExitCode`、`RestartCount`、`State.Health.Log` 和该服务日志文件。
### REQ-ROLLBACK-008
- 已捕获的日志和 health log 必须原文保存，不做脱敏或内容变换。示例、测试夹具、UI 演示和文档不得包含真实凭据或敏感环境变量。
### REQ-ROLLBACK-009
- 一个 update job 的 archive 必须是单一 `tar.zst` BLOB；每个失败服务拥有独立 archive directory，服务之间不得混合日志。
### REQ-ROLLBACK-010
- spool 文件必须在候选自动回滚前以原子写入完成，并仅允许 Dockrev 运行用户读取。候选删除、证据采集失败、spool 失败、归档失败或 BLOB 持久化失败都不得阻止既有自动回滚。
### REQ-ROLLBACK-011
- 归档 BLOB 与 `rollbackEvidence` summary 必须在同一数据库事务提交；只有提交成功后才能删除对应 spool。若归档写入失败，DockRev 必须在不带新 archive 的事务中完成既有 job 终态提交，将 evidence summary 标记为 `incomplete` 并记录有界错误；spool 保留供后续恢复，不能静默删除。若 evidence spool 初始化失败且候选健康检查随后触发回滚，任务 summary 必须记录 `rollbackEvidence.status=incomplete` 和有界初始化错误，不得创建下载附件。启动时先运行既有通用 job 恢复，再尝试附加带中断检查点的部分证据；对于仍由既有延后 update-backup recovery 处理的任务，证据可能先于该延后恢复完成而附加。证据恢复本身不改变任务状态，后续状态仍由既有恢复流程决定，并将 `logsTruncated` 保持为 true。若恢复清单、partial log、归档重建、归档大小读取或归档附加失败，job summary 必须在 archive BLOB 不存在时记录 `rollbackEvidence.status=incomplete` 与有界原因，并保留 spool；若 archive BLOB 已存在，不得以恢复失败元数据覆盖现有归档状态。对于已有 archive BLOB 的终态 job，恢复必须在读取 manifest 前清理残余 spool、本地归档和 part 文件，避免损坏或缺失的 manifest 使原始日志副本滞留。
- 启动恢复必须在 API listener 成功绑定后以后台任务执行；旧 job 的 evidence summary 可以先显示 `incomplete`，下载 endpoint 在 archive BLOB 提交前返回 `404`，归档成功附加后才转为 `available`。这项后台任务必须与其他证据恢复扫描使用同一串行化边界。
- 终态恢复读取的 checkpoint 记录数在 incomplete summary 已有 `failedCandidates` 时必须与其一致；数量不一致时不得将旧 checkpoint 附加为可下载归档，必须保留已有服务元数据、候选总数和 spool，并记录有界校验错误。若此前因 manifest 不可读而没有已知计数，则使用当前可解析 checkpoint 的记录数，不得把未知计数伪造为 0。若终态 summary 明确记录最终 manifest 写入失败，恢复只能依据可解析的 checkpoint 重试；该重试标记必须在有界摘要错误合并时保留，直到重试成功。此重试必须保守地将候选 `logsTruncated` 设为 `true`、记录完整性不确定原因并重写 manifest 后再建档。缺失、损坏或仍无法写入时，不得猜测候选记录或附加旧 manifest。
- 带有 update-stop 恢复快照的 job 必须在每次进程启动时重置上一进程留下的恢复领取标记，并且通用 incomplete-job recovery 不得终结仍持有该快照的 job。专用恢复流程随后重新领取并恢复服务；即使进程在领取快照后、恢复服务前再次退出，下一次启动仍必须重试。恢复成功后，job 终态与恢复快照清除必须在同一数据库事务提交，避免在终结与清除之间退出后再次执行服务恢复。
### REQ-ROLLBACK-012
- 终态 job 的既有保留期清理必须同时删除与该 job 对应的遗留 spool；这属于 job 到期删除，不得产生无主原始日志文件。
### REQ-ROLLBACK-013
- 对带有 archive BLOB 的终态 job，启动恢复必须在读取 manifest 前清理 spool、`.tar.zst` 和 `.tar.zst.part`，包括 spool 目录已缺失的 archive-only/part-only 文件。删除失败时必须记录告警并保留未删副本，不得覆盖已提交的归档状态。恢复附加必须只在 evidence BLOB 尚为空时执行；已有 BLOB 及其 summary 对恢复过程不可覆盖。
- 每种 `rollbackEvidence` summary（包括常规终结与恢复失败）最多包含 32 条服务记录、每服务最多 4 条捕获错误、每个元数据文本字段最多 256 个字符、最多 16 条顶层错误且每条最多 512 个字符。发生截断时必须附带有界说明并保留已知失败候选总数；若恢复清单无法读取且没有既有计数，不得将未知数写成 0，必须省略 `failedCandidates`，后续恢复不得因此拒绝有效 manifest。这些上限只适用于 summary；归档 manifest 和已采集日志必须保持原始内容。
- `State.Error` 可能包含 Docker 运行时细节，只能保留在私有 spool/归档 manifest 中，不得进入普通 job summary、列表、详情正文、job log、SSE 或通知；授权下载的完整归档仍保留其原始值。
- 恢复失败不得用空服务列表或空错误列表抹除先前已记录的有界服务诊断和错误。顶层错误达到限额时，应优先保留因 32 条服务摘要限制而不会显示在服务列表中的候选捕获错误；所有摘要仍遵守上述数量和文本长度限制。
### REQ-ROLLBACK-014
- jobs 列表、通用 job log、SSE 和实时终端不得包含 archive 内容；完整 archive 仅可由现有 `require_user` 授权路径读取。

### SHOULD

### REQ-ROLLBACK-015
- summary 必须包含 `rollbackEvidence` 元数据：状态、可确定时的已采集失败候选数、archive format、compression、每服务日志完整性、归档大小和采集/归档错误。它不得包含原始日志正文或 Docker `State.Error`；无法确定失败候选数时省略该字段，不得用 0 代替未知数。
### REQ-ROLLBACK-016
- 状态采集与日志流采集应并行执行；日志采集不得使用固定字节上限，300 秒 watchdog 只用于终止阻塞的日志命令并保护回滚时序。无法完成流式采集时必须独立记录不完整原因，且不阻止既有自动回滚。
### REQ-ROLLBACK-017
- 已有 job 记录、没有 evidence 的 job 和没有 healthcheck 的服务必须保持 API 兼容。

### REQ-ROLLBACK-018
- 常规完成与恢复成功产生的 `rollbackEvidence` summary 也必须遵守 `REQ-ROLLBACK-013` 的元数据上限；被省略或缩短的数据不得影响 archive manifest、`container.log` 原始字节、`logsBytes`、`logsTruncated` 或总失败候选数。

## Behavior Details

### Core flows

1. Compose 创建 candidate container 后，Dockrev 读取该候选的有效 health policy。项目开发者通过 Dockerfile `HEALTHCHECK` 调整 policy；运维通过 Compose `healthcheck` 覆盖该 policy。由于读取目标是实际创建的候选容器，两者都影响 deadline。
2. Dockrev 观察 candidate container 的 health status。`healthy` 接受更新；`unhealthy` 或在推导 deadline 时仍为 `starting` 都进入同一失败处理。
3. 在启动 Docker 日志命令前，先将私有 spool 的候选目录、占位状态文件和含“捕获中断”标记的 manifest 原子落盘，供进程中断后的启动恢复使用。随后并行读取状态、health log 和从首条可用记录开始的容器运行日志；Docker CLI stdout/stderr 在进程边界合并后分块直写 `container.log.part`，300 秒 watchdog 覆盖输出文件创建与流式写盘。命令正常退出、合并输出到 EOF 且写盘成功后，原子重命名为 `container.log` 并更新 manifest，标记 `logsTruncated=false`；超时、非零退出、未到 EOF 或写盘/重命名失败时保留部分文件、标记 `logsTruncated=true` 并记录原因，然后继续既有自动回滚。捕获错误不改变回滚决定。
4. 任务结束时，Dockrev 将 spool 组装为 `tar.zst`。archive 含有无敏感样例的 manifest 和每服务的状态、health log、container log 文件。归档文件按块写入现有 jobs BLOB，BLOB 和 summary 在同一事务中保存；若 BLOB 写入失败，则以 `incomplete` summary 完成 job 终态提交并保留 spool。API listener 成功绑定后，启动恢复在后台以分块写入方式附加归档，不创建第二份持久归档；恢复期间 job 可见但下载附件尚未就绪。
5. Job Detail 读取 summary 以显示 evidence 可用性；可用时，经现有授权下载原始 `tar.zst`。终态 job 被既有 GC 删除时，BLOB 与证据一并删除。

### Edge cases / errors

- Docker healthcheck 不存在时，沿用当前无需健康等待的接受路径，不生成 health rollback evidence。
- 候选健康通过并完成更新时，任务的 evidence BLOB 保持为空、summary 不包含 `rollbackEvidence`，且不得留下该任务的 evidence spool 或 archive 文件。
- Docker Engine 或 Compose 版本不提供 `startInterval` 时，使用 Docker 默认 `5s`；它只参与 deadline 推导，不要求改写镜像或 Compose 文件。
- 候选存在 health status 但有效 policy 读取失败时，不套用固定期限；Dockrev 仅等待 Docker 明确报告 `healthy` 或 `unhealthy`，并在失败证据 metadata 中保留缺失的 policy/deadline 状态。
- 日志命令超过 300 秒 watchdog、候选在采集时消失或磁盘写入失败时，既有 rollback 继续执行。若 rollback 成功，job status 为 `rolled_back`。其后的 archive 持久化失败不得阻止该终态提交；summary 使用 `incomplete` 并记录有界错误，spool 保留供启动恢复。`rollbackEvidence.status` 使用 `available`、`incomplete` 或 `absent` 说明归档状态，服务级 `logsTruncated` 标记日志是否未完整捕获。
- 若进程在 spool 成功、archive 完成前退出，启动恢复依据 job ID 和 spool 状态尝试归档。恢复不得把未成功归档的 spool 当作可删除文件。
- 若最终 manifest 写入失败，后续恢复必须基于有效 checkpoint 重建 manifest 和归档，并保守标记日志完整性不确定；恢复再次失败时保留 spool，不能把旧 manifest 标记为可用归档。
- 一个批量 update job 可以包含多份失败证据。每个服务的 archive 大小随 Docker 可用日志量增长，并随 job 的既有保留期删除；不存在 Dockrev 侧的日志大小上限。

## Interfaces and Contracts

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| jobs rollback evidence storage | database | internal | Modify | ./contracts/db.md | dockrev-api | updater, job history GC | one nullable BLOB on jobs |
| `GET /api/jobs/{job_id}` | HTTP API | external | Modify | ./contracts/http-api.md | dockrev-api | Job Detail | metadata only |
| `GET /api/jobs/{job_id}/rollback-evidence` | HTTP API | external | Modify | ./contracts/http-api.md | dockrev-api | Job Detail, operators | authorized archive download |

### 契约文档（按 Kind 拆分）

- [Database contract](./contracts/db.md)
- [HTTP API contract](./contracts/http-api.md)

## Verification

### VER-ROLLBACK-001
- Method: Inspect candidate container configuration in updater tests.
- covers: `REQ-ROLLBACK-001`
- Pass condition: the effective policy comes from the candidate container, including Compose overrides.

### VER-ROLLBACK-002
- Method: Exercise policy derivation and paused-time updater tests.
- covers: `REQ-ROLLBACK-002`, `REQ-ROLLBACK-003`, `REQ-ROLLBACK-004`
- Pass condition: Docker defaults and the conservative deadline are honored, and healthy/unhealthy/deadline outcomes follow the specified transition.

### VER-ROLLBACK-003
- Method: Run update transition tests and inspect job spool/archive state.
- covers: `REQ-ROLLBACK-005`, `REQ-ROLLBACK-007`, `REQ-ROLLBACK-010`
- Pass condition: capture starts before rollback, failures do not block rollback, and successful updates leave no evidence spool or archive.

### VER-ROLLBACK-004
- Method: Compare captured fixture bytes and exercise watchdog, process-failure, and write-failure cases.
- covers: `REQ-ROLLBACK-006`, `REQ-ROLLBACK-008`
- Pass condition: normal EOF preserves exact Docker output bytes; incomplete capture preserves partial bytes, marks `logsTruncated=true`, and records a reason without redaction or ordinary-log leakage.

### VER-ROLLBACK-005
- Method: Extract generated archives and inspect database transactions and failure recovery.
- covers: `REQ-ROLLBACK-009`, `REQ-ROLLBACK-011`
- Pass condition: each service remains isolated in the single archive, archive and summary commit atomically, and failed persistence preserves recoverable spool data.

### VER-ROLLBACK-006
- Method: Run startup recovery, committed-archive cleanup, archive-attachment race, summary-bound, and terminal-job retention tests.
- covers: `REQ-ROLLBACK-012`, `REQ-ROLLBACK-013`
- Pass condition: committed archives lose local residue including archive-only files; cleanup failures are reported without overwriting committed metadata; recovery cannot replace a committed BLOB or summary; normal and incomplete summaries obey the same bounds while archive contents remain unmodified; job GC removes matching evidence.

### VER-ROLLBACK-007
- Method: Exercise job API authorization and ordinary-output assertions.
- covers: `REQ-ROLLBACK-014`, `REQ-ROLLBACK-017`
- Pass condition: unauthorized archive access is rejected, authorized download returns original archive bytes, and job detail/list/SSE contain metadata only while existing jobs remain compatible.

### VER-ROLLBACK-008
- Method: Inspect summary fixtures and concurrent candidate-capture tests.
- covers: `REQ-ROLLBACK-015`, `REQ-ROLLBACK-016`, `REQ-ROLLBACK-018`
- Pass condition: summaries contain required structured metadata only, and state/log capture runs concurrently without an application-side byte cap.

### VER-ROLLBACK-009
- Method: Finalize and recover oversized candidate metadata, extract both archive manifests and raw logs, and inspect each summary.
- covers: `REQ-ROLLBACK-013`, `REQ-ROLLBACK-018`
- Pass condition: normal and recovered archive summaries keep service/error counts and text fields within limits, `failedCandidates` remains exact, and both archive manifests/logs retain their original contents.

## Visual Evidence

- Storybook canvas: `pages-jobdetailpage--health-rollback`.
- Confirmed desktop capture: [`rollback-evidence-desktop.png`](./assets/rollback-evidence-desktop.png), viewport `1440x900`.
- Confirmed mobile capture: [`rollback-evidence-mobile.png`](./assets/rollback-evidence-mobile.png), viewport `393x852`; evidence metadata and download action remain on one row without horizontal overflow.
- Captures are mock-only, contain no production data, and were confirmed by the owner before persistence.

## References

- [Dockerfile HEALTHCHECK reference](https://docs.docker.com/reference/dockerfile/#healthcheck)
- [Compose healthcheck reference](https://docs.docker.com/reference/compose-file/services/#healthcheck)
- [Store Rollback Evidence with Its Update Job](../../adr/0002-update-rollback-evidence-storage.md)
