# Dockrev：自动更新候选收敛与发布边界

## 状态

- Status: 已完成
- Lifecycle: active
- 本主题冻结领域合同、数据边界、验收标准与实现结果。

## Visual Evidence

- Desktop state gallery: [auto-update-policy-state-gallery-desktop.png](assets/auto-update-policy-state-gallery-desktop.png)
- Mobile state gallery: [auto-update-policy-state-gallery-mobile.png](assets/auto-update-policy-state-gallery-mobile.png)
- Ambiguous-history desktop: [auto-update-policy-ambiguous-history-desktop.png](assets/auto-update-policy-ambiguous-history-desktop.png)
- Ambiguous-history mobile: [auto-update-policy-ambiguous-history-mobile.png](assets/auto-update-policy-ambiguous-history-mobile.png)
- Stack detail awaiting-inference desktop: [stack-detail-policy-awaiting-desktop.png](assets/stack-detail-policy-awaiting-desktop.png)
- Stack detail awaiting-inference mobile: [stack-detail-policy-awaiting-mobile.png](assets/stack-detail-policy-awaiting-mobile.png)
- 覆盖候选等待、版本可用、版本未解析、规则未命中、延迟、排队、执行中、完成和跳过等策略动作状态；ambiguous-history 状态明确显示 provenance 不完整、候选 unresolved 且禁止自动部署。

## Related ADRs

- [Automatic Update Candidate Settlement](../../adr/0011-auto-update-candidate-settlement.md)

## 背景 / 问题陈述

Dockrev 目前把自动更新策略评估挂在“检查任务完成时提取出的新版本通知数据”上。这个入口对语义版本标签有效，但对 <code>latest</code>、<code>stable</code>、<code>main</code> 等 floating tag 存在断链：

1. 检查任务先发现候选 digest，版本推断由独立 worker 异步执行。
2. 自动策略的 SemVer matcher 在推断完成前只能看到 raw tag。<code>latest</code> 不能被解析成 SemVer，于是规则返回“不匹配”。
3. 版本推断完成后，现有完成事件主要用于刷新 snapshot、通知或页面展示，没有可靠地重新执行同一个候选的自动策略评估。
4. 现有 <code>auto_update_pending</code> 只在规则已经命中后创建，因此它无法表示“候选存在，但正在等待版本推断”。

所以截图中的“推断未完成”本身是异步架构下的正常中间状态；不正常的是它完成后没有回到策略评估链路，最终导致候选既没有进入延迟队列，也没有创建 update job。

这里还存在一个术语边界：Dockrev 的自动更新是把服务切换到候选镜像 digest 的部署动作，不是向镜像仓库 push 镜像，也不是发布 GitHub Release。版本推断完成只能说明“候选的版本证据已经收敛”，不能说明“服务已经更新”，更不能说明“镜像已经发布”。

## 目标 / 非目标

### Goals

- 为每个服务的候选 digest 建立独立、可持久化、可恢复的候选生命周期。
- 将 raw tag、digest 证据、resolved version、推断状态与失败原因收敛成一个 canonical candidate settlement。
- 明确区分候选状态与策略动作状态，避免把 inference pending 误认为 update pending。
- SemVer 在 resolved version 未准备好时等待并重新评估；Regex/Glob 保留对 raw tag 的直接匹配能力。
- 推断完成、事件丢失、进程重启或策略变化后，都能通过事件或 reconciliation 继续完成策略评估。
- 保留现有 digest lock、同 tag 保护、备份继承、服务/Stack 并发保护和 Dockrev 自身升级保护。
- 为 API、UI、通知、历史记录提供同一份候选结论。

### Non-goals

- 不实现镜像构建、镜像仓库 push 或 GitHub Release publication。
- 不改变“候选必须是当前 Compose tag 对应的新 digest”的选择语义。
- 不让 UI 手动检查、dry-run 或 preview 触发自动部署。
- 不引入跨实例分布式协调；当前仍以单实例拓扑为范围。
- 不通过把 <code>latest</code> 猜成某个版本来绕过 SemVer 安全边界。
- 不把旧的历史记录批量重放成不可预期的自动部署。

## 当前缺陷清单

### D1：SemVer matcher 读取了错误的输入层

现有 matcher 会依次尝试候选展示标签和 raw candidate tag。对 floating tag，展示标签在推断完成前通常也是 <code>latest</code>，因此 SemVer 解析失败。这个失败被当成“规则未命中”，而不是“等待版本证据”。

