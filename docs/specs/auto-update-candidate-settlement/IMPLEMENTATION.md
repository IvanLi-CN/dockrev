# Dockrev：自动更新候选收敛与发布边界实现准备

## Current Status

- Implementation: complete
- Lifecycle: active
- 已实现候选事实表、幂等 settlement、digest-bound SemVer 门禁、策略重评估、重试/周期 reconciliation 和兼容 API/UI 投影；迁移对不可证明历史采用 fail-closed 处理。
- 自动策略 update job 先持久化为 queued，再通过候选有效性 CAS 转为 running；候选替代会取消尚未启动的 queued job，并在 apply 尚未提交时请求停止已运行 job。
- 候选来源以结构化 provenance 保存为 schedule、github_webhook 或 unknown；unknown 历史只保留审计，不获得自动部署授权。
- migration 0020 与运行时 hydration 从 discovery history 选择最早可信的成功 check observation；缺失 image、baseline、source job 或合格来源时生成 unresolved 的 `migration_ambiguous_history` 审计事实，并保持 source unknown。
- migration 0022 对已执行旧 migration 的 active pending 重新应用相同的 Check、成功状态、授权 creator/source pairing 与 scope identity 门禁；不合格的 queued job 会取消，running job 只写入 stop control，pending 保留 `migration_ambiguous_history` 审计结果。
- hydration 事务只创建 candidate fact、supersede 旧候选和跳过旧 pending；启动/周期 reconciliation 随后复用现有 evaluator 与 claim safety，不直接执行 Compose。
- policy reconciliation 显式包含 `awaiting_inference` candidate；SemVer evaluator 仍返回 waiting 状态并保持无 pending/update job 的 fail-closed 语义。
- hydration diagnostic 仅在当前 digest 没有 candidate row 且存在 discovery history 时报告 `candidate_missing`；已有运行时 candidate 不会被错误报告为缺失。
- 自动策略 job 插入事务内重新确认 service current digest，并以 candidate/pending/policy scope 的完整 context 做原子 enqueue guard；缺少 candidate provenance 时 fail-closed，不回退到 digest-only 插入路径。pending claim 与 enqueue 共同要求成功 Check、授权 source/creator pairing 和稳定的 scope identity，避免 stale candidate 或非 Check source 获得执行授权。
- `JobScope` source scope 解析对大小写不敏感，与 migration predicate 保持一致；不合格的历史 queued/running action 分别取消或写入 stop control，保留 `migration_ambiguous_history` 审计原因。
- inference 开始时以原子递增 `settlement_generation` 发出唯一 evidence CAS token；settlement 只能提交与当前 token 精确匹配的 generation，迟到的旧 inference 结果不能覆盖更新的 retry 或 ready 结果。
- recovery queued auto-policy job 在恢复执行前重新验证成功 Check、schedule/GHCR webhook provenance、creator/scope identity、candidate target digest、expected current digest、当前 service digest、candidate 和 policy；缺少任一基线则 fail-closed，不能绕过 source/current-digest 门禁。
- ambiguous hydration 不伪造 `source_job_id` 或 `discovered_at`；这些缺失值在候选事实和 API diagnostic 中保持为空，`migration_ambiguous_history` 只作为不可执行的审计原因。
- queued recovery 必须同时匹配 pending 的 candidate identity、service scope、唯一 target、当前 service tag、candidate digest 和 expected current digest；缺失 candidate 绑定、target tag 或 scope/target service 不一致都会取消 queued action。
- queued recovery 的 fail-closed 取消和损坏历史都保留 `migration_ambiguous_history` audit reason；普通运行时 claim 仍使用 `policy_changed_before_start`，不混淆两类来源。
- hydration supersession 对已有或新建的 running stop-control 都写入 `auto-policy-supersession`，并尊重已提交 apply 的保护条件。
- hydration supersession 使用数据库实际返回的 current candidate id，兼容运行时创建的非默认 candidate id，避免写入悬空的 `superseded_by_candidate_id`。
- hydration 会恢复所有 qualifying discovery history 的 digest，并以当前 service candidate 统一 supersede 非当前历史 candidate，确保旧 pending/action 不能因迁移只看到当前 digest 而重新执行。
- candidate identity 在 hydration 分组、upsert、settlement、inference、policy projection 和诊断查询边界统一使用共享 digest 规整，避免 legacy 大小写或缺省 `sha256:` 前缀制造重复候选。
- migration `0024_normalize_auto_update_digest_identity` applies the same digest identity rule to services, candidates and pending rows. It first consolidates equivalent active pending rows before canonicalization, preserves the enqueued row with the strongest existing action, skips duplicate pending facts, cancels duplicate queued auto-policy jobs, and writes stop controls for duplicate running jobs. Reopening the database does not create another action.
- migration `0024_normalize_auto_update_digest_identity` also consolidates candidate rows before writing canonical digests: it chooses a deterministic keeper, rebinds pending rows to that keeper, cancels or stop-controls duplicate candidate actions that have no retained active pending row, and removes duplicate candidate facts so the existing unique key represents canonical identity.
- Before removing an equivalent candidate row, migration `0024` merges the earliest qualified `sourceJobId`/`source`/`discoveredAt`, the strongest settlement facts, and any still-active policy action projection into the deterministic keeper. Reserve, current claim, and latest-candidate reads all use the same canonical digest identity expression.
- migration `0025_normalize_new_version_notification_digest_identity` canonicalizes notification digests, preserves the sent active row when equivalent active notification records collide, marks the other active rows superseded with an audit reason, and keeps repeated startup migration idempotent. Notification reservation and current-service checks use the same canonical digest normalizer.
- legacy candidate backfill in migration `0014` accepts only a successful `check` whose schedule or GitHub webhook creator/source pairing and service/stack/all scope match the pending service; incomplete or non-check history is skipped and its queued/running action is cancelled or stop-controlled before the pending row is marked ambiguous.
- queued auto-policy recovery uses the same canonical digest expression for service, candidate, pending, target and expected-current-digest comparisons, so a prefixless or case-variant target cannot be rejected or accepted differently from enqueue identity.
- discovery history is ordered by `discovered_at ASC, id ASC` before canonical digest grouping; hydration selects the earliest complete, source-qualified observation and never replaces its first `discovered_at` with a later duplicate observation.
- current-digest CAS and hydration diagnostics also use the shared canonical digest SQL, so legacy prefixless and case-variant digests cannot bypass service identity checks or hide a missing candidate.
- Validation: the five focused Rust regression tests, `cargo check -p dockrev-api`, formatting and diff checks pass. The shared-testbox Compose smoke built the current binary and passed the plugin and standalone lifecycle modes; the Compose V1 rejection mode timed out waiting for health and did not produce a summary, matching the existing metrics migration blocker (<code>retained rollups cannot be recovered after raw retention</code>) recorded below.

