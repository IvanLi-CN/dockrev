import { useEffect, useMemo, useState } from 'react'
import { Button, Input, Mono, Pill, SelectField, Switch } from '../ui'
import type {
  AutoUpdateMatcherType,
  AutoUpdatePolicy,
  AutoUpdatePolicyMode,
  AutoUpdateRule,
  AutoUpdateRuleAction,
  NewVersionDiscoveryTimelineItem,
} from '../api'
import { getServiceNewVersionDiscoveryTimeline } from '../api'

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

type PreviewMatchResult =
  | { status: 'matched'; rule: AutoUpdateRule }
  | { status: 'missed' }
  | { status: 'uncertain'; reason: string }

function escapeRegex(value: string): string {
  return value.replace(/[\\^$.*+?()[\]{}|]/g, '\\$&')
}

function globToRegex(pattern: string): RegExp {
  let source = ''
  for (const char of pattern) {
    if (char === '*') source += '.*'
    else if (char === '?') source += '.'
    else source += escapeRegex(char)
  }
  return new RegExp(`^(?:${source})$`)
}

function parseLooseVersion(value: string): number[] | null {
  const match = value.trim().match(/^v?(\d+)(?:\.(\d+))?(?:\.(\d+))?$/)
  if (!match) return null
  return [match[1], match[2] ?? '0', match[3] ?? '0'].map((part) => Number(part))
}

function compareLooseVersion(left: number[], right: number[]): number {
  for (let index = 0; index < 3; index += 1) {
    const diff = (left[index] ?? 0) - (right[index] ?? 0)
    if (diff !== 0) return diff
  }
  return 0
}

function matchesSemverPreview(version: string, pattern: string): boolean | null {
  const parsedVersion = parseLooseVersion(version)
  if (!parsedVersion) return null
  const clauses = pattern
    .split(',')
    .flatMap((part) => part.trim().split(/\s+/))
    .map((part) => part.trim())
    .filter(Boolean)
  if (clauses.length === 0) return null

  for (const clause of clauses) {
    const match = clause.match(/^(>=|<=|>|<|=)?\s*v?(\d+(?:\.\d+){0,2})$/)
    if (!match) return null
    const op = match[1] || '='
    const target = parseLooseVersion(match[2])
    if (!target) return null
    const cmp = compareLooseVersion(parsedVersion, target)
    if (op === '>' && !(cmp > 0)) return false
    if (op === '>=' && !(cmp >= 0)) return false
    if (op === '<' && !(cmp < 0)) return false
    if (op === '<=' && !(cmp <= 0)) return false
    if (op === '=' && cmp !== 0) return false
  }
  return true
}

function matchRulePreview(rule: AutoUpdateRule, version: string): PreviewMatchResult {
  const pattern = rule.matcher.pattern.trim()
  if (!pattern) return { status: 'uncertain', reason: '规则为空' }
  try {
    if (rule.matcher.type === 'glob') {
      return globToRegex(pattern).test(version) ? { status: 'matched', rule } : { status: 'missed' }
    }
    if (rule.matcher.type === 'regex') {
      return new RegExp(`^(?:${pattern})$`).test(version) ? { status: 'matched', rule } : { status: 'missed' }
    }
    const matched = matchesSemverPreview(version, pattern)
    if (matched == null) return { status: 'uncertain', reason: 'semver 预览不确定' }
    return matched ? { status: 'matched', rule } : { status: 'missed' }
  } catch {
    return { status: 'uncertain', reason: '规则无法预览' }
  }
}

function previewPolicyFor(policy: AutoUpdatePolicy, stackPolicy: AutoUpdatePolicy | null | undefined): AutoUpdatePolicy | null {
  if (policy.mode === 'disabled') return null
  if (policy.mode === 'inherit') return stackPolicy ?? null
  return policy
}

function matchPolicyPreview(policy: AutoUpdatePolicy | null, version: string): PreviewMatchResult {
  if (!policy || !policy.enabled) return { status: 'missed' }
  for (const rule of policy.rules) {
    if (!rule.enabled) continue
    const result = matchRulePreview(rule, version)
    if (result.status !== 'missed') return result
  }
  return { status: 'missed' }
}