### D2：推断终态没有触发策略重评估

检查完成和版本推断完成是两个异步阶段。现有自动策略入口只保证第一个阶段触发评估，第二个阶段没有持久化的候选续接点，导致 <code>latest -&gt; 1.2.3</code> 后仍停留在未命中。

### D3：<code>auto_update_pending</code> 承载了过多语义

它同时被用来表达延迟门槛、入队状态和候选存在后的后续动作，却不能表达 inference waiting、unresolved 或 superseded。于是“没有 pending 行”无法区分“没有候选”和“候选尚未达到可匹配状态”。

### D4：运行时事件不是事实源

<code>task_finished</code> 或进程内通知可以及时唤醒消费者，但事件可能在重启或订阅竞态中丢失。若数据库没有保存候选 settlement 和下一次重评估所需的数据，系统就无法自行恢复。

### D5：旧候选与新 digest 的边界不够早

当 floating tag 连续指向多个 digest 时，旧延迟记录可能在新候选出现后继续等待到期，最后只能依赖入队前的 digest 比较被动跳过。候选替代应在发现新 digest 时显式发生。

### D6：延迟起点依赖检查完成而不是候选发现

推断耗时会把延迟计时人为推后；重启或事件丢失还可能造成重复计算。延迟策略应从首次接受该 digest 为候选的时间开始。

### D7：来源、幂等和通知身份没有共用一个候选键

自动部署只允许由 schedule 或 GHCR webhook 检查触发。若来源从 summary 推断，或通知、pending、候选分别使用不同的去重键，就会出现重复评估、漏评估或展示版本变化造成重复通知。

### D8：前端把“未解析”展示成“未命中”或普通 loading

用户无法判断是版本推断仍在进行、推断已失败、规则确实未命中，还是更新任务已在运行。状态命名不清也会让“完成后为什么没有发布”的判断失真。

## 核心概念

完整术语定义见 [CONTEXT.md](../../../CONTEXT.md)。本主题特别固定以下两个正交维度：

### Candidate settlement

候选 settlement 只回答“这个 service + digest 代表什么、是否已经有足够版本证据、还是否有效”：

| 状态 | 含义 | SemVer 自动策略 | Regex/Glob 自动策略 |
| --- | --- | --- | --- |
| <code>awaiting_inference</code> | floating tag 的 digest 尚未完成版本推断 | 等待，不视为未命中 | 可直接使用 raw tag 评估 |
| <code>ready</code> | 需要的版本证据已可用 | 使用 resolved version | 使用 resolved display tag，并保留 raw tag 回退 |
| <code>unresolved</code> | 推断达到终态但没有可信 SemVer | 不自动部署 | 仍可使用 raw tag |
| <code>superseded</code> | 已有更新的有效候选 digest | 永不执行 | 永不执行 |

<code>awaiting_inference</code> 是正常中间状态；<code>unresolved</code> 是需要可解释原因的终态；二者都不是 update job 失败。

### Policy action

策略动作只回答“当前有效策略对这个候选做了什么”：

| 状态 | 含义 |
| --- | --- |
| <code>waiting_inference</code> | 当前规则需要 resolved version，候选尚未收敛 |
| <code>rule_not_matched</code> | 规则输入已满足，但没有 enabled rule 命中 |
| <code>delayed</code> | 规则命中，正在等待时间和版本滞后门槛 |
| <code>queued</code> | 已通过 claim，已创建自动 update job |
| <code>running</code> | 对应 update job 正在执行 |
| <code>completed</code> | update job 已完成并完成服务状态 settlement |
| <code>skipped</code> | 因策略关闭、候选替代、服务保护或其他明确原因不执行 |

一个候选可以是 <code>awaiting_inference</code>，同时其 Regex/Glob policy action 已经是 <code>queued</code>；这两个维度不能合并成一个 <code>pending</code> 字段。

## 候选 Settlement 合同

### 候选身份

- 候选的稳定通知身份是 <code>serviceId + candidateDigest</code>。
- 同一个 digest 因 raw tag 或 display tag 变化而重新展示时，不创建第二个候选，也不重复发送新候选通知。
- 候选记录保留首次发现时间、最近一次可证明的来源、raw tag、当前服务基线和 digest 证据。
- 新 digest 被接受为当前有效候选时，旧的同服务有效候选立即转为 <code>superseded</code>。旧记录保留用于历史和版本滞后统计，但不能 claim 或执行部署。

