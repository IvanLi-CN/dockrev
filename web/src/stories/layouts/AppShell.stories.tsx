import { useEffect, type ReactNode } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { AppShell } from '../../Shell'
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

  if (detail) {
    expectStory(!root.querySelector('.detailSidebar'), 'Detail routes should use the single sidebar context slot')
  }
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
  autoOpenMobileDrawer?: boolean
  contextNavigation?: ReactNode
  mobileDrawerTitle?: string
  children?: ReactNode
}) {
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
      contextNavigation={props.contextNavigation}
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

function render(route: Route, options?: { autoOpenMobileDrawer?: boolean }): Story['render'] {
  return () => {
    return (
      <ShellStory
        route={route}
        autoOpenMobileDrawer={options?.autoOpenMobileDrawer}
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
      contextNavigation={<DetailRouteServiceTree route={route} variant="desktop" />}
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

export const UpdateReadyBubble: Story = {
  render: render({ name: 'overview' }),
  parameters: {
    pwaStatus: {
      updatePhase: 'ready',
      updatePromptVisible: true,
    },
  },
  play: async ({ canvasElement }) => {
    const bubble = canvasElement.querySelector<HTMLElement>('.pwaUpdateBubble')
    expectStory(bubble, 'Ready updates should render outside the content scroll area')
    expectStory(!canvasElement.querySelector('.shellStatusBanner-update'), 'Ready updates should not consume content flow')
  },
}

export const UpdateReadyBubbleMobile: Story = {
  render: render({ name: 'overview' }),
  parameters: {
    pwaStatus: {
      updatePhase: 'ready',
      updatePromptVisible: true,
    },
    viewport: { defaultViewport: 'mobile1' },
  },
  play: async ({ canvasElement }) => {
    const bubble = canvasElement.querySelector<HTMLElement>('.pwaUpdateBubble')
    const bottomNav = canvasElement.querySelector<HTMLElement>('.mobileBottomNav')
    expectStory(bubble && bottomNav, 'Mobile shell should render both the update bubble and bottom navigation')
    expectStory(
      bubble.getBoundingClientRect().bottom <= bottomNav.getBoundingClientRect().top,
      'Mobile update bubble should clear the bottom navigation',
    )
    expectStory(
      bubble.scrollWidth <= bubble.clientWidth,
      'Mobile update bubble should not overflow at the story viewport width',
    )
  },
}
export const SingleSidebarContext: Story = {
  render: render({ name: 'services' }),
  play: async ({ canvasElement }) => {
    const shell = canvasElement.querySelector<HTMLElement>('.appShell')
    expectStory(shell && !shell.classList.contains('appShellSidebarCollapsed'), 'AppShell should not expose a collapsed state')
    const navIcons = canvasElement.querySelectorAll('.navItemIcon')
    expectStory(navIcons.length === 5, 'Single sidebar should render one real icon per nav item')
    expectStory(Boolean(canvasElement.querySelector('.topbarIdentity .brandLogo')), 'Logo should render in the page header')
    expectStory(!canvasElement.querySelector('.sidebarBrand'), 'Sidebar should not duplicate the page header Logo')
    const primaryNav = canvasElement.querySelector<HTMLElement>('#appShellPrimaryNav')
    expectStory(primaryNav && getComputedStyle(primaryNav).gridTemplateColumns.split(' ').length === 5, 'Desktop primary navigation should be a five-icon horizontal row')
    expectStory(Boolean(canvasElement.querySelector('.sidebarContextViewport')), 'Single sidebar should expose a context viewport')
    expectStory(!canvasElement.querySelector('.sidebarCollapseButton'), 'Single sidebar should not render a collapse control')
  },
}
export const SidebarToggleInteraction: Story = {
  render: render({ name: 'overview' }),
  play: async ({ canvasElement }) => {
    expectStory(!canvasElement.querySelector('.sidebarCollapseButton'), 'The unified shell should have no toggle interaction')
    expectStory(canvasElement.querySelectorAll('.sidebar').length === 1, 'The unified shell should render one sidebar')
  },
}
export const DetailSidebarDesktop: Story = {
  render: renderDetailShell({ name: 'service', stackId: 'stack-prod', serviceId: 'svc-prod-api', section: 'logs' }),
  play: async ({ canvasElement }) => {
    const context = canvasElement.querySelector<HTMLElement>('.sidebarContextViewport')
    expectStory(context?.textContent?.includes('prod'), 'Sidebar context should render stack names')
    expectStory(context?.textContent?.includes('api'), 'Sidebar context should render service names')
    expectStory(context?.textContent?.includes('web'), 'Sidebar context should render sibling services')
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
    expectStory(Boolean(drawer?.querySelector('.mobileDrawerFooter .topbarUserTrigger')), 'Mobile drawer should keep the user identity in footer ③')
    expectStory(Boolean(drawer?.querySelector('.mobileDrawerFooter .themePreferenceSegmented')), 'Mobile drawer should keep the theme control in footer ③')
    expectStory(Boolean(drawer?.querySelector('.mobileDrawerFooter .sidebarAppMetaContent')), 'Mobile drawer should keep version metadata in footer ③')

    expectStory(
      !canvasElement.querySelector('.topbarUserSlotTopbar'),
      'Mobile header should move the user identity entry into settings',
    )
    expectStory(
      !canvasElement.querySelector('.topbarRight .themePreferenceIconButton'),
      'Mobile business pages should not mount the theme control outside settings',
    )
    expectStory(
      !canvasElement.querySelector('.topbarUserSlotSidebar'),
      'Mobile AppShell should not mount the desktop sidebar identity trigger',
    )
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
export const Settings: Story = {
  render: render({ name: 'settings' }),
  play: async ({ canvasElement }) => {
    expectStory(
      Boolean(canvasElement.querySelector('.sidebarThemeControl .themePreferenceSegmented')),
      'Expanded desktop sidebar should expose the three-state theme slider',
    )
  },
}
export const SettingsMobileThemeControl: Story = {
  render: render({ name: 'settings' }),
  parameters: { viewport: { defaultViewport: 'mobile1' } },
  play: async ({ canvasElement }) => {
    expectStory(
      Boolean(canvasElement.querySelector('.topbarRight .themePreferenceIconButton')),
      'Mobile settings should expose the theme icon in the topbar',
    )
    expectStory(
      !canvasElement.querySelector('.sidebarThemeControl'),
      'Mobile settings should not mount the desktop sidebar theme control',
    )
  },
}
