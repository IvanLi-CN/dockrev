import type { Meta, StoryObj } from '@storybook/react'
import { AppShell } from '../../Shell'
import type { Route } from '../../routes'
import { Button } from '../../ui'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'
import { buildTopbarAuthIdentityFromSettings } from '../../topbarAuthIdentity'

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

const demoAuthIdentity = buildTopbarAuthIdentityFromSettings({
  allowAnonymousInDev: true,
  allowedGroupMasked: 'o**s',
  allowedUserMasked: 'al***ce',
  authorizationMode: 'user_or_group',
  currentGroups: ['o**s'],
  currentUser: 'alice',
  forwardHeaderName: 'X-Forwarded-User',
  groupHeaderName: 'Remote-Groups',
  matchedBy: 'user',
})

const meta: Meta<typeof AppShell> = {
  title: 'Layouts/AppShell',
  component: AppShell,
  decorators: [withDockrevMockApi],
  parameters: { dockrevApiScenario: 'default' },
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
        topActions={<Button variant="primary">Action</Button>}
        authIdentity={demoAuthIdentity}
        lastScanHint={new Date().toISOString()}
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
export const OverviewWithIdentityPopover: Story = {
  render: render({ name: 'overview' }),
  play: async ({ canvasElement }) => {
    const trigger = canvasElement.querySelector<HTMLButtonElement>('.topbarUserTrigger')
    expectStory(trigger?.textContent?.includes('alice'), 'AppShell topbar should show the current user trigger')
    trigger?.click()
    await new Promise((resolve) => setTimeout(resolve, 160))

    const doc = canvasElement.ownerDocument
    const popover = doc.querySelector<HTMLElement>('.topbarUserPopover')
    expectStory(popover?.textContent?.includes('Forward Auth'), 'AppShell topbar popover should expose auth source details')
  },
}
export const Queue: Story = { render: render({ name: 'queue' }) }
export const Services: Story = { render: render({ name: 'services' }) }
export const Settings: Story = { render: render({ name: 'settings' }) }
