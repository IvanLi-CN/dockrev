# Dockrev：自动更新候选收敛与发布边界实现准备

## Current Status

- Implementation: complete
- Lifecycle: active
- 已实现候选事实表、幂等 settlement、digest-bound SemVer 门禁、策略重评估、重试/周期 reconciliation 和兼容 API/UI 投影；迁移对不可证明历史采用 fail-closed 处理。
- 自动策略 update job 先持久化为 queued，再通过候选有效性 CAS 转为 running；候选替代会取消尚未启动的 queued job，并在 apply 尚未提交时请求停止已运行 job。
- 候选来源以结构化 provenance 保存为 schedule、github_webhook 或 unknown；unknown 历史只保留审计，不获得自动部署授权。
- Validation: local checks complete; the shared-testbox Compose smoke is partially blocked by the existing metrics migration error <code>retained rollups cannot be recovered after raw retention</code> during the Compose V1 rejection setup. The V2 plugin and standalone lifecycle portions passed before that blocker.

## 实现顺序

### M1：候选数据模型与数据库不变量

- 新增 <code>auto_update_candidates</code> 表、索引和迁移。
- 固定 <code>serviceId + candidateDigest</code> 的候选唯一键。
- 保存 discovery provenance、discoveredAt、raw tag、current digest、settlement、retry 和 supersession。
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
- UI 区分 inference waiting、unresolved、rule not matched、delayed、queued/running/completed。
- SemVer preview 与后端严格解析语义一致。
- 通知和历史读取 canonical settlement，并按 service + digest 去重。
- 保持原始 job summary 不可变，把 settlement 作为独立事实展示。

### M6：迁移与有限补偿

- 从 active pending、可证明的发现记录和现有 snapshot 建立 candidate 初始状态。
- 对来源可证明为 schedule/GHCR webhook 且有 discoveredAt、仍是最新 digest 的候选执行一次有限 reconciliation。
- 迁移恢复严格 SemVer 或 digest-bound display evidence；floating/不可解析版本保持等待或 unresolved，只有当前服务 candidate digest 才能保留 active authorization。
- 来源不明、时间缺失、已 superseded 或历史终态记录不自动补发 update job。
- 迁移脚本必须可重复执行，且不能把历史通知直接转换成新的自动部署授权。

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
- [ ] Implementation-level validation and rollout evidence are complete. Local Rust tests and Clippy are complete; the shared-testbox Compose smoke remains blocked by the metrics migration error recorded above.
