import type { AutoUpdatePolicy } from '../api'
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

export function AutoUpdatePolicyResultCard(props: {
  busy?: boolean
  onOpenSettings: () => void
  policy: AutoUpdatePolicy
  scope: 'service' | 'stack'
  stackPolicy?: AutoUpdatePolicy | null
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
