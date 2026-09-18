# Dockrev：自动更新候选收敛与发布边界主题历史

## Lifecycle / Compatibility

- 本主题补足版本推断、通知收敛与自动部署策略之间的生命周期缺口。
- 既有版本推断 worker、digest snapshot、通知记录、自动策略配置和 update job 合同继续有效；本主题只规定它们如何共享候选事实。
- 既有 <code>auto_update_pending</code> 保留为已命中规则后的延迟/入队动作记录，不再扩展为 inference waiting 或候选发现表。
- 既有手动检查不获得自动部署权限；自动部署来源仍限定为 schedule 与 GHCR webhook。
- 自动更新仍是服务部署动作，不改变镜像仓库发布或 GitHub Release 的职责边界。

## Replacements / Background

- “检查完成即完成自动策略评估”的隐含假设由 candidate settlement + policy re-evaluation 合同替代。
- “没有 pending 就没有候选”的隐含假设由独立 <code>auto_update_candidates</code> 生命周期替代。
- “事件是状态”的隐含假设由“数据库事实源、事件即时唤醒、启动/周期 reconciliation 补偿”替代。
- “延迟从 check 完成开始”的隐含假设由 candidate discoveredAt 起算替代。
- “候选展示版本变化就是新候选”的隐含假设由 service + digest identity 替代。
- hydration diagnostic 将 candidate missing 限定为真实缺失 candidate row；启动/周期 reconciliation 继续处理 awaiting inference candidate。
- 自动策略 job 的 current-digest 校验收敛到插入事务内；历史 pending 清理与运行时 claim 共用成功 Check、来源、creator 和 scope identity 门禁，避免 stale 或非 Check provenance 获得部署授权。
- inference settlement 改为 begin 阶段发放、settle 阶段精确匹配的 generation token；migration provenance 同时要求 candidate 与 pending 使用同一 source check job。

## Decision Trace

- floating tag 的 SemVer 规则必须等待可信 resolved version；raw <code>latest</code> 不得作为安全的 SemVer fallback。
- digest-bound OCI <code>org.opencontainers.image.version</code> 可以作为 SemVer fallback，但只能在 exact digest 证据边界内使用。
- unresolved 是可解释的版本证据终态，不是 update job 失败；Regex/Glob 仍可以使用 raw tag。
- 新 digest 会 supersede 旧 candidate；旧候选保留历史和版本滞后统计，但永远不能执行部署。
- inference settlement、通知、历史和 policy evaluator 共享同一份 canonical candidate settlement。
- 先提交 snapshot/settlement，再发布事件，保证事件丢失时仍能靠 reconciliation 恢复。

## References

- [SPEC.md](./SPEC.md)
- [IMPLEMENTATION.md](./IMPLEMENTATION.md)
- [0011-auto-update-candidate-settlement](../../adr/0011-auto-update-candidate-settlement.md)
