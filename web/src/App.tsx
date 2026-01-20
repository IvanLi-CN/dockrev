import './App.css'
import { useMemo, useState, type ReactNode } from 'react'
import { AppShell } from './Shell'
import type { Route } from './routes'
import { OverviewPage } from './pages/OverviewPage'
import { QueuePage } from './pages/QueuePage'
import { ServicesPage } from './pages/ServicesPage'
import { ServiceDetailPage } from './pages/ServiceDetailPage'
import { SettingsPage } from './pages/SettingsPage'
import { useRoute } from './useRoute'
import { Switch } from './ui'
import { getTheme, setTheme, type DockrevTheme } from './theme'

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
    case 'services':
      return { title: '服务', topbarHint: '服务' }
    case 'settings':
      return {
        title: '系统设置',
        pageSubtitle: '单用户 / Forward Header · 认证配置 · 通知配置 · 备份默认策略',
        topbarHint: '系统设置',
      }
    case 'service':
      return { title: '服务详情', topbarHint: '服务详情' }
  }
}

export default function App() {
  const route = useRoute()
  const [pageActions, setPageActions] = useState<ReactNode>(null)
  const [composeHint, setComposeHint] = useState<{ path?: string; profile?: string; lastScan?: string }>({})
  const [theme, setThemeState] = useState<DockrevTheme>(() => getTheme())

  const head = useMemo(() => pageTitle(route), [route])
  const topActions = useMemo(() => {
    return (
      <>
        {pageActions}
        <div className="themeToggle">
          <span className="muted">主题</span>
          <Switch
            checked={theme === 'dark'}
            onChange={(checked) => {
              const next: DockrevTheme = checked ? 'dark' : 'light'
              setTheme(next)
              setThemeState(next)
            }}
          />
        </div>
      </>
    )
  }, [pageActions, theme])

  return (
    <AppShell
      route={route}
      title={head.title}
      pageSubtitle={head.pageSubtitle}
      topbarHint={head.topbarHint}
      topActions={topActions}
      composeHint={composeHint}
    >
      {route.name === 'overview' ? <OverviewPage onComposeHint={setComposeHint} onTopActions={setPageActions} /> : null}
      {route.name === 'queue' ? <QueuePage onTopActions={setPageActions} /> : null}
      {route.name === 'services' ? <ServicesPage onComposeHint={setComposeHint} onTopActions={setPageActions} /> : null}
      {route.name === 'settings' ? <SettingsPage onTopActions={setPageActions} /> : null}
      {route.name === 'service' ? (
        <ServiceDetailPage
          stackId={route.stackId}
          serviceId={route.serviceId}
          onComposeHint={setComposeHint}
          onTopActions={setPageActions}
        />
      ) : null}
    </AppShell>
  )
}