### 版本证据优先级

对需要 SemVer 的策略，resolved version 必须来自与 candidate digest 绑定的证据，优先级为：

1. raw candidate tag 本身是严格 SemVer 时，直接使用该 raw tag。
2. 针对 exact digest 的 registry tag snapshot 中的有效 SemVer tag，选择确定性的最高版本，并保留全部命中 tag。
3. exact digest 对应镜像的 OCI label <code>org.opencontainers.image.version</code>，使用共享的 SemVer 规整规则：允许前导 <code>v</code>/<code>V</code>，拒绝 build metadata。
4. 以上证据都不可用时，settlement 为 <code>unresolved</code>，并保存稳定的失败原因。

仅有一个没有 digest 绑定关系的字符串 <code>resolvedTag</code>，不能证明该 candidate digest 的版本。不能因为同一仓库的另一个 tag、当前 floating tag 的历史值或镜像排序结果看起来像版本，就把它当成 SemVer 证据。

### Matcher 输入

| Matcher | 输入 | 推断未完成时 | 推断终态失败时 |
| --- | --- | --- | --- |
| <code>semver</code> | resolved version | <code>waiting_inference</code>，禁止 raw fallback | <code>skipped</code>，原因 <code>version_unresolved</code> |
| <code>regex</code> | resolved display tag，然后 raw tag | 可用 raw tag 直接匹配 | 可用 raw tag 继续匹配 |
| <code>glob</code> | resolved display tag，然后 raw tag | 可用 raw tag 直接匹配 | 可用 raw tag 继续匹配 |

SemVer 的“等待”不是失败；将 raw <code>latest</code> 强制转换成任意版本则是不安全的 fallback。Regex/Glob 对 raw tag 的兼容是已有策略能力，不应被 inference worker 的失败阻断。

## 推断与重试

- 首次发现 floating candidate 时创建推断任务，并立即持久化 <code>awaiting_inference</code> settlement。
- 首次推断失败后最多自动重试三次，退避为 <code>1m -&gt; 5m -&gt; 10m</code>。
- 网络超时、registry 暂时不可达、worker 暂时异常属于可重试失败。
- 已完成的证据扫描明确表明没有有效 SemVer，或 OCI label 明确不可解析，属于终态失败；不通过无限轮询解决。
- 达到重试上限后持久化 <code>unresolved</code>、<code>reason</code>、<code>attempts</code> 和 <code>nextRetryAt</code>（终态时可为空）。
- force inference 或新的、来源合格的 schedule/GHCR webhook check 可以重新打开一轮推断；新一轮不删除旧尝试记录。
- 推断失败不应改变 check job 的成功与否，也不应让已发现的 digest 消失。

## 预期流程

~~~text
schedule / GHCR webhook check
        |
        v
发现当前 tag 对应的新 digest
        |
        +--> 持久化 candidate(discoveredAt, source, rawTag, digest)
        |          |
        |          +--> strict semver raw tag: ready
        |          |
        |          +--> floating tag: awaiting_inference + enqueue inference
        |
        v
策略评估
  semver: ready 才匹配；awaiting 等待；unresolved 跳过
  regex/glob: resolved tag 优先，raw tag 可直接匹配
        |
        +--> 未命中: rule_not_matched
        +--> 命中 immediate: claim -> queued -> update job
        +--> 命中 delayed: delayed -> 时间门槛 AND 版本滞后门槛
                                      |
                                      v
                              claim -> queued -> running
                                      |
                                      v
                         update job terminal settlement
                                      |
                                      v
                            服务接受新 digest
~~~

版本推断分支独立于 check 主链路：

~~~text
inference worker
      |
      +--> snapshot / OCI evidence committed
      +--> candidate settlement committed
      +--> settlement event published
                                  |
                                  v
                       policy re-evaluation
~~~

数据库提交顺序固定为：先提交 snapshot 与 candidate settlement，再发布 settlement event。事件只是即时唤醒信号，数据库是事实源。

## 持久化模型

### <code>auto_update_candidates</code>

新增独立候选生命周期记录，至少包含：

