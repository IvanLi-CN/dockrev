import type { Meta, StoryObj } from '@storybook/react'
import { useEffect } from 'react'
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
