import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  getDeployCheckReport,
  getDeployWelcome,
  putDeployWelcome,
  refreshDeployCheckReport,
  type DeployCheckItem,
  type DeployCheckReportResponse,
} from '../api'
import {
  settleDeployCheckReport,
  hasBlockingDeployCheckFailure,
  shouldKeepDeployCheckLoading,
  shouldKeepPollingDeployCheckReport,
  shouldTriggerDeployCheckReportRefresh,
} from '../deployCheck'
import { navigate } from '../routes'
import { Button, Label, Switch } from '../ui'

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

function isWarningNa(item: DeployCheckItem, allChecks: DeployCheckItem[]): boolean {
  if (item.status !== 'na') return false
  if (item.naReason === 'missing_prerequisite') return true
  if (item.naReason === 'disabled_by_switch' || item.evidence.trim().toLowerCase() === 'enabled=false') return false
  if (item.naReason === 'not_applicable') return false

  // Backward-compatible fallback for older API payloads without naReason.
  if (item.id === 'feature.registry_auth') {
    const composeAccess = allChecks.find((check) => check.id === 'core.compose_access')
    if (composeAccess?.status === 'fail') return true
  }
  if (/\bmissing\b/i.test(item.summary) || /\bmissing\b/i.test(item.evidence)) return true
  return false
}

function resolveRecommendation(item: DeployCheckItem, allChecks: DeployCheckItem[], warningNa: boolean): string {
  const current = item.recommendation.trim()

  if (item.id === 'core.docker_engine' && item.status === 'fail') {
    return '在 Dockrev 运行环境执行 `docker info`；容器部署请挂载 `/var/run/docker.sock`（或设置 `DOCKER_HOST=tcp://docker-socket-proxy:2375`）；修复后重启 Dockrev 并重新检查。'
  }
  if (item.id === 'core.compose_access' && item.status === 'fail') {
    return '到概览页执行“发现扫描”；确保目标项目由 Docker Compose 启动并带 compose labels；若 Dockrev 在容器内，按相同绝对路径只读挂载 compose 文件目录。'
  }
  if (item.id === 'core.service_image_ref_valid' && item.status === 'fail') {
    return '修复 compose 中服务的 `image` 字段（例如 `ghcr.io/org/app:1.2.3` 或 `ghcr.io/org/app@sha256:...`），然后重新发现/扫描。'
  }
  if (item.id === 'core.update_executor_ready' && item.status === 'fail') {
    return '设置 `DOCKREV_COMPOSE_BIN` 为 Compose V2+ 命令：插件模式用 `docker`（要求 `docker compose version`），standalone 模式用 `docker-compose`（要求 `docker-compose version`）。'
  }
  if (item.id === 'feature.registry_auth') {
    if (item.status === 'fail') {
      return '设置 `DOCKREV_DOCKER_CONFIG` 指向有效 Docker `config.json`，并在 `auths/credHelpers` 补齐缺失 registry 凭据（建议先 `docker login <host>`）。'
    }
    if (item.status === 'na') {
      if (warningNa || allChecks.some((check) => check.id === 'core.compose_access' && check.status === 'fail')) {
        return '先修复核心前置项（尤其 compose 发现/路径可访问）后再检查私有仓库鉴权。'
      }
      return '如需启用私有仓库镜像：使用私有 registry host（或 `docker.io/local/*`），并设置 `DOCKREV_DOCKER_CONFIG` 指向带凭据的 `config.json`。'
    }
  }
  if (item.id === 'feature.notifications.email') {
    if (item.status === 'fail') return '进入“设置 -> 通知 -> Email”补齐 `smtpUrl` 并保存，建议发送一次测试通知。'
    if (item.status === 'na') return '如需启用：进入“设置 -> 通知 -> Email”，打开开关并填写 `smtpUrl`。'
  }
  if (item.id === 'feature.notifications.webhook') {
    if (item.status === 'fail') return '进入“设置 -> 通知 -> Webhook”补齐合法 `http/https` URL 并保存。'
    if (item.status === 'na') return '如需启用：进入“设置 -> 通知 -> Webhook”，打开开关并填写可达 webhook URL。'
  }
  if (item.id === 'feature.notifications.telegram') {
    if (item.status === 'fail') return '进入“设置 -> 通知 -> Telegram”补齐 `botToken` 与 `chatId` 并保存。'
    if (item.status === 'na') return '如需启用：进入“设置 -> 通知 -> Telegram”，打开开关并填写 `botToken` 与 `chatId`。'
  }
  if (item.id === 'feature.notifications.web_push') {
    if (item.status === 'fail') return '进入“设置 -> 通知 -> Web Push”补齐 `vapidPublicKey`、`vapidPrivateKey`、`vapidSubject` 并保存。'
    if (item.status === 'na') return '如需启用：进入“设置 -> 通知 -> Web Push”，打开开关并填写 VAPID 配置。'
  }
  if (item.id === 'feature.github_packages') {
    if (item.status === 'fail') return '进入“设置 -> GitHub Packages”补齐 `PAT`、`callbackUrl`、`secret` 与目标仓库，然后测试触发。'
    if (item.status === 'na') return '如需启用：进入“设置 -> GitHub Packages”，开启功能并配置 `PAT`、`callbackUrl`、`secret`。'
  }

  if (current) return current
  return '无需操作'
}

