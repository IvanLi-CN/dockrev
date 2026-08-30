# Dockrev：Service Accepted State 一致性

> 当前有效规范以本文为准；实现覆盖与验证事实见 `./IMPLEMENTATION.md`，主题局部演进见 `./HISTORY.md`，持久架构取舍见关联 ADR。

## 背景 / 问题陈述

Service 快照同时被 update、rollback、discovery、check 和 runtime scan 改写。镜像更新会把临时 managed override 和 candidate container 暴露给并发观测；这些瞬时结果可能覆盖更新前的 candidate，或在自动回滚完成后才落库，使页面短暂或持续显示“没有新版本”。

Job 是否仍为 active 不能独立解决该问题：一个观测可以在更新前读取、在更新终态后写入。Service 快照需要独立于进程锁和 Job 活跃状态的持久并发合同。

## 目标 / 非目标

### Goals

- 将 Service 快照定义为 accepted deployment state，阻止候选容器和临时 override 成为权威状态。
- 用 Service 粒度的持久 generation CAS 拒绝迟到观测，同时允许无关 Service 和 Stack 并行工作。
- 让所有会改变 Service runtime 的受管操作在发布终态 Job 事件前完成统一 settlement。
- 在 registry 暂时不可用和进程重启时保留最后可信 candidate，并提供可恢复、可诊断的降级状态。

### Non-goals

- 不隐藏 operation progress、candidate health、rollback evidence 或生命周期事件中的瞬时事实。
- 不用全局锁串行化 Docker、registry 或 Compose 观测。
- 不把 Dockrev 之外的 Docker 操作归因给某个 Job；无受管 mutation 时，它们仍可由后续观测接纳。
- 不改变现有 update HTTP 请求或页面交互合同。

## 范围（Scope）

### In scope

- `services` accepted-state generation、`job_service_targets` ownership/baseline 和对应数据库迁移。
- check、runtime scan、discovery 的 generation-aware 持久化。
- update、manual rollback、service/stack lifecycle、managed-override reconcile，以及会 stop/restart Service 的 backup。
- 所有终态结果的统一 settlement、candidate 降级规则、启动恢复、Job 日志与 summary 诊断。

### Out of scope

- Supervisor 自升级状态、资源采样值和外部 Docker 事件的操作归因。
- 用 generation 替代现有 Job 冲突判定、managed-override 文件锁或前端 request generation。

## Related ADRs

- [Fence Accepted Service State With Generations](../../adr/0003-service-accepted-state-generation.md)

## 需求（Requirements）

### MUST

- `services.accepted_state_generation` 必须是非负单调整数。偶数表示可被观测更新的稳定 accepted state；奇数表示一个 mutating operation 持有 ownership。
- 接受一个 mutating Job 时，冲突检查、Job 与 target 插入、每个 target 的完整 baseline 保存，以及 Service 从偶数 generation 到下一奇数 generation 的占用必须在同一个 SQLite `IMMEDIATE` 事务中全有或全无地完成。
- `job_service_targets` 必须保存 `opened_generation` 和 versioned `baseline_snapshot_json`。baseline 至少包含 `image_ref`、`image_tag`、current runtime/digest/tag 集合、candidate tag/digest/arch、ignore 与 check 时间字段；历史 target 行允许为空，新操作不得为空。
- dry-run 和纯观测 Job 不得取得 mutation ownership。
- check 与 runtime scan 必须携带读取时的 generation。写入条件必须同时满足 Service ID、generation 相等且 generation 为偶数；成功写入必须把 generation 增加 `2`，CAS 未命中不得修改 Service、candidate 相关通知或派生快照。
- discovery 必须在外部 Compose 读取前取得 Stack observation token，包含既有 Service 身份集合与 generation。同步事务必须验证成员集合未变化、所有 generation 相等且为偶数；任一不满足时必须推迟整个 Stack，不得改写 `compose_files_json`、增删 Service 或部分更新 Service。
- update、rollback、lifecycle、managed-override reconcile 和会 stop/restart Service 的 backup 必须通过统一的 ownership 接口进入；任何 runtime mutation 不得绕过该接口。
- `success`、`rolled_back`、`failed` 和 `cancelled` 必须进入统一 settlement。accepted state 写入、奇数 generation 关闭为下一偶数，以及 Job 终态写入必须属于同一个 `IMMEDIATE` 事务；终态 management event 只能在该事务提交后发布。
- settlement 必须验证 Job、Service target 和 `opened_generation` 的所有权。所有权不匹配时不得覆盖 Service，也不得静默完成 Job。
- registry 成功时可以刷新 candidate；registry 不可用时必须保留 baseline 中仍不同于最终 current digest 的最后可信 candidate，并将 candidate refresh 标记为 deferred。registry 失败本身不得让 update Job 长期停留在运行态。
- apply 前失败可以直接以 baseline settlement。已产生 runtime 副作用的结果必须使用 updater 的明确结果和最终 runtime inspection；无法建立最终 runtime 事实时保持 generation fenced，并进入可重试恢复，不能猜测后关闭 ownership。
- 启动恢复必须先恢复 managed override/runtime 并 settlement，再发布中断 Job 的终态。不得先把 incomplete Job 设为终态后才恢复 override。

