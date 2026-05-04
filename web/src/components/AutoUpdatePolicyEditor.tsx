import { Button, Input, Mono, Pill, SelectField, Switch } from '../ui'
import type {
  AutoUpdateMatcherType,
  AutoUpdatePolicy,
  AutoUpdatePolicyMode,
  AutoUpdateRule,
  AutoUpdateRuleAction,
} from '../api'

export const AUTO_UPDATE_TIME_PRESETS = [
  { value: 0, label: '立即' },
  { value: 900, label: '15m' },
  { value: 3600, label: '1h' },
  { value: 10800, label: '3h' },
  { value: 21600, label: '6h' },
  { value: 43200, label: '12h' },
  { value: 86400, label: '1d' },
  { value: 259200, label: '3d' },
  { value: 604800, label: '7d' },
] as const

export const AUTO_UPDATE_VERSION_PRESETS = [
  { value: 0, label: '0' },
  { value: 1, label: '1' },
  { value: 2, label: '2' },
  { value: 3, label: '3' },
  { value: 5, label: '5' },
  { value: 8, label: '8' },
] as const

export function createDefaultAutoUpdatePolicy(mode: AutoUpdatePolicyMode): AutoUpdatePolicy {
  return {
    mode,
    enabled: false,
    rules: [],
  }
}

function defaultRule(index: number): AutoUpdateRule {
  return {
    id: `rule-${index + 1}`,
    name: `规则 ${index + 1}`,
    enabled: true,
    matcher: { type: 'semver', pattern: '>=1.0.0' },
    action: 'delayed',
    delay: { minAgeSeconds: 900, minVersionLag: 1 },
  }
}

function policyModeLabel(mode: AutoUpdatePolicyMode): string {
  if (mode === 'inherit') return '继承 Stack'
  if (mode === 'override') return '服务覆盖'
  return '禁用自动更新'
}

function matcherTypeLabel(type: AutoUpdateMatcherType): string {
  if (type === 'semver') return 'Semver 版本范围'
  if (type === 'regex') return 'Regex 正则'
  return 'Glob 通配符'
}

function matcherHelp(type: AutoUpdateMatcherType): string {
  if (type === 'semver') return '匹配候选展示版本，例如 >=1.0.0, <2.0.0。'
  if (type === 'regex') return '匹配完整候选版本或 raw tag，不匹配镜像仓库名。'
  return '使用 Docker tag 通配符 * 和 ?，例如 5.2.*。'
}

function matcherPlaceholder(type: AutoUpdateMatcherType): string {
  if (type === 'semver') return '>=1.0.0, <2.0.0'
  if (type === 'regex') return '^5\\.2\\.[0-9]+$'
  return '5.2.*'
}

function presetIndex<T extends readonly { value: number }[]>(items: T, value: number): number {
  const index = items.findIndex((item) => item.value === value)
  return index >= 0 ? index : 0
}

export function autoUpdateTimeLabel(value: number): string {
  return AUTO_UPDATE_TIME_PRESETS[presetIndex(AUTO_UPDATE_TIME_PRESETS, value)]?.label ?? '立即'
}

export function autoUpdateVersionLagLabel(value: number): string {
  return AUTO_UPDATE_VERSION_PRESETS[presetIndex(AUTO_UPDATE_VERSION_PRESETS, value)]?.label ?? '0'
}

export function autoUpdateRuleSummary(rule: AutoUpdateRule): string {
  if (rule.action === 'immediate') return '命中后立即部署'
  return `延迟 ${autoUpdateTimeLabel(rule.delay.minAgeSeconds)}，并落后 ${autoUpdateVersionLagLabel(rule.delay.minVersionLag)} 个匹配版本`
}

export function autoUpdatePolicySummary(policy: AutoUpdatePolicy | null | undefined): string {
  if (!policy || !policy.enabled) return '未启用'
  const active = policy.rules.filter((rule) => rule.enabled).length
  return `${active}/${policy.rules.length} 条规则启用`
}

export function activeAutoUpdateRules(policy: AutoUpdatePolicy | null | undefined): AutoUpdateRule[] {
  if (!policy || !policy.enabled) return []
  return policy.rules.filter((rule) => rule.enabled)
}

