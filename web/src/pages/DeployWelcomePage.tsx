import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import {
  getDeployCheckReport,
  getDeployWelcome,
  putDeployWelcome,
  type DeployCheckItem,
  type DeployCheckReportResponse,
} from '../api'
import { navigate } from '../routes'
import { Button, RefreshIcon } from '../ui'

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

export function DeployWelcomePage(props: { onTopActions: (node: ReactNode) => void }) {
  const { onTopActions } = props
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

  useEffect(() => {
    onTopActions(
      <Button
        variant="ghost"
        disabled={loading || saving}
        onClick={() => {
          void refresh()
        }}
      >
        <RefreshIcon className="inlineIcon" />
        重新检查
      </Button>,
    )
    return () => onTopActions(null)
  }, [loading, onTopActions, refresh, saving])

  const groups = useMemo(() => {
    const core: DeployCheckItem[] = []
    const feature: DeployCheckItem[] = []
    for (const item of report?.checks ?? []) {
      if (normalizeGroup(item.group, item.id) === 'core') core.push(item)
      else feature.push(item)
    }
    return { core, feature }
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
        {hasBlockingFailures ? (
          <div className="deployBlockingList">
            <div className="label">阻塞项：</div>
            <div className="mono">{report.overall.blockingCheckIds.join(', ')}</div>
          </div>
        ) : null}
      </div>

      <div className="card deployChecklistCard">
        <div className="title">核心功能（必须可用）</div>
        <div className="deployChecklistGrid">
          {groups.core.map((item) => (
            <DeployCheckRow key={item.id} item={item} />
          ))}
        </div>
      </div>

      <div className="card deployChecklistCard">
        <div className="title">条件功能（按启用状态判定）</div>
        <div className="deployChecklistGrid">
          {groups.feature.map((item) => (
            <DeployCheckRow key={item.id} item={item} />
          ))}
        </div>
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

function DeployCheckRow(props: { item: DeployCheckItem }) {
  const { item } = props
  const statusClass =
    item.status === 'pass'
      ? 'deployStatusPill deployStatusPillPass'
      : item.status === 'fail'
        ? 'deployStatusPill deployStatusPillFail'
        : 'deployStatusPill deployStatusPillNa'

  return (
    <div className={item.status === 'fail' && item.required ? 'deployCheckRow deployCheckRowFail' : 'deployCheckRow'}>
      <div className="deployCheckHead">
        <div>
          <div className="deployCheckTitle">{item.title}</div>
          <div className="mono">{item.id}</div>
        </div>
        <div className="deployCheckFlags">
          <span className={statusClass}>{item.status.toUpperCase()}</span>
          <span className={item.required ? 'deployRequiredFlag deployRequiredFlagYes' : 'deployRequiredFlag'}>
            {item.required ? 'required' : 'optional'}
          </span>
        </div>
      </div>

      <div className="deployCheckBody">
        <div>
          <span className="label">结论：</span>
          <span className="muted">{item.summary}</span>
        </div>
        <div>
          <span className="label">影响：</span>
          <span className="muted">{item.impact}</span>
        </div>
        <div>
          <span className="label">证据：</span>
          <span className="mono">{item.evidence || '-'}</span>
        </div>
        <div>
          <span className="label">建议：</span>
          <span className="muted">{item.recommendation || '无需操作'}</span>
        </div>
      </div>
    </div>
  )
}
