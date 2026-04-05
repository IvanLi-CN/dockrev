import './App.css'
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { AppShell } from './Shell'
import type { Route } from './routes'
import { navigate } from './routes'
import { OverviewPage } from './pages/OverviewPage'
import { QueuePage } from './pages/QueuePage'
import { JobDetailPage } from './pages/JobDetailPage'
import { ServicesPage } from './pages/ServicesPage'
import { CleanupPage } from './pages/CleanupPage'
import { ServiceDetailPage } from './pages/ServiceDetailPage'
import { SettingsPage } from './pages/SettingsPage'
import { VersionInferencePage } from './pages/VersionInferencePage'
import { GhcrWebhookQueuePage } from './pages/GhcrWebhookQueuePage'
import { GhcrWebhookInboxPage } from './pages/GhcrWebhookInboxPage'
import { GhcrWebhookRegistryPage } from './pages/GhcrWebhookRegistryPage'
import { SupervisorMisroutePage } from './pages/SupervisorMisroutePage'
import { brandMarkUrl } from './publicAssetUrls'
import { DeployWelcomePage } from './pages/DeployWelcomePage'
import { UnauthorizedPage } from './pages/UnauthorizedPage'
import { useRoute } from './useRoute'
import { usePageResumeRefresh } from './usePageResumeRefresh'
import {
  AUTH_RECOVERED_EVENT,
  AUTH_REQUIRED_EVENT,
  getDeployWelcome,
  getSettings,
  type AuthRequiredDetails,
} from './api'
import { TopbarUserIdentity } from './components/TopbarUserIdentity'
import {
  buildFallbackTopbarAuthIdentity,
  buildTopbarAuthIdentityFromAuthRequired,
  buildTopbarAuthIdentityFromSettings,
  type TopbarAuthIdentity,
} from './topbarAuthIdentity'

function pageTitle(route: Route): { title: string; pageSubtitle?: string; topbarHint?: string } {
  switch (route.name) {
    case 'overview':
      return {
        title: '概览',
        pageSubtitle: '聚焦：可更新 / 需提示（同前缀新版本）/ 架构不匹配 / 被阻止',
        topbarHint: 'Compose 镜像更新 / 版本提示',
      }
    case 'queue':
      return { title: '任务队列', topbarHint: '任务队列' }
    case 'job':
      return { title: '任务详情', topbarHint: '任务队列' }
    case 'services':
      return { title: '服务', topbarHint: '服务' }
    case 'cleanup':
      return {
        title: '清理',
        pageSubtitle: '按规则预览 docker prune 候选，支持全局 / Stack / 服务三级清理',
        topbarHint: 'Docker 清理控制台',
      }
    case 'version-inference':
      return {
        title: '版本推测',
        pageSubtitle: '镜像版本推测任务与缓存状态总览',
        topbarHint: '版本推测可观测性',
      }
    case 'ghcr-webhooks':
      return {
        title: 'GHCR Webhook',
        pageSubtitle: 'Webhook 注册/反注册任务与巡检状态',
        topbarHint: 'GHCR Webhook 队列',
      }
    case 'ghcr-webhook-inbox':
      return {
        title: 'Webhook 收件箱',
        pageSubtitle: '按 delivery 展示事件、处理结果与响应状态',
        topbarHint: '任务队列',
      }
    case 'ghcr-webhook-registry':
      return {
        title: 'GHCR Webhook 维护',
        pageSubtitle: '集中维护仓库 webhook 注册状态、重试注册与删除反注册任务',
        topbarHint: '系统设置',
      }
    case 'deploy-check':
      return {
        title: '部署检查',
        pageSubtitle: '面向运维：按功能能力判定配置完整性（PASS/FAIL）',
        topbarHint: '部署检查',
      }
    case 'settings':
      return {
        title: '系统设置',
        pageSubtitle: 'Forward Auth · 用户/组鉴权 · 通知配置 · 备份默认策略',
        topbarHint: '系统设置',
      }
    case 'service':
      return { title: '服务详情', topbarHint: '服务详情' }
    case 'supervisor-misroute':
      return { title: '部署问题', topbarHint: '自我升级（Supervisor）' }
  }
}