### SHOULD

- CAS 拒绝应返回结构化 `applied | deferred_mutation | stale_generation | membership_changed` 结果，并在 Job summary/log 中记录计数、Service/Stack 和 expected/actual generation，不作为观测 Job 的系统错误。
- 对 accepted state 有依赖的新版本通知、digest-tag snapshot 清理和其他投影应与 accepted-state CAS 同事务提交，或以提交后的新 generation 作为显式前置条件。
- 奇数 generation 没有可恢复 owning Job 时应产生明确诊断并保持 fenced，由恢复流程处理，不能自动重置为偶数。

## 功能与行为规格（Functional/Behavior Spec）

### Observation flow

1. check/runtime scan 读取 Service 与偶数 generation，执行 Docker/registry I/O。
2. 持久化以读取 generation 做 CAS。成功时一次写入完整 accepted snapshot 并将 generation 增加 `2`；失败时丢弃该结果，后续调度重新观测。
3. discovery 读取 Stack 成员和 generation token，解析 Compose，然后在一个事务中验证 token 并同步整个 Stack；临时 managed override 或迟到结果不能进入 accepted declaration。

### Mutation flow

1. Job 接受事务验证目标无冲突且 generation 为偶数，保存 baseline，将每个目标 generation 增加 `1` 为奇数。
2. Operation 可以继续发布进度、候选健康和生命周期证据，但普通观测不能改写 accepted state。
3. Operation 结束后构造每个 Service 的 settlement：最终 declaration、current runtime、candidate 与 refresh 状态。
4. Settlement 事务验证 opened generation，写 accepted state，将 generation 增加 `1` 为偶数，并写 Job 终态。事务提交后才发布 management event。

### Candidate settlement

- 成功接纳目标 digest 时，目标成为 current；只有 registry 已确认存在更新的情况下才写入新的 candidate。
- 自动或手动回滚成功时，baseline current 恢复为 current，baseline candidate 若仍与 current 不同则保留。
- apply 前失败或取消时恢复 baseline；产生副作用后失败或取消时以明确 runtime 结果为准。
- registry 查询失败时保留仍有效的 baseline candidate，记录 deferred refresh；不得用 `NULL` 表达“registry 未知”。

### Recovery flow

1. 启动时枚举奇数 generation 及其 owning Job target，恢复/清理该 Job 的 managed override 和其他副作用。
2. 能确认最终 runtime 时执行正常 settlement，并原子终结 Job。
3. Docker/runtime 事实仍不可用时保留奇数 generation 和恢复诊断，稍后重试；read-only API 可以继续返回最后 accepted baseline，但不得接受新观测写入。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | Owner | 使用方 | 备注 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| accepted-state persistence | database | internal | Modify | [Database contract](./contracts/db.md) | dockrev-api DB | observers, mutations, recovery | generation, ownership and baseline |
| Service observation CAS | Rust DB API | internal | Modify | [Database contract](./contracts/db.md) | dockrev-api DB | check, runtime scan | advances even generation by two |
| Stack observation CAS | Rust DB API | internal | New | [Database contract](./contracts/db.md) | dockrev-api DB | discovery | all-or-nothing membership and generation validation |
| mutation acquire/settle | Rust DB API | internal | New | [Database contract](./contracts/db.md) | dockrev-api operations | all runtime mutations | atomic ownership and terminal settlement |

### 契约文档（按 Kind 拆分）

- [Database contract](./contracts/db.md)