## 实现顺序

### M1：候选数据模型与数据库不变量

- 新增 <code>auto_update_candidates</code> 表、索引和迁移。
- 固定 <code>serviceId + candidateDigest</code> 的候选唯一键。
- 保存 discovery provenance、discoveredAt、raw tag、current digest、digest-bound resolved version/tags、settlement、retry 和 supersession。
- 将现有 active <code>auto_update_pending</code> 与 candidate identity 建立明确关联；保留旧 action 记录。
- 为 candidate settlement 和 policy action 分开定义状态转换，禁止使用一个 pending 字段覆盖两种生命周期。

### M2：Canonical candidate settlement

- 抽取统一的 candidate settlement service，供 service check、snapshot/inference worker、notification、auto policy 和 API 使用。
- 严格 SemVer raw tag 直接 ready；floating tag 先 awaiting inference。
- 读取 exact digest registry snapshot 和 OCI explicit version，并使用共享规整 helper。
- 将 ready、unresolved、superseded 及 reason 写入数据库；重复 settlement 必须幂等。

### M3：事件唤醒与 reconciliation

- inference worker 在 settlement transaction commit 后发布 <code>settlement event</code>。
- event consumer 只负责唤醒相关 candidate 的 policy re-evaluation，不把 event 当作事实源。
- 启动时恢复非终态 candidate、retryable inference 和 active pending。
- 周期任务执行 bounded reconciliation，补偿事件丢失、worker 重启和策略保存后的状态变化。
- 单实例下使用数据库条件更新和已有 operation ownership；不扩展跨实例协调。

### M4：策略评估、延迟与 claim

