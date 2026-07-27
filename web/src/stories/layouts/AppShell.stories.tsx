import { useEffect, type ReactNode } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { APP_SHELL_SIDEBAR_COLLAPSED_STORAGE_KEY, AppShell } from '../../Shell'
import { DetailRouteServiceTree } from '../../components/DetailRouteServiceTree'
import type { Route } from '../../routes'
import { Button } from '../../ui'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'
import { buildTopbarAuthIdentityFromSettings } from '../../topbarAuthIdentity'

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function expectDesktopActiveNavBaseline(root: ParentNode) {
  const activeLink = root.querySelector<HTMLAnchorElement>('.navItemActive')
  expectStory(activeLink, 'Desktop sidebar should expose one active nav item')

  const view = activeLink.ownerDocument.defaultView
  const activeStyle = view?.getComputedStyle(activeLink)
  expectStory(activeStyle?.backgroundImage !== 'none', 'Active desktop nav item should keep the collapsible-sidebar gradient fill')
  expectStory(
    !!activeStyle?.boxShadow?.includes('inset') && activeStyle.boxShadow.includes('0px 0px 0px 1px'),
    'Active desktop nav item should keep the full inset outline instead of a left accent bar',
  )
  expectStory(
    activeStyle?.borderTopColor !== 'rgba(0, 0, 0, 0)',
    'Active desktop nav item should keep the highlighted border color',
  )

  const activeIcon = activeLink.querySelector<HTMLElement>('.navItemIcon')
  const activeIconStyle = activeIcon ? view?.getComputedStyle(activeIcon) : undefined
  expectStory(
    !!activeIconStyle && activeIconStyle.color !== activeStyle?.color,
    'Active desktop nav icon should keep its dedicated primary accent color',
  )
}