## 验收标准（Acceptance Criteria）

- Given healthcheck update 已启动 candidate，When discovery 读取到临时 managed override 且 runtime scan 观察到 candidate，Then 两个观测都不能改写 accepted state；自动回滚终态为 `current=baseline old`、`candidate=target new`、声明镜像保持原值。
- Given 一个观测在 generation `10` 读取，When mutation 经过 `11` 并 settlement 为 `12` 后该观测才落库，Then CAS 拒绝该观测，settlement 不被覆盖。
- Given 两个观测都从同一偶数 generation 读取，When 一个先成功提交并把 generation 增加 `2`，Then 另一个不能再提交。
- Given Stack 中任一 Service generation 为奇数、已变化或成员集合已变化，When discovery 同步，Then 整个 Stack 被推迟，Compose metadata 和所有 Service 均不改变。
- Given mutation targets 多个 Service，When 任一 target 有冲突或无法占用，Then Job、targets、baseline 和所有 generation 都不发生部分提交。
- Given update 自动回滚且 registry 不可用，When Job 进入终态，Then candidate 保留、refresh 标记 deferred，spinner 可以停止。
- Given mutation 成功、回滚、失败或取消，When 终态 management event 发布，Then 所有 target 均已 settlement 且 generation 为偶数。
- Given 进程在 generation 为奇数时退出，When 启动恢复完成，Then 先恢复 runtime/override 并 settlement，再发布终态；事实不足时保持 fenced 且有可重试诊断。
- Given 两个无交集 Service 同时运行 mutation 或 observer，When 执行各自事务，Then 二者不因全局 Service 状态锁相互阻塞。

## 验收清单（Acceptance checklist）

- [x] accepted state、瞬时观测与 mutation ownership 的长期行为已明确。
- [x] 迟到写、registry 降级、取消、崩溃恢复和 orphan ownership 已覆盖。
- [x] 数据库与内部事务接口已写入独立契约。
- [x] 每项验收条件均可映射到确定性的实现测试。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- 第一条红灯必须扩展现有 healthcheck rollback API 集成 seam，真实调用 update、discovery 和 runtime scan，并用 runner gate 固定并发顺序。
- DB 测试覆盖偶数观测 `+2`、mutation `+1/+1`、迟到 CAS、owner 校验、多 target 原子性、无关 Service 并行和历史空 baseline 兼容。
- discovery 测试覆盖临时 override、成员变化与单 Service generation 变化下的整 Stack defer。
- transition 测试覆盖四类终态、registry deferred candidate、apply 前失败和产生副作用后的失败。
- recovery 测试覆盖奇数 generation、遗留 override、Docker 暂不可用和 orphan ownership 诊断。
- adapter 测试覆盖 update、rollback、service/stack lifecycle、managed-override reconcile 和 stop-related backup 均通过统一 ownership 接口。

### Quality checks

- `cargo test -p dockrev-api api::tests::update_apply_healthcheck_rollback_preserves_candidate_across_concurrent_observers -- --exact --nocapture`
- `cargo test -p dockrev-api service_operation`
- `cargo test -p dockrev-api runtime_scan`
- `cargo test -p dockrev-api discovery`
- `cargo test -p dockrev-api update_ --no-fail-fast`
- `cargo fmt --all --check`
- `cargo clippy -p dockrev-api --all-targets -- -D warnings`
- 需要真实 Compose/Docker 的集成验证仅通过共享测试机执行。

## 风险 / 假设

- 风险：多个 accepted-state 投影若不与 CAS 同步，仍可能产生快照与通知不一致；实现清单必须逐一枚举写入者。
- 风险：queued mutation 从接受时持有奇数 generation，长队列会延后观测写入；Job 进度和 accepted baseline 仍可读，取消路径必须 settlement。
- 风险：Stack discovery 的全有或全无 defer 会延迟无关 Service 的配置发现，但避免 Compose 与 Service 部分同步；下一次扫描可自然恢复。
- 假设：SQLite `IMMEDIATE` 事务仍是 Job 接受和 settlement 的唯一数据库一致性边界。

## Related PRs

- None

## 参考（References）

- `CONTEXT.md`
- `docs/specs/uupfm-update-status-settle-after-finish/SPEC.md`
- `docs/specs/update-rollback-diagnostics/SPEC.md`
- `docs/specs/service-lifecycle-observability/SPEC.md`