- candidate identity：<code>id</code>、<code>serviceId</code>、<code>stackId</code>、<code>imageRef</code>、<code>candidateDigest</code>；
- observation：<code>rawTag</code>、<code>currentDigest</code>、<code>currentTag</code>、<code>discoveredAt</code>、<code>source</code>、<code>sourceCheckJobId</code> 或 webhook identity；
- settlement：<code>status</code>、<code>resolvedVersion</code>、<code>resolvedTags</code>、<code>settledAt</code>、<code>reason</code>；
- retry：<code>attempts</code>、<code>nextRetryAt</code>、<code>lastError</code>；
- supersession：<code>supersededAt</code>、<code>supersededByCandidateId</code>；
- latest policy projection：<code>policyStatus</code>、<code>policyReason</code>、<code>policyScope</code>、<code>ruleId</code>、<code>policyEvaluatedAt</code>、<code>updateJobId</code>。

候选身份应有数据库唯一约束，至少保证同一 service + digest 只有一条当前生命周期记录。候选 settlement 的写入必须幂等，重复 worker 完成事件不能产生第二个候选或覆盖较新的 digest。

### <code>auto_update_pending</code>

保留现有表用于已命中策略的 delayed/claim/enqueue 动作，并增加到 candidate identity 的明确关联。它不再表示 inference waiting，也不承担候选发现事实。

- 只有 policy action 已命中且需要延迟或等待入队时才创建。
- <code>firstSeenAt</code> 改为 candidate <code>discoveredAt</code>，不能使用 inference completion 或 check completion 作为延迟起点。
- 旧 pending 在 candidate 被 superseded、策略失效或服务不再符合条件时必须显式转为 skipped，并保留原因。
- active pending 的唯一性至少覆盖 service、candidate digest、effective policy scope、rule；重复事件只能复用现有动作。

### 历史、通知与快照

- 原始 job summary 保持不可变，candidate settlement 作为独立事实保存 resolved version 和失败原因。
- 通知、历史列表、策略评估和 API 读取同一份 canonical settlement；不得各自从部分 summary 猜版本。
- 通知按 <code>serviceId + candidateDigest</code> 去重。raw tag 到 resolved version 的展示变化不产生第二个 candidate identity。
- 新候选可以计入历史版本滞后；只有最新未 superseded candidate 可以执行自动部署。

## 策略重评估与 Claim

### 触发来源

自动策略只接受以下来源：

- schedule check；
- GHCR webhook 命中的 service check。

来源必须作为结构化字段持久化或可靠传入，不能仅通过“是否产生 new version notification”反推。UI 手动 check、preview、dry-run 和其他来源只更新发现事实，不创建自动 update job。

### 重评估规则

每次以下事件发生时，对仍有效的 candidate 使用当前 effective policy 重评估：

- candidate 首次发现；
- inference settlement 成功或终态失败；
- 新 digest 取代旧 candidate；
- schedule/GHCR webhook 对相同 digest 产生新的合格观察；
- policy 保存、服务/Stack 继承关系变化；
- 服务从 update/lifecycle/backup 冲突中恢复；
- 启动或周期 reconciliation。

重评估使用当前有效的 Service/Stack policy。若策略改变，旧 pending 不继续代表旧规则；candidate 仍可按新的原始 <code>discoveredAt</code> 重新计算延迟。

### Claim 不变量

创建 update job 前必须在同一受保护的状态转换中重新确认：

- candidate 仍是 service 当前最新 digest；
- candidate 不是 <code>superseded</code>；
- effective policy 仍启用且 rule 仍匹配；
- 若为 SemVer，resolved version 仍是 exact digest 证据；
- service、Stack、全局更新和 Dockrev 自身保护均允许执行；
- 没有另一个 active mutating operation；
- 本 candidate 没有已接受的自动 update job。

claim 失败只能释放本次 claim 或写入明确的 skipped/retryable 状态，不能直接执行 Compose side effect。

## 重启、事件丢失与候选替代

- 启动时扫描所有非终态 candidate、到期 retry 和 active pending，执行一次 reconciliation。
- 周期 reconciliation 再次比较 service 当前 digest、snapshot/inference 状态、候选状态和 effective policy；它用于补偿事件丢失，不重放已经 superseded 的历史。
- 收到 <code>task_finished</code> 时可以立即唤醒相关 candidate，但处理前必须从数据库重新读取 settlement。
- 新 digest 到来时，旧 candidate 和其 pending action 先标记 superseded/skipped，再让新 candidate 进入策略评估。
- 单实例范围内使用数据库条件更新和现有 operation lock 防止重复 claim；本主题不承诺多实例全局唯一。