function formatPreviewTime(value: string | null | undefined): string {
  const trimmed = (value ?? '').trim()
  if (!trimmed) return '时间未知'
  const parsed = new Date(trimmed)
  if (Number.isNaN(parsed.valueOf())) return trimmed
  return new Intl.DateTimeFormat(undefined, {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(parsed)
}

function previewKindLabel(kind: NewVersionDiscoveryTimelineItem['kind']): string {
  if (kind === 'currentCandidate') return '当前候选'
  if (kind === 'currentRunning') return '当前运行'
  return '历史发现'
}

export function AutoUpdatePolicyEditor(props: {
  scope: 'service' | 'stack'
  policy: AutoUpdatePolicy
  stackPolicy?: AutoUpdatePolicy | null
  busy?: boolean
  onChange: (policy: AutoUpdatePolicy) => void
  onSave?: () => void
  previewServiceId?: string
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
          <label className="autoPolicyInlineField">
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
      {props.previewServiceId ? (
        <AutoUpdateHistoryPreview
          policy={previewPolicyFor(policy, props.stackPolicy)}
          serviceId={props.previewServiceId}
        />
      ) : null}
    </div>
  )
}

function AutoUpdateHistoryPreview(props: {
  policy: AutoUpdatePolicy | null
  serviceId: string
}) {
  const [timelineState, setTimelineState] = useState<{
    serviceId: string
    items: NewVersionDiscoveryTimelineItem[] | null
    status: 'loading' | 'ready' | 'error'
    error: string | null
  }>(() => ({ serviceId: props.serviceId, items: null, status: 'loading', error: null }))

  useEffect(() => {
    let cancelled = false
    void getServiceNewVersionDiscoveryTimeline(props.serviceId)
      .then((response) => {
        if (cancelled) return
        setTimelineState({
          serviceId: props.serviceId,
          items: response.items.slice(0, 20),
          status: 'ready',
          error: null,
        })
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setTimelineState({
          serviceId: props.serviceId,
          items: null,
          status: 'error',
          error: err instanceof Error && err.message.trim() ? err.message : '加载历史版本失败',
        })
      })
    return () => {
      cancelled = true
    }
  }, [props.serviceId])

  const activeState =
    timelineState.serviceId === props.serviceId
      ? timelineState
      : { serviceId: props.serviceId, items: null, status: 'loading' as const, error: null }
  const previewRows = useMemo(
    () =>
      (activeState.items ?? []).map((item) => ({
        item,
        result: matchPolicyPreview(props.policy, item.version),
      })),
    [activeState.items, props.policy],
  )

  const matchedCount = previewRows.filter((row) => row.result.status === 'matched').length

  return (
    <div className="autoPolicyHistoryPreview">
      <div className="autoPolicyHistoryHead">
        <div>
          <div className="title">历史版本命中预览</div>
          <div className="muted">展示最近 20 条内的发现记录，按当前草稿规则本地预览。</div>
        </div>
        <Pill tone={matchedCount > 0 ? 'info' : 'muted'}>{matchedCount}/{previewRows.length} 命中</Pill>
      </div>

      {activeState.status === 'loading' ? <div className="autoPolicyHistoryState">正在加载历史版本…</div> : null}
      {activeState.status === 'error' ? (
        <div className="autoPolicyHistoryState autoPolicyHistoryStateError">{activeState.error}</div>
      ) : null}
      {activeState.status === 'ready' && previewRows.length === 0 ? (
        <div className="autoPolicyHistoryState">暂无历史版本记录。</div>
      ) : null}
      {previewRows.length > 0 ? (
        <div className="autoPolicyHistoryList">
          {previewRows.map(({ item, result }, index) => (
            <div className="autoPolicyHistoryRow" key={`${item.kind}:${item.version}:${item.occurredAt ?? index}`}>
              <div className="autoPolicyHistoryVersion">
                <Mono>{item.version}</Mono>
                <span>{previewKindLabel(item.kind)}</span>
              </div>
              <div className="autoPolicyHistoryResult">
                {result.status === 'matched' ? (
                  <>
                    <Pill tone="ok">命中</Pill>
                    <span>{result.rule.name} · {autoUpdateRuleSummary(result.rule)}</span>
                  </>
                ) : result.status === 'uncertain' ? (
                  <>
                    <Pill tone="warn">不确定</Pill>
                    <span>{result.reason}</span>
                  </>
                ) : (
                  <>
                    <Pill tone="muted">未命中</Pill>
                    <span>不会触发自动部署</span>
                  </>
                )}
              </div>
              <div className="autoPolicyHistoryTime">{formatPreviewTime(item.occurredAt)}</div>
            </div>
          ))}
        </div>
      ) : null}
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
  const stopDrawerDrag = (event: { stopPropagation: () => void }) => {
    event.stopPropagation()
  }
  return (
    <div
      className="autoPolicySlider"
      onMouseDownCapture={stopDrawerDrag}
      onPointerDownCapture={stopDrawerDrag}
      onPointerMoveCapture={stopDrawerDrag}
      onTouchStartCapture={stopDrawerDrag}
      onTouchMoveCapture={stopDrawerDrag}
    >
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