- SemVer 只读取 resolved version；Regex/Glob 继续支持 raw tag。
- 将来源校验从 summary 推断改为结构化 schedule/GHCR webhook provenance。
- candidate discovery、settlement、policy save、service recovery 和 reconciliation 都调用同一个 evaluator。
- delayed action 使用 candidate discoveredAt，并同时满足时间和版本滞后门槛。
- enqueue 前重新检查最新 candidate、effective policy、service digest、并发保护和 Dockrev 自身保护。
- 自动 job 采用 deferred enqueue：先写 queued 事实，再以 candidate/pending 条件 claim 启动；candidate supersession 在启动前取消 queued，启动后且 apply 未提交时写入 stop control。
- 自动 update job 使用现有显式 targets/digest update path；不新增镜像发布路径。

### M5：API、UI、通知与历史

- API 增加 candidate settlement 与 policy action 的可选字段，保持既有 payload 兼容。
- Candidate settlement 增加可选 `source`、`sourceJobId`、`hydrationOrigin`；Service/Stack/Overview 增加只读 `candidateHydration` diagnostic。
- UI 区分 inference waiting、unresolved、rule not matched、delayed、queued/running/completed。
- UI 区分 candidate missing、hydrated 与 ambiguous history，并展示 source、source job 与 hydration origin。
- SemVer preview 与后端严格解析语义一致。
- 通知和历史读取 canonical settlement，并按 service + digest 去重。
- 保持原始 job summary 不可变，把 settlement 作为独立事实展示。

### M6：迁移与有限补偿

- 从 active pending 与可证明的 discovery history 建立 candidate 初始状态；0020 保存 `hydration_origin=discovery_history` 及 source job provenance。
- 对来源可证明为 schedule/GHCR webhook 且有 discoveredAt、仍是最新 digest 的候选执行一次有限 reconciliation。
- 迁移恢复严格 SemVer 或 digest-bound display evidence；floating/不可解析版本保持等待或 unresolved，只有当前服务 candidate digest 才能保留 active authorization。
- 来源不明、image/baseline/source job/time 缺失、已 superseded 或历史终态记录不自动补发 update job；当前 digest 的不完整历史保留 `migration_ambiguous_history` audit reason。
- 对同一 candidate 的 inference settlement 使用单调递增 `settlement_generation` token 做条件更新；reconciliation 先原子 claim 当前 generation，再以精确 token 写入证据，旧 worker 的迟到结果保持幂等且不可覆盖新状态。
- recovery 只允许通过与正常 enqueue 相同的 provenance、scope、candidate target/current digest 和 policy 校验；历史 queued action 缺少 persisted current-digest baseline 时直接跳过并保留 `migration_ambiguous_history` 审计原因。
- hydration 按 service + digest 处理完整 discovery history，再根据 service 当前 candidate digest supersede 旧候选；重复 migration、启动 hydration 和周期 reconciliation 不增加 candidate、pending 或 update job。
- migration `0024_normalize_auto_update_digest_identity` 在旧数据库上先处理 active pending 的等价 digest 冲突，再处理候选重复行并 canonicalize service/candidate/pending identity；candidate keeper 选择是确定性的，pending 会重绑到 keeper，无保留 active pending 的重复 action 会取消或写 stop control，重复 candidate fact 会移除，重复打开数据库保持幂等。
- migration `0014` 的 legacy pending 回填与 runtime claim 共用成功 Check、schedule/GitHub webhook creator/source pairing 和 service/stack/all scope identity 门禁；queued recovery 的 service、candidate、target digest 也共用 canonical identity SQL。
- 迁移脚本必须可重复执行，且不能把历史通知直接转换成新的自动部署授权。
- migration `0025` 只重写通知身份和历史状态，不创建 candidate、pending 或 update job，也不改变通知发送 side effect；重复启动只保留一个等价 digest 的 active notification。

## 计划修改边界

### Backend

- <code>crates/dockrev-api/src/auto_update.rs</code>
- <code>crates/dockrev-api/src/db/auto_update.rs</code>
- <code>crates/dockrev-api/src/db/schema.rs</code> 与数据库迁移入口
- <code>crates/dockrev-api/src/service_check.rs</code>
- <code>crates/dockrev-api/src/snapshot_worker.rs</code>
- <code>crates/dockrev-api/src/notify.rs</code>
- <code>crates/dockrev-api/src/api/stacks.rs</code>、<code>api/services.rs</code>、<code>api/types</code>
- 启动任务、事件订阅和 reconciliation 所在的 state/main 模块

