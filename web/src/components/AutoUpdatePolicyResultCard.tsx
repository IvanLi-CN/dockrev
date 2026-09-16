import type { AutoUpdatePolicy, CandidateSettlement } from '../api'
import { Button, Mono, Pill } from '../ui'
import {
  activeAutoUpdateRules,
  autoUpdatePolicySummary,
  autoUpdateRuleSummary,
} from './AutoUpdatePolicyEditor'

function policyResult(props: {
  policy: AutoUpdatePolicy
  scope: 'service' | 'stack'
  stackPolicy?: AutoUpdatePolicy | null
}) {
  if (props.scope === 'service') {
    if (props.policy.mode === 'disabled') {
      return {
        detail: '不会执行 Stack 级自动部署策略。',
        effectivePolicy: null,
        source: 'service disabled',
        state: '未启用',
        tone: 'muted' as const,
      }
    }
    if (props.policy.mode === 'inherit') {
      const stackPolicy = props.stackPolicy ?? null
      return {
        detail: stackPolicy?.enabled ? '使用 Stack 策略作为最终自动部署结果。' : 'Stack 策略未启用，当前服务不会自动部署。',
        effectivePolicy: stackPolicy,
        source: '继承 Stack',
        state: autoUpdatePolicySummary(stackPolicy),
        tone: stackPolicy?.enabled ? ('ok' as const) : ('muted' as const),
      }
    }
    return {
      detail: props.policy.enabled ? '使用服务级覆盖策略作为最终自动部署结果。' : '服务覆盖策略未启用。',
      effectivePolicy: props.policy,
      source: '服务覆盖',
      state: autoUpdatePolicySummary(props.policy),
      tone: props.policy.enabled ? ('ok' as const) : ('muted' as const),
    }
  }

  return {
    detail: props.policy.enabled ? 'Stack 策略会被继承它的服务使用。' : 'Stack 策略未启用。',
    effectivePolicy: props.policy,
    source: 'Stack 策略',
    state: autoUpdatePolicySummary(props.policy),
    tone: props.policy.enabled ? ('ok' as const) : ('muted' as const),
  }
}

export function policyActionLabel(status: string): string {
  switch (status) {
    case 'waiting_inference': return '等待版本证据'
    case 'rule_not_matched': return '规则未命中'
    case 'delayed': return '等待延迟条件'
    case 'queued': return '更新已排队'
    case 'running': return '更新执行中'
    case 'completed': return '更新已完成'
    case 'failed': return '更新失败'
    case 'skipped': return '已跳过'
    case 'unresolved': return '版本无法解析'
    default: return status
  }
}

export function policyActionTone(status: string): 'ok' | 'warn' | 'bad' | 'muted' | 'info' {
  switch (status) {
    case 'delayed':
      return 'warn'
    case 'failed':
    case 'unresolved':
      return 'bad'
    case 'queued':
    case 'running':
    case 'waiting_inference':
      return 'info'
    case 'completed':
      return 'ok'
    default:
      return 'muted'
  }
}

const settlementReasonLabels: Record<string, string> = {
  digest_bound_version: 'digest 已绑定版本',
  version_inference_pending: '等待版本证据',
  version_inference_unresolved: '版本证据无法解析',
}

export function candidateSettlementDetail(settlement: CandidateSettlement): string {
  const details = [
    settlementReasonLabels[settlement.reason ?? ''] ?? settlement.reason,
    settlement.attempts > 0 ? `尝试 ${settlement.attempts} 次` : null,
    settlement.retryAt ? `下次重试 ${settlement.retryAt}` : null,
  ].filter(Boolean)
  return details.length > 0 ? details.join(' · ') : '无需继续推断'
}

export function AutoUpdatePolicyResultCard(props: {
  busy?: boolean
  onOpenSettings: () => void
  policy: AutoUpdatePolicy
  scope: 'service' | 'stack'
  stackPolicy?: AutoUpdatePolicy | null
  projection?: { policyStatus: string; reason?: string | null; ruleId?: string | null; evaluatedAt?: string | null } | null
  candidateSettlement?: CandidateSettlement | null
}) {
  const result = policyResult(props)
  const rules = activeAutoUpdateRules(result.effectivePolicy)
  const primaryRule = rules[0] ?? null

  return (
    <div className="card autoPolicyResultCard">
      <div className="autoPolicyResultHead">
        <div>
          <div className="title">自动更新结果</div>
          <div className="muted">{result.detail}</div>
        </div>
        <div className="autoPolicyResultActions">
          <Pill tone={result.tone}>{result.state}</Pill>
          <Button disabled={props.busy} onClick={props.onOpenSettings} variant="primary">
            设置
          </Button>
        </div>
      </div>

      <div className="autoPolicyResultFacts">
        <div className="autoPolicyFactCell">
          <span className="label autoPolicyFactLabel">来源</span>
          <span className="autoPolicyFactValue">
            <Mono>{result.source}</Mono>
          </span>
        </div>
        <div className="autoPolicyFactCell">
          <span className="label autoPolicyFactLabel">启用规则</span>
          <span className="autoPolicyFactValue">
            <Mono>{rules.length}</Mono>
          </span>
        </div>
        <div className="autoPolicyFactCell">
          <span className="label autoPolicyFactLabel">最终动作</span>
          <span className="autoPolicyFactValue">
            {primaryRule ? `${primaryRule.name} · ${autoUpdateRuleSummary(primaryRule)}` : '无自动部署动作'}
          </span>
        </div>
        {props.projection ? (
          <div className="autoPolicyFactCell">
            <span className="label autoPolicyFactLabel">策略动作</span>
            <span className="autoPolicyFactValue">
              <Mono>{policyActionLabel(props.projection.policyStatus)}</Mono>
              {props.projection.reason ? <span>{props.projection.reason}</span> : null}
            </span>
          </div>
        ) : null}
        {props.candidateSettlement ? (
          <div className="autoPolicyFactCell" data-auto-policy-evidence="candidate-settlement">
            <span className="label autoPolicyFactLabel">候选证据</span>
            <span className="autoPolicyFactValue">
              <Mono>{props.candidateSettlement.status}</Mono>
              <span>{candidateSettlementDetail(props.candidateSettlement)}</span>
            </span>
          </div>
        ) : null}
      </div>

      {primaryRule ? (
        <div className="autoPolicyResultRule">
          <Mono>{primaryRule.matcher.type}</Mono>
          <span>{primaryRule.matcher.pattern}</span>
        </div>
      ) : null}
    </div>
  )
}
