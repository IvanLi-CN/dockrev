import type { Meta, StoryObj } from '@storybook/react'
import { AppShell } from '../../Shell'
import type { Route } from '../../routes'
import { Button } from '../../ui'

const meta: Meta<typeof AppShell> = {
  title: 'Layouts/AppShell',
  component: AppShell,
}

export default meta
type Story = StoryObj<typeof AppShell>

function render(route: Route): Story['render'] {
  return () => {
    return (
      <AppShell
        route={route}
        title="示例页面"
        pageSubtitle="在 Storybook 中预览 AppShell"
        topbarHint="Compose 镜像更新 / 版本提示"
        topActions={<Button variant="primary">Action</Button>}
        composeHint={{ path: '/srv/prod/compose.yml', profile: 'prod', lastScan: new Date().toISOString() }}
      >
        <div className="card">
          <div className="title">内容区</div>
          <div className="muted">这里是 page content</div>
        </div>
      </AppShell>
    )
  }
}

export const Overview: Story = { render: render({ name: 'overview' }) }
export const Queue: Story = { render: render({ name: 'queue' }) }
export const Services: Story = { render: render({ name: 'services' }) }
export const Settings: Story = { render: render({ name: 'settings' }) }