export default function App() {
  const route = useRoute()
  const [pageActions, setPageActions] = useState<ReactNode>(null)
  const [lastScanHint, setLastScanHint] = useState<string | undefined>(undefined)
  const [deployWelcomeState, setDeployWelcomeState] = useState<{ loaded: boolean; neverAutoOpen: boolean }>({
    loaded: false,
    neverAutoOpen: true,
  })
  const [authFailure, setAuthFailure] = useState<AuthRequiredDetails | null>(null)
  const [authIdentity, setAuthIdentity] = useState<TopbarAuthIdentity>(() => buildFallbackTopbarAuthIdentity())
  const authFailureActiveRef = useRef(false)
  const authFailureVersionRef = useRef(0)
  const authIdentityRefreshInFlightRef = useRef(false)
  const suppressNextAuthRecoveredRef = useRef(false)

  const head = useMemo(() => pageTitle(route), [route])
  const topActions = useMemo(() => {
    return <>{pageActions}</>
  }, [pageActions])
  const refreshAuthIdentity = useCallback(async () => {
    if (authIdentityRefreshInFlightRef.current) return null
    authIdentityRefreshInFlightRef.current = true
    suppressNextAuthRecoveredRef.current = true
    const authFailureVersionAtStart = authFailureVersionRef.current
    try {
      const settings = await getSettings()
      if (authFailureActiveRef.current || authFailureVersionAtStart !== authFailureVersionRef.current) {
        return null
      }
      return buildTopbarAuthIdentityFromSettings(settings.auth)
    } finally {
      authIdentityRefreshInFlightRef.current = false
      suppressNextAuthRecoveredRef.current = false
    }
  }, [])
  const requestAuthIdentityRefresh = usePageResumeRefresh(async () => {
    const nextAuthIdentity = await refreshAuthIdentity()
    if (!nextAuthIdentity) return
    setAuthIdentity(nextAuthIdentity)
  }, { onError: () => {} })

  useEffect(() => {
    let cancelled = false
    void getDeployWelcome()
      .then((settings) => {
        if (cancelled) return
        setDeployWelcomeState({ loaded: true, neverAutoOpen: settings.neverAutoOpen })
      })
      .catch(() => {
        if (cancelled) return
        // Fail open to dashboard when the preference endpoint is unavailable.
        setDeployWelcomeState({ loaded: true, neverAutoOpen: true })
      })
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    if (!deployWelcomeState.loaded || deployWelcomeState.neverAutoOpen) return
    if (route.name !== 'overview') return
    if (typeof window === 'undefined') return
    const key = 'dockrev:deployWelcome:redirected'
    if (window.sessionStorage.getItem(key) === '1') return
    window.sessionStorage.setItem(key, '1')
    navigate({ name: 'deploy-check' })
  }, [deployWelcomeState, route.name])

  useEffect(() => {
    if (typeof window === 'undefined') return

    const onAuthRequired = (event: Event) => {
      const detail = (event as CustomEvent<{ details?: AuthRequiredDetails | null }>).detail
      const nextAuthFailure = detail?.details ?? null
      authFailureActiveRef.current = true
      authFailureVersionRef.current += 1
      setAuthFailure(nextAuthFailure)
      if (nextAuthFailure) {
        setAuthIdentity(buildTopbarAuthIdentityFromAuthRequired(nextAuthFailure))
      } else {
        setAuthIdentity(buildFallbackTopbarAuthIdentity())
      }
    }
    const onAuthRecovered = () => {
      const hadAuthFailure = authFailureActiveRef.current
      authFailureActiveRef.current = false
      setAuthFailure(null)
      if (suppressNextAuthRecoveredRef.current) {
        suppressNextAuthRecoveredRef.current = false
        return
      }
      if (!hadAuthFailure) return
      void requestAuthIdentityRefresh().catch(() => {})
    }

    window.addEventListener(AUTH_REQUIRED_EVENT, onAuthRequired)
    window.addEventListener(AUTH_RECOVERED_EVENT, onAuthRecovered)
    return () => {
      window.removeEventListener(AUTH_REQUIRED_EVENT, onAuthRequired)
      window.removeEventListener(AUTH_RECOVERED_EVENT, onAuthRecovered)
    }
  }, [requestAuthIdentityRefresh])

  useEffect(() => {
    let cancelled = false
    void refreshAuthIdentity()
      .then((nextAuthIdentity) => {
        if (cancelled || !nextAuthIdentity) return
        setAuthIdentity(nextAuthIdentity)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [refreshAuthIdentity])

  if (route.name === 'supervisor-misroute') {
    return (
      <div className="standaloneShell">
        <div className="standaloneContent">
          <div className="standaloneHead">
            <div className="standaloneHeadLeft">
              <div className="brand">
                <img className="brandMark" src={brandMarkUrl} alt="" aria-hidden="true" />
                Dockrev
              </div>
            </div>
            <div className="standaloneHeadRight">
              <TopbarUserIdentity authIdentity={authIdentity} />
            </div>
          </div>
          <SupervisorMisroutePage basePath={route.basePath} pathname={route.pathname} />
        </div>
      </div>
    )
  }

  if (authFailure) {
    return (
      <AppShell
        route={route}
        title={head.title}
        pageSubtitle={head.pageSubtitle}
        topbarHint={head.topbarHint}
        topActions={null}
        authIdentity={authIdentity}
        lastScanHint={lastScanHint}
      >
        <UnauthorizedPage authDetails={authFailure} />
      </AppShell>
    )
  }

  if (route.name === 'deploy-check') {
    return <DeployWelcomePage />
  }

  return (
    <AppShell
      route={route}
      title={head.title}
      pageSubtitle={head.pageSubtitle}
      topbarHint={head.topbarHint}
      topActions={topActions}
      authIdentity={authIdentity}
      lastScanHint={lastScanHint}
    >
      {route.name === 'overview' ? <OverviewPage onLastScanHint={setLastScanHint} onTopActions={setPageActions} /> : null}
      {route.name === 'queue' ? <QueuePage onTopActions={setPageActions} /> : null}
      {route.name === 'job' ? <JobDetailPage jobId={route.jobId} onTopActions={setPageActions} /> : null}
      {route.name === 'services' ? <ServicesPage onLastScanHint={setLastScanHint} onTopActions={setPageActions} /> : null}
      {route.name === 'cleanup' ? <CleanupPage onLastScanHint={setLastScanHint} onTopActions={setPageActions} /> : null}
      {route.name === 'version-inference' ? (
        <VersionInferencePage onLastScanHint={setLastScanHint} onTopActions={setPageActions} />
      ) : null}
      {route.name === 'ghcr-webhooks' ? <GhcrWebhookQueuePage onTopActions={setPageActions} /> : null}
      {route.name === 'ghcr-webhook-inbox' ? <GhcrWebhookInboxPage onTopActions={setPageActions} /> : null}
      {route.name === 'ghcr-webhook-registry' ? <GhcrWebhookRegistryPage onTopActions={setPageActions} /> : null}
      {route.name === 'settings' ? <SettingsPage onTopActions={setPageActions} /> : null}
      {route.name === 'service' ? (
        <ServiceDetailPage
          stackId={route.stackId}
          serviceId={route.serviceId}
          onLastScanHint={setLastScanHint}
          onTopActions={setPageActions}
        />
      ) : null}
    </AppShell>
  )
}
