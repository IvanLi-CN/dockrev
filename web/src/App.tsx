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
      return { title: '系统设置', topbarHint: '系统设置' }
    case 'service':
      return { title: '服务详情', topbarHint: '服务详情' }
  }
}

export default function App() {
  const route = useRoute()
  const [topActions, setTopActions] = useState<ReactNode>(null)
  const [composeHint, setComposeHint] = useState<{ path?: string; profile?: string; lastScan?: string }>({})

  const head = useMemo(() => pageTitle(route), [route])

  return (
    <AppShell
      route={route}
      title={head.title}
      pageSubtitle={head.pageSubtitle}
      topbarHint={head.topbarHint}
      topActions={topActions}
      composeHint={composeHint}
    >
      {route.name === 'overview' ? <OverviewPage onComposeHint={setComposeHint} onTopActions={setTopActions} /> : null}
      {route.name === 'queue' ? <QueuePage onTopActions={setTopActions} /> : null}
      {route.name === 'services' ? <ServicesPage onComposeHint={setComposeHint} onTopActions={setTopActions} /> : null}
      {route.name === 'settings' ? <SettingsPage onTopActions={setTopActions} /> : null}
      {route.name === 'service' ? (
        <ServiceDetailPage
          stackId={route.stackId}
          serviceId={route.serviceId}
          onComposeHint={setComposeHint}
          onTopActions={setTopActions}
        />
      ) : null}
    </AppShell>
  )
}
