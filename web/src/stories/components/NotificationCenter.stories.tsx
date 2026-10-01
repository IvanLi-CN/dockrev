import type { Meta, StoryObj } from '@storybook/react'
import type { ReactNode } from 'react'
import { useEffect, useState } from 'react'
import { userEvent, within } from 'storybook/test'
import {
  NotificationCenter,
  NotificationNavigationOverride,
  NotificationProvider,
  useNotifications,
} from '../../NotificationCenter'
import { getNotificationInbox } from '../../notificationApi'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof NotificationCenter> = {
  title: 'Components/NotificationCenter',
  component: NotificationCenter,
  tags: ['autodocs'],
  decorators: [withDockrevMockApi],
  parameters: {
    dockrevApiScenario: 'default',
    layout: 'fullscreen',
    viewport: { defaultViewport: 'dockrevWide' },
  },
}

export default meta
type Story = StoryObj<typeof NotificationCenter>

function OpenNotificationCenter() {
  const { open } = useNotifications()
  useEffect(() => open(), [open])
  return <NotificationCenter />
}

function NotificationEvidenceSurface(props: { children: ReactNode }) {
  return (
    <div className="appShell" data-visual-evidence-surface="notification-center" style={{ minHeight: '100vh', background: 'var(--background)' }}>
      <header className="topbar">
        <div className="topbarMain">
          <div className="topbarRight">
            <div className="topbarNotificationCenter">{props.children}</div>
          </div>
        </div>
      </header>
    </div>
  )
}

function renderCenter() {
  return (
    <NotificationEvidenceSurface>
      <NotificationProvider>
        <OpenNotificationCenter />
      </NotificationProvider>
    </NotificationEvidenceSurface>
  )
}

function AcknowledgeBeforeNavigation() {
  const [result, setResult] = useState('')
  const onNavigate = async (target: string) => {
    const response = await getNotificationInbox({ limit: 50 })
    const item = response.items.find((candidate) => candidate.url === target)
    if (!item?.readAt) {
      setResult('打开时通知尚未标记已读')
    } else if (target === '/queue/job_story_aggregate') {
      setResult('已确认已读后打开完整清单')
    } else if (target === '/queue/job_story_ghcr') {
      setResult('已确认已读后打开审计任务')
    } else if (target.includes('/services/')) {
      setResult('已确认已读后打开服务详情')
    } else {
      setResult('已确认已读后打开任务详情')
    }
  }
  return (
    <NotificationEvidenceSurface>
      <NotificationNavigationOverride navigate={onNavigate}>
        <NotificationProvider>
          <OpenNotificationCenter />
          <output data-testid="notification-navigation-result">{result}</output>
        </NotificationProvider>
      </NotificationNavigationOverride>
    </NotificationEvidenceSurface>
  )
}

export const Desktop: Story = {
  render: renderCenter,
  parameters: { dockrevApiScenario: 'notification-details' },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    for (const action of ['查看任务详情', '查看服务详情', '查看完整清单', '查看审计任务']) {
      await canvas.findByText(action, { exact: true })
    }
    await canvas.findByText('支付 API：2.4.0 -> 2.5.0', { exact: true })
    await canvas.findByText('后台 Worker：1.8.2 -> 1.9.0', { exact: true })
    await canvas.findByText('另有 1 个服务有新版本。', { exact: true })
  },
}

export const Mobile: Story = {
  render: renderCenter,
  parameters: {
    dockrevApiScenario: 'notification-details',
    viewport: { defaultViewport: 'dockrevMobile' },
  },
}

export const ZeroUnread: Story = {
  render: renderCenter,
  parameters: { dockrevApiScenario: 'notification-empty' },
}

export const SingleUnread: Story = {
  render: renderCenter,
  parameters: { dockrevApiScenario: 'notification-single' },
}

export const AllRead: Story = {
  render: renderCenter,
  parameters: { dockrevApiScenario: 'notification-all-read' },
}

export const Error: Story = {
  render: renderCenter,
  parameters: { dockrevApiScenario: 'notification-error' },
}

export const Loading: Story = {
  render: renderCenter,
  parameters: {
    dockrevApiBehaviorByRoute: {
      'GET /api/notifications/unread-count': { delayMs: 900 },
      'GET /api/notifications/inbox': { delayMs: 900 },
    },
  },
}

export const MarkAllRead: Story = {
  render: renderCenter,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const markAllRead = await canvas.findByRole('button', { name: '全部标记已读' })
    await userEvent.click(markAllRead)
    for (let attempt = 0; attempt < 30; attempt += 1) {
      if (canvasElement.textContent?.includes('0 条未读')) return
      await new Promise((resolve) => setTimeout(resolve, 25))
    }
    throw new globalThis.Error('mark all read should clear the notification count')
  },
}

export const ReadBeforeNavigation: Story = {
  render: () => <AcknowledgeBeforeNavigation />,
  parameters: { dockrevApiScenario: 'notification-details' },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(await canvas.findByRole('button', { name: /更新任务已成功/ }))
    await canvas.findByText('已确认已读后打开任务详情')
  },
}

export const ServiceReadBeforeNavigation: Story = {
  render: () => <AcknowledgeBeforeNavigation />,
  parameters: { dockrevApiScenario: 'notification-details' },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await canvas.findByText('查看服务详情', { exact: true })
    await userEvent.click(await canvas.findByRole('button', { name: /发现 1 个新版本/ }))
    await canvas.findByText('已确认已读后打开服务详情')
  },
}

export const AggregateReadBeforeNavigation: Story = {
  render: () => <AcknowledgeBeforeNavigation />,
  parameters: { dockrevApiScenario: 'notification-details' },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(await canvas.findByRole('button', { name: /发现 3 个新版本/ }))
    await canvas.findByText('已确认已读后打开完整清单')
  },
}

export const GhcrReadBeforeNavigation: Story = {
  render: () => <AcknowledgeBeforeNavigation />,
  parameters: { dockrevApiScenario: 'notification-details' },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(await canvas.findByRole('button', { name: /GHCR Webhook 有 2 项异常/ }))
    await canvas.findByText('已确认已读后打开审计任务')
  },
}