export function DeployWelcomePage() {
  const [report, setReport] = useState<DeployCheckReportResponse | null>(null)
  const [neverAutoOpen, setNeverAutoOpen] = useState(false)
  const [welcomeLoaded, setWelcomeLoaded] = useState(false)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [reportRefreshing, setReportRefreshing] = useState(false)
  const [reportRefreshError, setReportRefreshError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setLoading(true)
    setError(null)
    setReportRefreshError(null)
    const [reportResult, welcomeResult] = await Promise.allSettled([getDeployCheckReport(), getDeployWelcome()])
    let keepLoadingAfterBootstrap = false
    let reportError: string | null = null

    if (reportResult.status === 'fulfilled') {
      const envelope = reportResult.value
      reportError = envelope.lastError ?? null
      setReportRefreshError(reportError)
      if (envelope.report) {
        setReport(envelope.report)
      }
      setReportRefreshing(Boolean(envelope.refreshing))
      const shouldWaitForFirstReport = shouldKeepDeployCheckLoading(envelope)
      keepLoadingAfterBootstrap = shouldWaitForFirstReport
      const shouldRequestRefresh = shouldTriggerDeployCheckReportRefresh(envelope)
      const shouldPoll = shouldRequestRefresh || shouldKeepPollingDeployCheckReport(envelope)
      if (shouldPoll) {
        setReportRefreshing(true)
        const seed = shouldRequestRefresh ? refreshDeployCheckReport() : Promise.resolve(envelope)
        void seed
          .then((nextEnvelope) => settleDeployCheckReport(nextEnvelope))
          .then((settled) => {
            setReportRefreshError(null)
            if (settled.report) {
              setReport(settled.report)
            }
            setReportRefreshing(Boolean(settled.refreshing))
            setLoading(false)
          })
          .catch((e) => {
            setReportRefreshError(errorMessage(e))
            setError(errorMessage(e))
            setReportRefreshing(false)
            setLoading(false)
          })
      }
    } else {
      reportError = errorMessage(reportResult.reason)
      setReportRefreshError(reportError)
      setError(errorMessage(reportResult.reason))
      setLoading(false)
      return
    }

    if (welcomeResult.status === 'fulfilled') {
      setNeverAutoOpen(welcomeResult.value.neverAutoOpen)
      setWelcomeLoaded(true)
      setError(reportError)
    } else {
      // Keep the checklist visible even if the preference endpoint is temporarily unavailable.
      setWelcomeLoaded(false)
      setError(`检查报告已加载，但欢迎页偏好读取失败：${errorMessage(welcomeResult.reason)}`)
    }

    setLoading(keepLoadingAfterBootstrap)
  }, [])

  const retryInitialReportRefresh = useCallback(async () => {
    setLoading(true)
    setReportRefreshing(true)
    setError(null)
    setReportRefreshError(null)
    try {
      const settled = await settleDeployCheckReport(await refreshDeployCheckReport())
      setReportRefreshError(null)
      if (settled.report) {
        setReport(settled.report)
      }
      setReportRefreshing(Boolean(settled.refreshing))
    } catch (e) {
      setReportRefreshError(errorMessage(e))
      setError(errorMessage(e))
      setReportRefreshing(false)
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

  const hasBlockingFailures = report
    ? hasBlockingDeployCheckFailure(report) || Boolean(reportRefreshError)
    : Boolean(reportRefreshError)

  async function enterDashboard() {
    if (hasBlockingFailures) return
    setSaving(true)
    setError(null)
    try {
      if (welcomeLoaded) {
        await putDeployWelcome({ neverAutoOpen })
      }
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
                  void retryInitialReportRefresh()
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
          {reportRefreshing ? <div className="deployWelcomeSubtitle">正在后台刷新最新检查结果…</div> : null}

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
          <DeployChecklistList items={groups.core} allChecks={report.checks} prefix="CORE" />
        </section>

        <section className="deployWelcomePanel">
          <div className="deploySectionHead">
            <h2>条件功能 Checklist（按启用状态）</h2>
            <p>功能未启用时显示 NA；启用后缺配置会标记 FAIL。</p>
          </div>
          <DeployChecklistList items={groups.feature} allChecks={report.checks} prefix="FEATURE" />
        </section>

        <section className="deployWelcomePanel deployWelcomeActionPanel">
          <div className="deployWelcomeActionLayout">
            <div className="deployWelcomeActionCopy">
              <div className="deployNeverAutoCheckbox">
                <Switch
                  id="deploy-never-auto-open"
                  checked={neverAutoOpen}
                  disabled={saving || !welcomeLoaded || hasBlockingFailures}
                  onChange={setNeverAutoOpen}
                />
                <Label htmlFor="deploy-never-auto-open">不再自动显示此页面</Label>
              </div>
              <p className="deployWelcomeActionHint">勾选后，后续访问首页将直接进入 Dashboard；可在设置页手动重新打开本页面。</p>
            </div>
            <div className="deployWelcomeActions">
              <Button variant="ghost" disabled={loading || saving} onClick={() => void refresh()}>
                重新检查
              </Button>
              <Button
                variant="primary"
                disabled={saving || hasBlockingFailures}
                onClick={() => void enterDashboard()}
              >
                {saving ? '保存中…' : '进入 Dashboard'}
              </Button>
            </div>
          </div>
          {error ? <div className="error">{error}</div> : null}
        </section>
      </main>
    </div>
  )
}

function DeployChecklistList(props: { items: DeployCheckItem[]; allChecks: DeployCheckItem[]; prefix: string }) {
  const { items, allChecks, prefix } = props
  if (items.length === 0) {
    return <div className="deployChecklistEmpty">暂无检查项</div>
  }

  return (
    <ol className="deployChecklistList">
      {items.map((item, index) => (
        <DeployChecklistItem key={item.id} item={item} allChecks={allChecks} number={`${prefix}-${index + 1}`} />
      ))}
    </ol>
  )
}

function DeployChecklistItem(props: { item: DeployCheckItem; allChecks: DeployCheckItem[]; number: string }) {
  const { item, allChecks, number } = props
  const warningNa = isWarningNa(item, allChecks)
  const recommendation = resolveRecommendation(item, allChecks, warningNa)
  const status = statusMeta(item.status)
  const statusDescription = warningNa ? '功能未启用（前置配置/条件缺失，建议尽快补齐）' : status.desc
  const rowClass = [
    'deployChecklistItem',
    warningNa ? 'deployChecklistItem--na-warning' : '',
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
            <span className={`deployBadge ${warningNa ? 'na-warning' : item.status}`}>{status.text}</span>
          </div>
        </div>

        <dl className="deployChecklistFacts">
          <div className="deployChecklistFact deployChecklistFact--summary">
            <dt>判定</dt>
            <dd>{item.summary}</dd>
          </div>
          <div className="deployChecklistFact deployChecklistFact--impact">
            <dt>影响</dt>
            <dd>{item.impact}</dd>
          </div>
          <div className="deployChecklistFact deployChecklistFact--recommendation">
            <dt>建议</dt>
            <dd>{recommendation}</dd>
          </div>
        </dl>

        <details className="deployChecklistDetails">
          <summary>展开证据与技术细节</summary>
          <dl className="deployChecklistFacts deployChecklistFactsSecondary">
            <div>
              <dt>ID</dt>
              <dd className="mono">{item.id}</dd>
            </div>
            <div>
              <dt>证据</dt>
              <dd className="mono">{item.evidence || '-'}</dd>
            </div>
            <div>
              <dt>说明</dt>
              <dd>{statusDescription}</dd>
            </div>
          </dl>
        </details>
      </div>
    </li>
  )
}
