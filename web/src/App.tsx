import './App.css'
import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { AppShell } from './Shell'
import type { Route } from './routes'
import { navigate } from './routes'
import { OverviewPage } from './pages/OverviewPage'
import { QueuePage } from './pages/QueuePage'
import { JobDetailPage } from './pages/JobDetailPage'
import { ServicesPage } from './pages/ServicesPage'
import { ServiceDetailPage } from './pages/ServiceDetailPage'
import { SettingsPage } from './pages/SettingsPage'
import { VersionInferencePage } from './pages/VersionInferencePage'
import { GhcrWebhookQueuePage } from './pages/GhcrWebhookQueuePage'
import { GhcrWebhookInboxPage } from './pages/GhcrWebhookInboxPage'
import { SupervisorMisroutePage } from './pages/SupervisorMisroutePage'
import { DeployWelcomePage } from './pages/DeployWelcomePage'
import { useRoute } from './useRoute'
import { getDeployWelcome } from './api'

function pageTitle(route: Route): { title: string; pageSubtitle?: string; topbarHint?: string } {
  switch (route.name) {
    case 'overview':
      return {
        title: '概览',
        pageSubtitle: '聚焦：可更新 / 需提示（同前缀新版本）/ 架构不匹配 / 被阻止',
        topbarHint: 'Compose 镜像更新 / 版本提示',
      }
    case 'queue':
      return { title: '更新队列', topbarHint: '更新队列' }
    case 'job':
      return { title: '任务详情', topbarHint: '更新队列' }
    case 'services':
      return { title: '服务', topbarHint: '服务' }
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
        title: 'Webhook Inbox',
        pageSubtitle: 'Webhook 触发记录列表（delivery）',
        topbarHint: '更新队列',
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
        pageSubtitle: '单用户 / Forward Header · 认证配置 · 通知配置 · 备份默认策略',
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

  const head = useMemo(() => pageTitle(route), [route])
  const topActions = useMemo(() => {
    return <>{pageActions}</>
  }, [pageActions])

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

  if (route.name === 'supervisor-misroute') {
    return (
      <div className="standaloneShell">
        <div className="standaloneContent">
          <div className="standaloneHead">
            <div className="standaloneHeadLeft">
              <div className="brand">
                <img className="brandMark" src="/brand-mark.png" alt="" aria-hidden="true" />
                Dockrev
              </div>
            </div>
            <div className="chipStatic chipStaticUser">用户：ivan（FH）</div>
          </div>
          <SupervisorMisroutePage basePath={route.basePath} pathname={route.pathname} />
        </div>
      </div>
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
      lastScanHint={lastScanHint}
    >
      {route.name === 'overview' ? <OverviewPage onLastScanHint={setLastScanHint} onTopActions={setPageActions} /> : null}
      {route.name === 'queue' ? <QueuePage onTopActions={setPageActions} /> : null}
      {route.name === 'job' ? <JobDetailPage jobId={route.jobId} onTopActions={setPageActions} /> : null}
      {route.name === 'services' ? <ServicesPage onLastScanHint={setLastScanHint} onTopActions={setPageActions} /> : null}
      {route.name === 'version-inference' ? (
        <VersionInferencePage onLastScanHint={setLastScanHint} onTopActions={setPageActions} />
      ) : null}
      {route.name === 'ghcr-webhooks' ? <GhcrWebhookQueuePage onTopActions={setPageActions} /> : null}
      {route.name === 'ghcr-webhook-inbox' ? <GhcrWebhookInboxPage onTopActions={setPageActions} /> : null}
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