### Web

- <code>web/src/api.ts</code>
- <code>web/src/versionDisplay.ts</code>
- Service/Stack candidate 与 auto policy 展示组件
- 相关 Storybook stories、mock payload 和交互测试

### Documentation

- 本主题的 <code>SPEC.md</code>、<code>IMPLEMENTATION.md</code>、<code>HISTORY.md</code>
- [CONTEXT.md](../../../CONTEXT.md)
- [docs/specs/README.md](../README.md)
- [0011-auto-update-candidate-settlement](../../adr/0011-auto-update-candidate-settlement.md)

## 数据模型草案

| 数据 | 事实所有者 | 关键约束 |
| --- | --- | --- |
| Candidate identity | <code>auto_update_candidates</code> | service + digest 唯一；重复观察幂等 |
| Candidate settlement | <code>auto_update_candidates</code> | snapshot/OCI 证据必须绑定 exact digest |
| Inference retry | candidate row | attempt、nextRetryAt、lastError 持久化 |
| Policy action | candidate projection + <code>auto_update_pending</code> | 与 candidate status 分离 |
| Update execution | existing jobs/update operation ledger | 只能由最新 candidate claim |
| Notification identity | notification records | service + digest 去重 |
| Accepted service state | existing service snapshot/settlement | 只有 update terminal settlement 才能改变 |

## 关键转换

~~~text
candidate discovered
  -> awaiting_inference | ready
  -> ready | unresolved
  -> superseded

policy evaluation
  -> waiting_inference
  -> rule_not_matched
  -> delayed
  -> queued
  -> running
  -> completed | skipped
~~~

转换实现必须满足：

- 一个旧事件不能覆盖新 digest 的 candidate settlement；
- 一个旧 candidate 不能覆盖已接受的 service state；
- 一个重复 evaluator 不能产生第二个 active pending 或 update job；
- 一个 retryable error 不能被写成安全的 unresolved 版本；
- 一个 unresolved SemVer candidate 不能被 raw tag fallback 授权部署。

## 事件与事务顺序

~~~text
read registry/snapshot/OCI evidence
        |
        v
DB transaction:
  candidate settlement
  snapshot state
  policy projection if applicable
        |
        v
commit
        |
        v
publish settlement event
        |
        v
reload candidate and evaluate current policy
~~~

事件发布失败时，数据库中的 candidate 仍必须能被周期 reconciliation 处理。事件重复时，consumer 必须按 candidate identity 和 settlement generation 幂等。

## 验证计划

### Focused backend validation

- auto update matcher and validation tests；
- candidate settlement and evidence precedence tests；
- inference retry and reopen tests；
- task-finished subscription race and event-loss tests；
- restart reconciliation tests；
- duplicate claim and duplicate enqueue tests；
- superseded candidate and old pending tests；
- delay/lag/policy-change tests；
- API serialization and backward-compatibility tests。
- runtime candidate hydration diagnostics and awaiting-inference reconciliation regression tests。

### Web validation

- API fixture contains awaiting, ready, unresolved and superseded candidates；
- policy preview differentiates waiting from not matched；
- delayed and update job status copy does not reuse inference pending；
- Storybook covers desktop/mobile states and failed/retryable inference；
- existing Service/Stack version and update flows remain compatible。

### Operational validation

- run the focused Rust and Web suites after implementation；
- run shared testbox integration/Compose smoke only under the repository's heavy-validation policy；
- inspect database migration on a copy of representative old pending/history rows；
- verify no image push, GitHub Release publication or unrelated deployment side effect is introduced。

## Definition of Done

- [x] Candidate settlement contract is implemented and persisted.
- [x] SemVer waiting and unresolved fail-closed behavior are covered.
- [x] Regex/Glob raw-tag compatibility is covered.
- [x] Retry and periodic reconciliation are implemented.
- [x] Supersession and duplicate claim invariants are covered by the existing conditional pending claim.
- [x] Delay starts at discovery and policy changes are re-evaluated.
- [x] API/UI states are distinguishable from inference readiness.
- [x] Notification/history/API consume the same settlement.
- [x] Existing updater safety boundaries remain intact.
- [ ] Implementation-level validation and rollout evidence are complete. Local focused Rust checks and Clippy are complete; the shared-testbox Compose smoke still needs the existing metrics migration blocker resolved or explicitly waived before this item can be checked.
