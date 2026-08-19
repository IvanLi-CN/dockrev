import type { Meta, StoryObj } from '@storybook/react'
import { SidebarAppMeta } from '../../components/SidebarAppMeta'

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function wait(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function StorySurface(props: {
  collapsed: boolean
  versionDisplay: string
  versionHref: string | null
}) {
  return (
    <div
      style={{
        display: 'inline-flex',
        width: props.collapsed ? 72 : 292,
        margin: 24,
        padding: props.collapsed ? '20px 14px' : '20px 24px',
        border: '1px solid var(--borderColor)',
        borderRadius: 10,
        background: 'color-mix(in srgb, var(--panel2) 88%, var(--color-primary) 12%)',
      }}
    >
      <SidebarAppMeta {...props} />
    </div>
  )
}

const meta: Meta<typeof StorySurface> = {
  title: 'Components/SidebarAppMeta',
  component: StorySurface,
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof StorySurface>

export const Expanded: Story = {
  args: {
    collapsed: false,
    versionDisplay: 'v0.74.2',
    versionHref: 'https://github.com/IvanLi-CN/dockrev/releases/tag/0.74.2',
  },
  play: async ({ canvasElement }) => {
    const slot = canvasElement.querySelector<HTMLElement>('[data-slot="sidebar-app-meta"]')
    const release = canvasElement.querySelector<HTMLAnchorElement>('.sidebarAppMetaVersion')
    const repository = canvasElement.querySelector<HTMLAnchorElement>('.sidebarAppMetaGithub')
    expectStory(slot && Math.abs(slot.getBoundingClientRect().height - 44) <= 1, 'Expanded App meta slot should be 44px high')
    expectStory(release?.href.endsWith('/releases/tag/0.74.2'), 'Version should link to the matching GitHub release')
    expectStory(repository?.rel.includes('noopener'), 'Repository link should retain safe external-link semantics')
    expectStory(canvasElement.textContent?.includes('Powered by Ivan Li'), 'Expanded content should include the attribution')
  },
}

export const VersionUnavailable: Story = {
  args: {
    collapsed: false,
    versionDisplay: '-',
    versionHref: null,
  },
  play: async ({ canvasElement }) => {
    const version = canvasElement.querySelector<HTMLElement>('.sidebarAppMetaVersionDisabled')
    expectStory(version?.textContent === '-', 'Unavailable version should render the stable dash fallback')
    expectStory(!canvasElement.querySelector('.sidebarAppMetaVersion[href]'), 'Unavailable version should not render a release link')
  },
}

export const CollapsedFlyout: Story = {
  args: {
    collapsed: true,
    versionDisplay: 'v0.74.2',
    versionHref: 'https://github.com/IvanLi-CN/dockrev/releases/tag/0.74.2',
  },
  play: async ({ canvasElement }) => {
    const trigger = canvasElement.querySelector<HTMLButtonElement>('.sidebarAppMetaTrigger')
    expectStory(trigger, 'Collapsed App meta should render one trigger')
    expectStory(Math.abs(trigger.getBoundingClientRect().height - 44) <= 1, 'Collapsed App meta trigger should be 44px high')
    expectStory(trigger.getAttribute('aria-label')?.includes('v0.74.2'), 'Collapsed trigger should retain version context')

    trigger.dispatchEvent(new PointerEvent('pointerover', { bubbles: true, pointerType: 'mouse' }))
    await wait(100)
    const doc = canvasElement.ownerDocument
    let flyout = doc.querySelector<HTMLElement>('.sidebarAppMetaPopover')
    expectStory(flyout?.getAttribute('data-side') === 'right', 'Flyout should open to the right through the portal')
    expectStory(flyout?.getAttribute('aria-label')?.includes('release'), 'Flyout dialog should have an accessible name')
    expectStory(flyout?.textContent?.includes('Powered by Ivan Li'), 'Flyout should reuse the expanded content')

    trigger.dispatchEvent(new PointerEvent('pointerout', { bubbles: true, pointerType: 'mouse' }))
    await wait(360)
    expectStory(!doc.querySelector('.sidebarAppMetaPopover'), 'Unpinned flyout should close after pointer leave')

    trigger.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerType: 'mouse' }))
    trigger.click()
    await wait(100)
    flyout = doc.querySelector<HTMLElement>('.sidebarAppMetaPopover')
    expectStory(flyout, 'Click should pin the flyout open')

    trigger.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerType: 'mouse' }))
    trigger.click()
    await wait(100)
    expectStory(!doc.querySelector('.sidebarAppMetaPopover'), 'Second click should close the pinned flyout')

    trigger.focus()
    trigger.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 0 }))
    await wait(100)
    flyout = doc.querySelector<HTMLElement>('.sidebarAppMetaPopover')
    expectStory(flyout?.querySelector('a') === doc.activeElement, 'Keyboard activation should focus the first flyout link')

    doc.activeElement?.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, key: 'Escape' }))
    await wait(100)
    expectStory(!doc.querySelector('.sidebarAppMetaPopover'), 'Escape should close the keyboard-opened flyout')
    expectStory(doc.activeElement === trigger, 'Escape should restore focus to the trigger')

    trigger.dispatchEvent(new PointerEvent('pointerover', { bubbles: true, pointerType: 'mouse' }))
    await wait(100)
    trigger.focus()
    trigger.dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 0 }))
    await wait(100)
    flyout = doc.querySelector<HTMLElement>('.sidebarAppMetaPopover')
    expectStory(flyout?.querySelector('a') === doc.activeElement, 'Keyboard pinning should focus an already hover-open flyout')
  },
}
