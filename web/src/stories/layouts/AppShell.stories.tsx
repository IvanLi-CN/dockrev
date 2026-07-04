import { useEffect, type ReactNode } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { APP_SHELL_SIDEBAR_COLLAPSED_STORAGE_KEY, AppShell } from '../../Shell'
import type { Route } from '../../routes'
import { Button } from '../../ui'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'
import { buildTopbarAuthIdentityFromSettings } from '../../topbarAuthIdentity'

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function setSidebarCollapsedPreference(collapsed: boolean) {
  if (typeof window === 'undefined') return
  window.localStorage.setItem(APP_SHELL_SIDEBAR_COLLAPSED_STORAGE_KEY, collapsed ? '1' : '0')
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
  parameters: {
    dockrevApiScenario: 'default',
    layout: 'fullscreen',
  },
}

export default meta
type Story = StoryObj<typeof AppShell>

function ShellStory(props: {
  route: Route
  sidebarCollapsed?: boolean
  autoOpenMobileDrawer?: boolean
  children?: ReactNode
}) {
  setSidebarCollapsedPreference(props.sidebarCollapsed ?? false)

  useEffect(() => {
    if (!props.autoOpenMobileDrawer) return undefined
    const frame = window.requestAnimationFrame(() => {
      document.querySelector<HTMLButtonElement>('.mobileMenuButton')?.click()
    })
    return () => window.cancelAnimationFrame(frame)
  }, [props.autoOpenMobileDrawer])

  return (
    <AppShell
      route={props.route}
      title="示例页面"
      pageSubtitle="在 Storybook 中预览 AppShell"
      topActions={<Button variant="primary">Action</Button>}
      authIdentity={demoAuthIdentity}
      lastScanHint={new Date().toISOString()}
    >
      {props.children ?? (
        <div className="card">
          <div className="title">内容区</div>
          <div className="muted">这里是 page content</div>
        </div>
      )}
    </AppShell>
  )
}

function render(route: Route, options?: { sidebarCollapsed?: boolean; autoOpenMobileDrawer?: boolean }): Story['render'] {
  return () => {
    return (
      <ShellStory
        route={route}
        autoOpenMobileDrawer={options?.autoOpenMobileDrawer}
        sidebarCollapsed={options?.sidebarCollapsed}
      />
    )
  }
}

export const Overview: Story = { render: render({ name: 'overview' }) }
export const CollapsedSidebar: Story = {
  render: render({ name: 'services' }, { sidebarCollapsed: true }),
  play: async ({ canvasElement }) => {
    const shell = canvasElement.querySelector<HTMLElement>('.appShell')
    expectStory(shell?.classList.contains('appShellSidebarCollapsed'), 'AppShell should start collapsed')

    const navIcons = canvasElement.querySelectorAll('.navItemIcon')
    expectStory(navIcons.length === 5, 'Collapsed sidebar should render one real icon per nav item')

    const label = canvasElement.querySelector<HTMLElement>('.navItemLabel')
    expectStory(label, 'Collapsed sidebar should keep nav labels in the DOM for accessible names')
    const labelStyle = label.ownerDocument.defaultView?.getComputedStyle(label)
    expectStory(labelStyle?.display === 'none', 'Collapsed sidebar should visually hide nav labels')

    const activeLink = canvasElement.querySelector<HTMLAnchorElement>('.navItemActive')
    expectStory(activeLink?.getAttribute('aria-label') === '运维大盘', 'Collapsed active nav item should keep an aria-label')
  },
}
export const SidebarToggleInteraction: Story = {
  render: render({ name: 'overview' }),
  play: async ({ canvasElement }) => {
    const toggle = canvasElement.querySelector<HTMLButtonElement>('.sidebarCollapseButton')
    expectStory(toggle?.getAttribute('aria-expanded') === 'true', 'Sidebar toggle should report expanded state first')
    toggle?.click()
    await new Promise((resolve) => setTimeout(resolve, 80))

    const shell = canvasElement.querySelector<HTMLElement>('.appShell')
    expectStory(shell?.classList.contains('appShellSidebarCollapsed'), 'Sidebar toggle should collapse the shell')
    expectStory(toggle?.getAttribute('aria-expanded') === 'false', 'Sidebar toggle should report collapsed state')
    expectStory(
      window.localStorage.getItem(APP_SHELL_SIDEBAR_COLLAPSED_STORAGE_KEY) === '1',
      'Sidebar collapsed state should persist to localStorage',
    )
  },
}
export const MobileDrawerWithIcons: Story = {
  render: render({ name: 'overview' }, { sidebarCollapsed: true, autoOpenMobileDrawer: true }),
  parameters: {
    viewport: { defaultViewport: 'mobile1' },
  },
  play: async ({ canvasElement }) => {
    const shell = canvasElement.querySelector<HTMLElement>('.appShell')
    expectStory(shell?.classList.contains('appShellSidebarCollapsed'), 'Mobile story should preserve collapsed preference')

    const content = canvasElement.querySelector<HTMLElement>('.content')
    expectStory(
      content ? content.getBoundingClientRect().width > 300 : false,
      'Collapsed preference should not constrain mobile content to the desktop icon column',
    )

    const drawer = canvasElement.querySelector<HTMLElement>('#mobileDockrevMenu')
    expectStory(drawer?.querySelectorAll('.mobileNavIcon').length === 5, 'Mobile drawer should render one icon per nav item')
  },
}
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
