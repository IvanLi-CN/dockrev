import type { Meta, StoryObj } from '@storybook/react'
import { useEffect } from 'react'
import { userEvent, within } from 'storybook/test'
import { NotificationCenter, NotificationProvider, useNotifications } from '../../NotificationCenter'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof NotificationCenter> = {
  title: 'Components/NotificationCenter',
  component: NotificationCenter,
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

function renderCenter() {
  return (
    <div style={{ minHeight: '100vh', padding: 24 }}>
      <NotificationProvider>
        <OpenNotificationCenter />
      </NotificationProvider>
    </div>
  )
}

export const Desktop: Story = {
  render: renderCenter,
}

export const Mobile: Story = {
  render: renderCenter,
  parameters: {
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