function validationMessage(policy: AutoUpdatePolicy): string | null {
  if (policy.mode === 'disabled') return null
  if (policy.mode === 'inherit') return null
  if (policy.enabled && policy.rules.length === 0) return '启用后至少需要一条规则'
  const unnamedRule = policy.rules.find((rule) => !rule.name.trim())
  if (unnamedRule) return `规则 ${unnamedRule.id || '-'} 需要名称`
  const emptyPatternRule = policy.rules.find((rule) => !rule.matcher.pattern.trim())
  if (emptyPatternRule) return `规则 ${emptyPatternRule.name || emptyPatternRule.id || '-'} 需要匹配规则`
  return null
}

export function AutoUpdatePolicyEditor(props: {
  scope: 'service' | 'stack'
  policy: AutoUpdatePolicy
  stackPolicy?: AutoUpdatePolicy | null
  busy?: boolean
  onChange: (policy: AutoUpdatePolicy) => void
  onSave?: () => void
}) {
  const { policy } = props
  const isService = props.scope === 'service'
  const effectiveMode = isService ? policy.mode : 'override'
  const canEditRules = effectiveMode !== 'disabled' && effectiveMode !== 'inherit'
  const validation = validationMessage(policy)

  const setPolicy = (patch: Partial<AutoUpdatePolicy>) => {
    props.onChange({ ...policy, ...patch })
  }
  const setRule = (index: number, next: AutoUpdateRule) => {
    setPolicy({ rules: policy.rules.map((rule, idx) => (idx === index ? next : rule)) })
  }
  const removeRule = (index: number) => {
    setPolicy({ rules: policy.rules.filter((_, idx) => idx !== index) })
  }

  return (
    <div className="autoPolicyEditor">
      <div className="autoPolicyTop">
        <div>
          <div className="title">自动更新策略</div>
          <div className="muted">延迟门槛按时间和版本数同时计算。</div>
        </div>
        <Pill tone={policy.enabled ? 'ok' : 'muted'}>{autoUpdatePolicySummary(policy)}</Pill>
      </div>

      <div className="autoPolicyControls">
        {isService ? (
          <label className="formField">
            <span className="label">模式</span>
            <SelectField
              disabled={props.busy}
              onChange={(value) => setPolicy({ mode: value })}
              options={[
                { value: 'inherit', label: policyModeLabel('inherit') },
                { value: 'override', label: policyModeLabel('override') },
                { value: 'disabled', label: policyModeLabel('disabled') },
              ]}
              value={policy.mode}
            />
          </label>
        ) : null}
        <div className="autoPolicySwitchRow">
          <span className="label">启用</span>
          <Switch
            aria-label="启用自动更新策略"
            checked={policy.enabled}
            disabled={props.busy || effectiveMode === 'disabled' || effectiveMode === 'inherit'}
            onChange={(enabled) => setPolicy({ enabled })}
          />
        </div>
        {isService && effectiveMode === 'inherit' ? (
          <div className="autoPolicyInherited">
            <span>继承 Stack</span>
            <Mono>{autoUpdatePolicySummary(props.stackPolicy)}</Mono>
          </div>
        ) : null}
      </div>

      {effectiveMode === 'disabled' ? (
        <div className="autoPolicyEmpty">此服务不会执行 Stack 级自动部署策略。</div>
      ) : canEditRules ? (
        <>
          <div className="autoPolicyRuleList">
            {policy.rules.map((rule, index) => (
              <div className="autoPolicyRule" key={`${rule.id}-${index}`}>
                <div className="autoPolicyRuleHead">
                  <Switch
                    aria-label={`启用规则 ${rule.name || index + 1}`}
                    checked={rule.enabled}
                    disabled={props.busy}
                    onChange={(enabled) => setRule(index, { ...rule, enabled })}
                  />
                  <Input
                    className="input autoPolicyRuleName"
                    disabled={props.busy}
                    onChange={(event) => setRule(index, { ...rule, name: event.target.value })}
                    value={rule.name}
                  />
                  <Button disabled={props.busy} onClick={() => removeRule(index)} variant="ghost">
                    删除
                  </Button>
                </div>

                <div className="autoPolicyRuleGrid">
                  <label className="formField">
                    <span className="label">匹配</span>
                    <SelectField<AutoUpdateMatcherType>
                      disabled={props.busy}
                      onChange={(type) => setRule(index, { ...rule, matcher: { ...rule.matcher, type } })}
                      options={[
                        { value: 'semver', label: matcherTypeLabel('semver') },
                        { value: 'regex', label: matcherTypeLabel('regex') },
                        { value: 'glob', label: matcherTypeLabel('glob') },
                      ]}
                      value={rule.matcher.type}
                    />
                    <span className="muted autoPolicyFieldHint">{matcherHelp(rule.matcher.type)}</span>
                  </label>
                  <label className="formField autoPolicyPattern">
                    <span className="label">规则</span>
                    <Input
                      className="input"
                      disabled={props.busy}
                      onChange={(event) =>
                        setRule(index, { ...rule, matcher: { ...rule.matcher, pattern: event.target.value } })
                      }
                      placeholder={matcherPlaceholder(rule.matcher.type)}
                      value={rule.matcher.pattern}
                    />
                    <span className="muted autoPolicyFieldHint">先匹配候选展示版本；没有展示版本时回退 raw tag。</span>
                  </label>
                  <label className="formField">
                    <span className="label">动作</span>
                    <SelectField<AutoUpdateRuleAction>
                      disabled={props.busy}
                      onChange={(action) => setRule(index, { ...rule, action })}
                      options={[
                        { value: 'immediate', label: '立即' },
                        { value: 'delayed', label: '延迟' },
                      ]}
                      value={rule.action}
                    />
                  </label>
                </div>

                {rule.action === 'delayed' ? (
                  <div className="autoPolicyDelayGrid">
                    <NonlinearSlider
                      disabled={props.busy}
                      label="时间"
                      onChange={(minAgeSeconds) => setRule(index, { ...rule, delay: { ...rule.delay, minAgeSeconds } })}
                      presets={AUTO_UPDATE_TIME_PRESETS}
                      value={rule.delay.minAgeSeconds}
                    />
                    <NonlinearSlider
                      disabled={props.busy}
                      label="版本"
                      onChange={(minVersionLag) => setRule(index, { ...rule, delay: { ...rule.delay, minVersionLag } })}
                      presets={AUTO_UPDATE_VERSION_PRESETS}
                      value={rule.delay.minVersionLag}
                    />
                  </div>
                ) : null}

                <div className="autoPolicyPreview">
                  <Mono>{rule.matcher.type}</Mono>
                  <span>{rule.matcher.pattern || '-'}</span>
                  <span>{autoUpdateRuleSummary(rule)}</span>
                </div>
              </div>
            ))}
            {policy.rules.length === 0 ? <div className="autoPolicyEmpty">暂无规则</div> : null}
          </div>
        </>
      ) : null}
      <div className="autoPolicyActions">
        {canEditRules ? (
          <Button disabled={props.busy} onClick={() => setPolicy({ rules: [...policy.rules, defaultRule(policy.rules.length)] })}>
            添加规则
          </Button>
        ) : null}
        {props.onSave ? (
          <Button disabled={props.busy || Boolean(validation)} onClick={props.onSave} variant="primary">
            保存策略
          </Button>
        ) : null}
      </div>
      {validation ? <div className="autoPolicyValidation">{validation}</div> : null}
    </div>
  )
}

function NonlinearSlider(props: {
  label: string
  presets: readonly { value: number; label: string }[]
  value: number
  disabled?: boolean
  onChange: (value: number) => void
}) {
  const index = presetIndex(props.presets, props.value)
  return (
    <div className="autoPolicySlider">
      <div className="autoPolicySliderHead">
        <span className="label">{props.label}</span>
        <Mono>{props.presets[index]?.label ?? props.presets[0]?.label}</Mono>
      </div>
      <input
        aria-label={props.label}
        disabled={props.disabled}
        max={props.presets.length - 1}
        min={0}
        onChange={(event) => props.onChange(props.presets[Number(event.target.value)]?.value ?? props.presets[0]!.value)}
        step={1}
        type="range"
        value={index}
      />
      <div className="autoPolicySliderTicks">
        {props.presets.map((preset) => (
          <span key={preset.value}>{preset.label}</span>
        ))}
      </div>
    </div>
  )
}