修复上线时由 <code>service_new_version_discoveries</code> 与成功 check job provenance 回填当前 service digest 缺失的 candidate。只接受 type 为 check、status 为 success 且来源可证明为 schedule 或 GHCR webhook 的最早可信观察；它保存原始 <code>sourceJobId</code>、<code>source</code>、<code>discoveredAt</code> 和当前 baseline。来源不明、image/baseline/source job/time 不完整或已被替代的历史记录只能形成 <code>unresolved</code> 的 <code>migration_ambiguous_history</code> 审计事实，不能获得自动部署授权。

迁移与启动/周期 reconciliation 复用同一 hydration helper。hydration 只创建 candidate fact、执行旧候选 supersession 和 pending 失效，不执行 Compose side effect；回填后仍须由当前 service digest、effective policy、SemVer evidence 与 operation protection 重新校验后才能 enqueue。

## API / UI 合同

现有接口保持兼容，在 service/candidate 结构中增加可选状态字段：

~~~json
{
  "candidateSettlement": {
    "status": "awaiting_inference",
    "rawTag": "latest",
    "candidateDigest": "sha256:...",
    "resolvedVersion": null,
    "resolvedTags": null,
    "reason": "inference_running",
    "attempts": 1,
    "retryAt": null,
    "discoveredAt": "...",
    "source": "github_webhook",
    "sourceJobId": "check-...",
    "hydrationOrigin": "discovery_history"
  },
  "candidateHydration": {
    "status": "hydrated",
    "reason": null,
    "candidateDigest": "sha256:...",
    "source": "github_webhook",
    "sourceJobId": "check-...",
    "hydrationOrigin": "discovery_history",
    "discoveredAt": "..."
  },
  "autoUpdate": {
    "policyStatus": "waiting_inference",
    "reason": "semver_requires_resolved_version",
    "ruleId": null,
    "evaluatedAt": "..."
  }
}
~~~

对已有 <code>versionInference</code> 字段：

- 保留现有 <code>pending</code>/<code>ready</code> 兼容语义；
- 增加可选 <code>reason</code>、<code>retryAt</code>、<code>resolvedTag</code> 和 <code>unresolved</code> 终态表达；
- 不把 <code>versionInference.status=ready</code> 解释成 update job 已完成。

UI 至少表达以下不同状态：

- “等待版本解析”：候选已发现，SemVer 策略暂不评估；
- “版本解析失败”：显示原因、尝试次数和可用的重试时间；
- “规则未命中”：版本输入已齐全，但当前规则不匹配；
- “等待延迟”：规则命中，显示时间门槛和版本滞后门槛；
- “已排队/执行中/已完成”：对应真实 update job 状态；
- “候选已被替代”：只读历史状态。
- “历史回填不完整”：显示 <code>ambiguous_history</code> 与 <code>migration_ambiguous_history</code>，明确不可自动部署；发现历史存在但 candidate 缺失时显示 <code>candidate_missing</code>。

前端 SemVer 预览必须复用后端相同的严格解析规则：没有 resolved version 时显示“不确定/等待解析”，不能显示“确定未命中”。Regex/Glob 预览可明确展示 raw tag 的匹配结果。

## 验收标准

