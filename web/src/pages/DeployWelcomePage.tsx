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
      <div className="page">
        <div className="card">
          <div className="title">部署检查</div>
          <div className="muted" style={{ marginTop: 10 }}>{loading ? '正在加载检查报告…' : error ?? '无法加载检查报告'}</div>
          <div className="formActions">
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
        </div>
      </div>
    )
  }

  return (
    <div className="page deployWelcomePage">
      <div className="card deployWelcomeHero">
        <div className="deployWelcomeHeroTop">
          <div>
            <div className="title">部署检查清单</div>
            <div className="muted" style={{ marginTop: 8 }}>
              面向运维：确认“功能是否因配置缺失而不可用”，不依赖任务队列状态
            </div>
          </div>
          <div className={hasBlockingFailures ? 'deployOverallBadge deployOverallBadgeFail' : 'deployOverallBadge deployOverallBadgePass'}>
            {hasBlockingFailures ? 'FAIL' : 'PASS'}
          </div>
        </div>
        <div className="deployWelcomeHeroSummary">
          <div className="muted">{report.overall.summary}</div>
          <div className="muted">生成时间：{formatTime(report.generatedAt)}</div>
        </div>
        <div className="deployChecklistStatRow">
          <div className="deployChecklistStat">
            <div className="deployChecklistStatLabel">必需项</div>
            <div className="deployChecklistStatValue">{stats.requiredTotal}</div>
          </div>
          <div className="deployChecklistStat">
            <div className="deployChecklistStatLabel">已通过</div>
            <div className="deployChecklistStatValue">{stats.requiredPass}</div>
          </div>
          <div className="deployChecklistStat">
            <div className="deployChecklistStatLabel">阻塞失败</div>
            <div className="deployChecklistStatValue">{stats.requiredFail}</div>
          </div>
          <div className="deployChecklistStat">
            <div className="deployChecklistStatLabel">可选项（NA）</div>
            <div className="deployChecklistStatValue">
              {stats.optionalNa}/{stats.optionalTotal}
            </div>
          </div>
        </div>
        {hasBlockingFailures ? (
          <div className="deployBlockingList">
            <div className="label">阻塞项：</div>
            <div className="mono">{report.overall.blockingCheckIds.join(', ')}</div>
          </div>
        ) : null}
      </div>

      <div className="card deployChecklistCard">
        <div className="deployChecklistHead">
          <div className="title">核心功能检查清单（必须可用）</div>
          <div className="deployChecklistLegend">
            <span className="deployChecklistLegendItem">
              <span className="deployChecklistBox deployChecklistBoxPass" aria-hidden="true">
                ✓
              </span>
              PASS
            </span>
            <span className="deployChecklistLegendItem">
              <span className="deployChecklistBox deployChecklistBoxFail" aria-hidden="true">
                !
              </span>
              FAIL
            </span>
            <span className="deployChecklistLegendItem">
              <span className="deployChecklistBox deployChecklistBoxNa" aria-hidden="true">
                –
              </span>
              NA
            </span>
          </div>
        </div>
        <ol className="deployChecklistList">
          {groups.core.map((item) => (
            <DeployChecklistItem key={item.id} item={item} />
          ))}
        </ol>
      </div>

      <div className="card deployChecklistCard">
        <div className="deployChecklistHead">
          <div className="title">条件功能检查清单（按启用状态判定）</div>
          <div className="muted">未启用功能显示为 NA，不纳入 FAIL 判定。</div>
        </div>
        <ol className="deployChecklistList">
          {groups.feature.map((item) => (
            <DeployChecklistItem key={item.id} item={item} />
          ))}
        </ol>
      </div>

      <div className="card deployWelcomeFooter">
        <label className="deployNeverAutoCheckbox">
          <input
            type="checkbox"
            checked={neverAutoOpen}
            onChange={(e) => setNeverAutoOpen(e.target.checked)}
            disabled={saving}
          />
          不再自动显示此页面
        </label>
        <div className="muted">勾选后，后续访问首页将直接进入 Dashboard；可在“系统设置”里重新打开本页。</div>

        <div className="formActions" style={{ marginTop: 14 }}>
          <Button variant="primary" disabled={saving} onClick={() => void enterDashboard()}>
            {saving ? '保存中…' : '进入 Dashboard'}
          </Button>
          <Button variant="ghost" disabled={loading || saving} onClick={() => void refresh()}>
            重新检查
          </Button>
        </div>
        {error ? <div className="error">{error}</div> : null}
      </div>
    </div>
  )
}

function DeployChecklistItem(props: { item: DeployCheckItem }) {
  const { item } = props
  const statusClass = item.status === 'pass' ? 'pass' : item.status === 'fail' ? 'fail' : 'na'
  const markLabel = item.status === 'pass' ? '✓' : item.status === 'fail' ? '!' : '–'
  const titleClass = item.status === 'fail' && item.required ? 'deployChecklistItem deployChecklistItemFail' : 'deployChecklistItem'

  return (
    <li className={titleClass}>
      <div className="deployChecklistMarker">
        <span className={`deployChecklistBox deployChecklistBox${statusClass[0].toUpperCase()}${statusClass.slice(1)}`}>{markLabel}</span>
      </div>
      <div className="deployChecklistContent">
        <div className="deployChecklistTitleRow">
          <div className="deployCheckTitle">{item.title}</div>
          <div className="deployCheckFlags">
            <span className={item.required ? 'deployRequiredFlag deployRequiredFlagYes' : 'deployRequiredFlag'}>
              {item.required ? 'required' : 'optional'}
            </span>
            <span className={`deployStatusPill deployStatusPill${statusClass[0].toUpperCase()}${statusClass.slice(1)}`}>
              {item.status.toUpperCase()}
            </span>
          </div>
        </div>
        <div className="mono">{item.id}</div>
        <div className="deployChecklistSummary">
          <span className="label">判定：</span>
          <span className="muted">{item.summary}</span>
        </div>
        <ul className="deployChecklistMeta">
          <li>
            <span className="label">影响</span>
            <span className="muted">{item.impact}</span>
          </li>
          <li>
            <span className="label">证据</span>
            <span className="mono">{item.evidence || '-'}</span>
          </li>
          <li>
            <span className="label">建议</span>
            <span className="muted">{item.recommendation || '无需操作'}</span>
          </li>
        </ul>
      </div>
    </li>
  )
}
