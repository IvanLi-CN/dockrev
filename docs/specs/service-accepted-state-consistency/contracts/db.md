# 数据库（DB）

## Service accepted-state generation

- 范围（Scope）: internal
- 变更（Change）: Modify
- 影响表（Affected tables）: `services`, `job_service_targets`, `jobs`

### Schema delta（结构变更）

- `services.accepted_state_generation INTEGER NOT NULL DEFAULT 0`
- `job_service_targets.opened_generation INTEGER`
- `job_service_targets.baseline_snapshot_json TEXT`
- `baseline_snapshot_json` 使用带 `schemaVersion` 的 JSON object，完整保存 acquisition 时的以下列：
  - declaration: `image_ref`, `image_tag`
  - current: `current_digest`, `current_resolved_tag`, `current_runtime_started_at`, `current_resolved_tags_json`
  - candidate: `candidate_tag`, `candidate_resolved_tag`, `candidate_digest`, `candidate_arch_match`, `candidate_arch_json`
  - evaluation: `ignore_rule_id`, `ignore_reason`, `checked_at`
- 历史 `job_service_targets` 的新列保持 `NULL`；所有新 mutating Job target 必须同时写入非空 `opened_generation` 与 baseline。
- 现有 `(service_id, job_id)` 索引继续支持 recovery/ownership 查询，不新增全局锁表。

### Generation transitions（代际转换）

| 来源 | 前置状态 | 提交 | 后置状态 |
| --- | --- | --- | --- |
| check/runtime observation | `G` 为 expected 偶数 | 完整 accepted snapshot 与投影 | `G + 2` |
| mutation acquisition | `G` 为当前偶数 | Job、targets、baseline、ownership | `G + 1` |
| owning settlement | 当前值等于 target `opened_generation` 且为奇数 | 最终 accepted snapshot 与 Job terminal | `G + 1`，恢复偶数 |

任何条件更新影响行数与预期 target 数不一致时，事务必须回滚。generation 不允许在内存中先行推进或通过无条件 UPDATE 修正。

### Observation transaction（观测事务）

- Service CAS 的逻辑条件为：

```sql
WHERE id = :service_id
  AND accepted_state_generation = :expected_generation
  AND accepted_state_generation % 2 = 0
```

- 成功 UPDATE 必须同时执行 `accepted_state_generation = accepted_state_generation + 2`。
- Service snapshot、new-version notification reconciliation 和依赖该 snapshot 的投影必须在同事务提交；CAS 未命中时均不得变化。
- discovery token 包含事务外 Compose I/O 前读取的 Stack Service ID/name 集合与每项 generation。同步事务先验证数据库成员集合完全相同、所有 generation 与 token 相等且为偶数，再一次性更新 Stack metadata、增删/更新 incoming Services 与相关投影。
- discovery CAS 未命中返回 defer reason；不得把部分 Service 或临时 managed override 提交为新 baseline。

### Acquisition transaction（占用事务）

- 使用 `TransactionBehavior::Immediate`。
- 在一个事务中完成：解析完整 targets、既有冲突检查、验证所有 generation 为偶数、捕获 baseline、将 generation 增加 `1`、插入 Job、插入带 ownership 的 targets、插入初始 Job log。
- 多 target acquisition 是全有或全无；任一冲突、缺失 Service 或 generation 条件失败均不得留下 Job 或部分奇数 generation。
- `opened_generation` 保存 acquisition 后的奇数值。
- dry-run 不进入该事务的 generation/ownership 分支。

### Settlement transaction（结算事务）

- settlement 输入包含 Job ID、每个 target 的 `opened_generation` 和完整最终 accepted snapshot。
- 每个 Service UPDATE 必须匹配 ID、opened generation 和奇数状态，并把 generation 增加 `1`。
- 所有 Service settlement 成功后，才可在同事务写 Job terminal status、finished time、summary 与终态 log。
- 任一 target ownership 不匹配时整个事务回滚；调用方进入恢复/诊断路径，不发布 terminal management event。
- registry refresh deferred 只影响 candidate freshness metadata。baseline candidate 与最终 current 不同且没有更可信 registry 结果时，candidate 字段保持 baseline 值。

### Migration notes（迁移说明）

- 向后兼容窗口（Backward compatibility window）: 新列均为 additive；旧 Job target 可读但不能作为新 ownership。新二进制把现有 Service generation 初始化为稳定 `0`。
- 发布/上线步骤（Rollout steps）: 先应用 schema，再启用统一 acquisition/observer CAS/settlement；三者必须由同一应用版本交付，不能只启用奇数 ownership 而保留无条件 observer writes。
- 回滚策略（Rollback strategy）: 回滚到不识别 generation 的旧二进制前，必须确认不存在奇数 generation 或未结算 owning Job。附加列可以保留，不需要降级 DDL。
- 回填/数据迁移（Backfill / data migration）: 不为历史 Job 构造 baseline；现有 Service 从 generation `0` 开始，首次 accepted observation 或 mutation 自然推进。