- Given 服务使用 <code>latest</code> 且发现新 digest，When check 成功，Then candidate 被持久化为 <code>awaiting_inference</code>，check 不因推断未完成而失败。
- Given SemVer policy 需要 resolved version，When candidate 仍为 <code>awaiting_inference</code>，Then policy action 为 <code>waiting_inference</code>，不创建 <code>auto_update_pending</code>，不创建 update job。
- Given inference 返回 digest-bound resolved version，When settlement 提交完成，Then 同一个 candidate 变为 <code>ready</code>，并自动触发一次策略重评估。
- Given inference 完成事件丢失或进程在 settlement 后重启，When reconciliation 运行，Then candidate 仍被重评估，不依赖事件补发。
- Given inference 达到终态但无可信版本，When SemVer policy 重评估，Then action 为 <code>skipped</code> 且原因是 <code>version_unresolved</code>，不能把 <code>latest</code> 当版本部署。
- Given inference 失败，When存在 Regex/Glob policy，Then raw tag 仍可匹配并按策略执行。
- Given inference transient failure，When重试任务运行，Then最多执行三次自动重试，退避依次为 1 分钟、5 分钟、10 分钟，最终写入 unresolved 或 ready。
- Given同一 candidate 收到重复 settlement event，When policy re-evaluation 执行，Then不会生成重复 candidate、重复 pending 或重复 update job。
- Given新 digest 被发现，When旧 candidate 仍处于 delayed 或 queued，Then旧 candidate 变为 superseded/skipped，只有新 digest 可 claim。
- Given delayed policy，When inference耗时或进程重启，Then时间门槛从 candidate discoveredAt 计算，且必须与版本滞后门槛同时满足。
- Given policy 在 candidate 等待期间变更，When reconciliation 执行，Then使用新 effective policy，不继续执行旧 rule。
- Given来源是 UI 手动 check，When规则匹配，Then只记录候选，不创建自动 update job。
- Given update job 完成，When服务状态 settlement 成功，Then UI 才显示自动更新完成；inference 完成不会被显示为部署完成或镜像发布。
- Given API 返回未解析状态，When用户查看 Service/Stack，Then能区分 inference waiting、unresolved、rule not matched、delayed 和 update running。
- Given discovery history 存在但 candidate row 缺失，When migration 或启动/周期 reconciliation 运行，Then只为当前 service digest 创建唯一 candidate，保留首次 discoveredAt 与合格 source provenance，并继续经过 policy evaluator。
- Given discovery history 缺少 image、baseline、source job 或合格成功 check，When hydration 运行，Then candidate 为 unresolved、reason 为 <code>migration_ambiguous_history</code>、source 不合格且不会创建 update job。

## 非功能性验收 / 质量门槛

### Backend tests

- candidate settlement：strict semver、floating pending、snapshot evidence、OCI fallback、unresolved；
- matcher：SemVer 禁止 raw fallback，Regex/Glob 允许 raw fallback；
- retry：退避、上限、force/new qualified check reopen；
- event/restart：settlement 先提交、重复事件、事件丢失、启动 reconciliation；
- candidate identity：重复 digest 去重、新 digest supersede、旧 pending 不可执行；
- hydration：成功 schedule/webhook discovery 回填、重复 hydration、最早可信观察、ambiguous history fail-closed 和 migration copy；
- policy：当前策略重评估、来源门控、延迟起点、版本滞后、claim 条件；
- update：显式 digest target、正常 updater 保护、终态服务状态 settlement。

### API / Web tests

- API 可选字段与旧 payload 兼容；
- candidate source/hydration origin 与 candidateHydration diagnostic 在 Service/Stack/Overview 可观察，旧客户端可忽略；
- Service/Stack 页面区分五类推断/策略/更新状态；
- SemVer preview 在无 resolved version 时为不确定而非未命中；
- 通知、历史与 API 使用同一 candidate settlement；
- 重复事件不会重复通知或重复入队。

### Operational checks

- 启动 reconciliation 能恢复 waiting、retryable、delayed candidate；
- 日志包含 candidate identity、source、settlement status、policy action、reason 和 update job identity；
- 不引入需要跨实例锁或新的外部发布权限的运行时依赖。

## 实现前置条件

- 本文与 [0011-auto-update-candidate-settlement](../../adr/0011-auto-update-candidate-settlement.md) 的状态模型和边界一致。
- API 字段命名、来源枚举、retry reason、skipped reason 和 candidate 唯一键冻结。
- 数据库迁移和旧 pending/notification 历史兼容策略冻结。
- 实现必须遵循本合同；数据库迁移、前端改动和线上补偿必须分别经过对应的验证与授权门禁。

## 相关主题

- [版本推测采集解耦](../kdapc-version-inference-decouple/SPEC.md)
- [版本推测收敛](../wczjc-version-inference-convergence/SPEC.md)
- [版本推测可观测性](../e8kzr-version-inference-observability/SPEC.md)
- [新版本通知事件驱动收敛](../s4fqf-new-version-notify-event-settle-explicit-version/SPEC.md)
- [更新候选跨版本发现次数](../2hnkx-new-version-discovery-count/SPEC.md)
- [自动部署策略配置器](../xyy72-auto-deploy-policy-configurator/SPEC.md)
