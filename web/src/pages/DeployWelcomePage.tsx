import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  getDeployCheckReport,
  getDeployWelcome,
  putDeployWelcome,
  type DeployCheckItem,
  type DeployCheckReportResponse,
} from '../api'
import { navigate } from '../routes'
import { Button } from '../ui'

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function formatTime(ts: string): string {
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function normalizeGroup(input: DeployCheckItem['group'], id: string): 'core' | 'feature' {
  if (input === 'core' || input === 'feature') return input
  if (id.startsWith('core.')) return 'core'
  return 'feature'
}

function statusMeta(status: DeployCheckItem['status']): {
  text: 'PASS' | 'FAIL' | 'NA'
  mark: '✓' | '✕' | '–'
  desc: string
} {
  switch (status) {
    case 'pass':
      return { text: 'PASS', mark: '✓', desc: '功能可用' }
    case 'fail':
      return { text: 'FAIL', mark: '✕', desc: '功能不可用（缺配置或配置错误）' }
    default:
      return { text: 'NA', mark: '–', desc: '功能未启用，不参与失败判定' }
  }
}

export function DeployWelcomePage() {
  const [report, setReport] = useState<DeployCheckReportResponse | null>(null)
  const [neverAutoOpen, setNeverAutoOpen] = useState(false)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [reportData, welcome] = await Promise.all([getDeployCheckReport(), getDeployWelcome()])
      setReport(reportData)
      setNeverAutoOpen(welcome.neverAutoOpen)
    } catch (e: unknown) {
      setError(errorMessage(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const groups = useMemo(() => {
    const core: DeployCheckItem[] = []
    const feature: DeployCheckItem[] = []
    for (const item of report?.checks ?? []) {
      if (normalizeGroup(item.group, item.id) === 'core') core.push(item)
      else feature.push(item)
    }
    return { core, feature }
  }, [report])

  const stats = useMemo(() => {
    const checks = report?.checks ?? []
    const required = checks.filter((item) => item.required)
    const optional = checks.filter((item) => !item.required)
    return {
      requiredTotal: required.length,
      requiredPass: required.filter((item) => item.status === 'pass').length,
      requiredFail: required.filter((item) => item.status === 'fail').length,
      optionalTotal: optional.length,
      optionalNa: optional.filter((item) => item.status === 'na').length,
    }
  }, [report])

  const hasBlockingFailures = report?.overall.result === 'fail'

  async function enterDashboard() {
    setSaving(true)
    setError(null)
    try {
      await putDeployWelcome({ neverAutoOpen })
      if (typeof window !== 'undefined') {
        window.sessionStorage.setItem('dockrev:deployWelcome:redirected', '1')
      }
      navigate({ name: 'overview' })
    } catch (e: unknown) {
      setError(errorMessage(e))
    } finally {
      setSaving(false)
    }
  }

  if (!report) {
    return (
      <div className="deployWelcomeRoot">
        <main className="deployWelcomeMain">
          <section className="deployWelcomePanel">
            <p className="deployWelcomeEyebrow">Deployment Checklist</p>
            <h1 className="deployWelcomeTitle">部署功能完整性检查</h1>
            <p className="deployWelcomeSubtitle">{loading ? '正在加载部署检查报告…' : error ?? '无法加载检查报告'}</p>
            <div className="deployWelcomeActions">
              <Button
                variant="primary"
                disabled={loading}
                onClick={() => {
                  void refresh()
                }}
              >
                重试
              </Button>
            </div>
          </section>
        </main>
      </div>
    )
  }

  return (
    <div className="deployWelcomeRoot">
      <main className="deployWelcomeMain">
        <section className={`deployWelcomePanel deployWelcomeSummaryPanel ${hasBlockingFailures ? 'is-fail' : 'is-pass'}`}>
          <div className="deployWelcomeSummaryHead">
            <div>
              <p className="deployWelcomeEyebrow">Deployment Checklist</p>
              <h1 className="deployWelcomeTitle">部署功能完整性检查清单</h1>
              <p className="deployWelcomeSubtitle">仅判定“功能是否因配置缺失而不可用”，不依赖 jobs 数据与外部网络。</p>
            </div>
            <div className={`deployWelcomeOverall ${hasBlockingFailures ? 'is-fail' : 'is-pass'}`}>
              <span className="deployWelcomeOverallLabel">整体结论</span>
              <strong>{hasBlockingFailures ? 'FAIL' : 'PASS'}</strong>
              <span className="deployWelcomeOverallSummary">{report.overall.summary}</span>
            </div>
          </div>

          <div className="deployWelcomeDefinitions" role="list" aria-label="状态说明">
            <div className="deployWelcomeDefinition" role="listitem">
              <span className="deployBadge pass">PASS</span>
              <span>所有必需功能可用</span>
            </div>
            <div className="deployWelcomeDefinition" role="listitem">
              <span className="deployBadge fail">FAIL</span>
              <span>至少一个必需功能不可用（会阻塞功能完整性）</span>
            </div>
            <div className="deployWelcomeDefinition" role="listitem">
              <span className="deployBadge na">NA</span>
              <span>该功能未启用，不纳入 FAIL 判定</span>
            </div>
          </div>

          <div className="deployWelcomeStats" role="list" aria-label="统计">
            <div className="deployWelcomeStat" role="listitem">
              <span>必需项总数</span>
              <strong>{stats.requiredTotal}</strong>
            </div>
            <div className="deployWelcomeStat" role="listitem">
              <span>必需项通过</span>
              <strong>{stats.requiredPass}</strong>
            </div>
            <div className="deployWelcomeStat" role="listitem">
              <span>必需项失败</span>
              <strong>{stats.requiredFail}</strong>
            </div>
            <div className="deployWelcomeStat" role="listitem">
              <span>可选项 NA</span>
              <strong>
                {stats.optionalNa}/{stats.optionalTotal}
              </strong>
            </div>
          </div>

          {hasBlockingFailures ? (
            <div className="deployBlockingNotice" role="alert">
              <span className="deployBadge fail">BLOCKING</span>
              <div>
                <div className="deployBlockingTitle">以下检查项导致整体 FAIL（需先修复）：</div>
                <div className="mono">{report.overall.blockingCheckIds.join(', ')}</div>
              </div>
            </div>
          ) : null}

          <div className="deployWelcomeGeneratedAt">报告生成时间：{formatTime(report.generatedAt)}</div>
        </section>

        <section className="deployWelcomePanel">
          <div className="deploySectionHead">
            <h2>核心功能 Checklist（必须可用）</h2>
            <p>任一项 FAIL 都会导致部署功能不完整。</p>
          </div>
          <DeployChecklistList items={groups.core} prefix="CORE" />
        </section>

        <section className="deployWelcomePanel">
          <div className="deploySectionHead">
            <h2>条件功能 Checklist（按启用状态）</h2>
            <p>功能未启用时显示 NA；启用后缺配置会标记 FAIL。</p>
          </div>
          <DeployChecklistList items={groups.feature} prefix="FEATURE" />
        </section>

        <section className="deployWelcomePanel deployWelcomeActionPanel">
          <label className="deployNeverAutoCheckbox">
            <input
              type="checkbox"
              checked={neverAutoOpen}
              onChange={(e) => setNeverAutoOpen(e.target.checked)}
              disabled={saving}
            />
            <span>不再自动显示此页面</span>
          </label>
          <p className="deployWelcomeActionHint">勾选后，后续访问首页将直接进入 Dashboard；可在设置页手动重新打开本页面。</p>
          <div className="deployWelcomeActions">
            <Button variant="primary" disabled={saving} onClick={() => void enterDashboard()}>
              {saving ? '保存中…' : '进入 Dashboard'}
            </Button>
            <Button variant="ghost" disabled={loading || saving} onClick={() => void refresh()}>
              重新检查
            </Button>
          </div>
          {error ? <div className="error">{error}</div> : null}
        </section>
      </main>
    </div>
  )
}

function DeployChecklistList(props: { items: DeployCheckItem[]; prefix: string }) {
  const { items, prefix } = props
  if (items.length === 0) {
    return <div className="deployChecklistEmpty">暂无检查项</div>
  }

  return (
    <ol className="deployChecklistList">
      {items.map((item, index) => (
        <DeployChecklistItem key={item.id} item={item} number={`${prefix}-${index + 1}`} />
      ))}
    </ol>
  )
}

function DeployChecklistItem(props: { item: DeployCheckItem; number: string }) {
  const { item, number } = props
  const status = statusMeta(item.status)
  const rowClass = [
    'deployChecklistItem',
    `deployChecklistItem--${item.status}`,
    item.required && item.status === 'fail' ? 'deployChecklistItem--blocking' : '',
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <li className={rowClass}>
      <div className="deployChecklistMark" aria-hidden="true">
        {status.mark}
      </div>
      <div className="deployChecklistBody">
        <div className="deployChecklistTopRow">
          <div className="deployChecklistTitleGroup">
            <span className="deployChecklistNumber">{number}</span>
            <h3>{item.title}</h3>
          </div>
          <div className="deployChecklistFlags">
            <span className={`deployBadge ${item.required ? 'required' : 'optional'}`}>
              {item.required ? 'required' : 'optional'}
            </span>
            <span className={`deployBadge ${item.status}`}>{status.text}</span>
          </div>
        </div>

        <div className="deployChecklistId mono">{item.id}</div>

        <dl className="deployChecklistFacts">
          <div>
            <dt>判定</dt>
            <dd>{item.summary}</dd>
          </div>
          <div>
            <dt>影响</dt>
            <dd>{item.impact}</dd>
          </div>
          <div>
            <dt>建议</dt>
            <dd>{item.recommendation || '无需操作'}</dd>
          </div>
          <div>
            <dt>证据</dt>
            <dd className="mono">{item.evidence || '-'}</dd>
          </div>
          <div>
            <dt>说明</dt>
            <dd>{status.desc}</dd>
          </div>
        </dl>
      </div>
    </li>
  )
}