function expectDesktopHeaderAlignment(root: ParentNode, detail = false) {
  const headerWorkspace = root.querySelector<HTMLElement>('.topbarMain')
  const content = root.querySelector<HTMLElement>('.content')
  expectStory(headerWorkspace && content, 'AppShell should render a header workspace and content region')

  const headerLeft = headerWorkspace.getBoundingClientRect().left
  const contentLeft = content.getBoundingClientRect().left
  expectStory(Math.abs(headerLeft - contentLeft) <= 1, 'Header workspace should start on the main route column boundary')

  if (!detail) return
  const detailSidebar = root.querySelector<HTMLElement>('.detailSidebar')
  expectStory(detailSidebar, 'Detail shell should render its service navigation rail')
  expectStory(
    Math.abs(detailSidebar.getBoundingClientRect().right - contentLeft) <= 1,
    'Header workspace should start after the service navigation rail',
  )
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
  detailSidebarContent?: ReactNode
  mobileNavContent?: ReactNode
  mobileDrawerTitle?: string
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
      detailSidebarContent={props.detailSidebarContent}
      detailSidebarTitle={undefined}
      mobileNavContent={props.mobileNavContent}
      mobileDrawerTitle={props.mobileDrawerTitle}
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

function renderDetailShell(
  route: Extract<Route, { name: 'stack' | 'service' }>,
  options?: { autoOpenMobileDrawer?: boolean },
): Story['render'] {
  return () => (
    <ShellStory
      route={route}
      autoOpenMobileDrawer={options?.autoOpenMobileDrawer}
      detailSidebarContent={<DetailRouteServiceTree route={route} variant="desktop" />}
      mobileNavContent={<DetailRouteServiceTree route={route} variant="mobile" />}
      mobileDrawerTitle="服务导航"
    />
  )
}

export const Overview: Story = {
  render: render({ name: 'overview' }),
  play: async ({ canvasElement }) => {
    expectDesktopActiveNavBaseline(canvasElement)
    expectDesktopHeaderAlignment(canvasElement)
  },
}
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
    expectDesktopActiveNavBaseline(canvasElement)
    expectDesktopHeaderAlignment(canvasElement)

    const desktopBrand = canvasElement.querySelector<HTMLElement>('.topbarDesktopBrand')
    const headerWorkspace = canvasElement.querySelector<HTMLElement>('.topbarMain')
    expectStory(desktopBrand && headerWorkspace, 'Collapsed shell should render the desktop brand and workspace')
    expectStory(
      desktopBrand.getBoundingClientRect().right <= headerWorkspace.getBoundingClientRect().left + 1,
      'Collapsed desktop brand should stay inside the primary navigation header track',
    )
    expectStory(
      desktopBrand.querySelector('[role="img"]')?.getAttribute('aria-label') === 'Dockrev',
      'Desktop brand should retain its accessible name',
    )

    const identityTrigger = canvasElement.querySelector<HTMLButtonElement>('.sidebarMeta .topbarUserTrigger')
    expectStory(identityTrigger, 'Collapsed sidebar should keep the user identity trigger available')
    expectStory(identityTrigger?.getAttribute('aria-label')?.includes('alice'), 'Collapsed identity trigger should retain its accessible name')
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
export const DetailSidebarDesktop: Story = {
  render: renderDetailShell({ name: 'service', stackId: 'stack-prod', serviceId: 'svc-prod-api', section: 'logs' }),
  play: async ({ canvasElement }) => {
    const detailSidebar = canvasElement.querySelector<HTMLElement>('.detailSidebar')
    expectStory(detailSidebar?.textContent?.includes('prod'), 'Detail sidebar should render stack names')
    expectStory(detailSidebar?.textContent?.includes('api'), 'Detail sidebar should render service names')
    expectStory(detailSidebar?.textContent?.includes('web'), 'Detail sidebar should render sibling services')
    expectDesktopHeaderAlignment(canvasElement, true)
  },
}
export const MobileBottomNavAndDrawer: Story = {
  render: renderDetailShell(
    { name: 'service', stackId: 'stack-prod', serviceId: 'svc-prod-api', section: 'logs' },
    { autoOpenMobileDrawer: true },
  ),
  parameters: {
    viewport: { defaultViewport: 'mobile1' },
  },
  play: async ({ canvasElement }) => {
    const bottomNavItems = canvasElement.ownerDocument.querySelectorAll('.mobileBottomNavItem')
    expectStory(bottomNavItems.length === 5, 'Mobile shell should move primary navigation into bottom nav')

    const drawer = canvasElement.querySelector<HTMLElement>('#mobileDockrevMenu')
    expectStory(drawer?.textContent?.includes('服务导航'), 'Mobile drawer should be dedicated to the service tree')
    expectStory(drawer?.textContent?.includes('prod'), 'Mobile drawer should render stack names')
    expectStory(drawer?.textContent?.includes('api'), 'Mobile drawer should render service names')

    const identityTrigger = canvasElement.querySelector<HTMLButtonElement>('.topbarUserSlotTopbar .topbarUserTrigger')
    expectStory(identityTrigger, 'Mobile header should retain the user identity trigger')
  },
}
export const OverviewWithSidebarIdentityPopover: Story = {
  render: render({ name: 'overview' }),
  play: async ({ canvasElement }) => {
    const trigger = canvasElement.querySelector<HTMLButtonElement>('.sidebarMeta .topbarUserTrigger')
    expectStory(trigger?.textContent?.includes('alice'), 'AppShell sidebar should show the current user trigger')

    const topbarIdentity = canvasElement.querySelector<HTMLElement>('.topbarUserSlotTopbar')
    expectStory(
      topbarIdentity?.ownerDocument.defaultView?.getComputedStyle(topbarIdentity).display === 'none',
      'Desktop header should not duplicate the user identity trigger',
    )
    trigger?.click()
    await new Promise((resolve) => setTimeout(resolve, 160))

    const doc = canvasElement.ownerDocument
    const popover = doc.querySelector<HTMLElement>('.topbarUserPopover')
    expectStory(popover?.textContent?.includes('Forward Auth'), 'AppShell sidebar popover should expose auth source details')
  },
}
export const Queue: Story = { render: render({ name: 'queue' }) }
export const Services: Story = { render: render({ name: 'services' }) }
export const Settings: Story = { render: render({ name: 'settings' }) }
