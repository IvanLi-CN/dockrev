import type { Meta, StoryObj } from '@storybook/react'
import { SettingsPage } from '../../pages/SettingsPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'
import { derivePublicBaseUrlSuggestion } from '../../publicBaseUrlSuggestion'
import { currentRoutePathname } from '../../routes'
import type { SettingsSection } from '../../routes'

const meta: Meta<typeof SettingsPage> = {
  title: 'Pages/SettingsPage',
  component: SettingsPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof SettingsPage>

const INSTANCE_PUBLIC_BASE_URL_SUGGEST_DISMISSED_STORAGE_KEY =
  'dockrev:settings:instancePublicBaseUrl:suggestCurrentOriginDismissed'

function preparePublicBaseUrlSuggestionStorage(mode: 'clear' | 'dismissed' = 'clear') {
  if (typeof window === 'undefined') return
  if (mode === 'dismissed') {
    window.localStorage.setItem(INSTANCE_PUBLIC_BASE_URL_SUGGEST_DISMISSED_STORAGE_KEY, '1')
    return
  }
  window.localStorage.removeItem(INSTANCE_PUBLIC_BASE_URL_SUGGEST_DISMISSED_STORAGE_KEY)
}

function renderSettingsPage(
  pageSubtitle = 'Forward Auth · 用户/组鉴权 · 通知配置 · 备份默认策略',
  options?: { publicBaseUrlSuggestion?: 'clear' | 'dismissed'; section?: SettingsSection },
) {
  preparePublicBaseUrlSuggestionStorage(options?.publicBaseUrlSuggestion ?? 'clear')
  return (
    <PageHarness route={{ name: 'settings', section: options?.section }} title="系统设置" pageSubtitle={pageSubtitle}>
      {({ route, onTopActions }) => (
        <SettingsPage section={route.name === 'settings' ? route.section : undefined} onTopActions={onTopActions} />
      )}
    </PageHarness>
  )
}

function scrollToNotificationCard(root: HTMLElement): void {
  const cards = Array.from(root.querySelectorAll<HTMLElement>('.card'))
  const notificationCard = cards.find((card) => card.querySelector('.title')?.textContent?.trim() === '通知')
  notificationCard?.scrollIntoView({ block: 'start', behavior: 'auto' })
}

function scrollToOctoRillCard(root: HTMLElement): void {
  const cards = Array.from(root.querySelectorAll<HTMLElement>('.card'))
  const octoRillCard = cards.find((card) => card.querySelector('.title')?.textContent?.trim() === 'OctoRill 更新日志')
  octoRillCard?.scrollIntoView({ block: 'center', behavior: 'auto' })
}

function scrollToResourceMonitorCard(root: HTMLElement): HTMLElement | undefined {
  const card = Array.from(root.querySelectorAll<HTMLElement>('.card')).find(
    (node) => node.querySelector('.title')?.textContent?.trim() === '资源监控',
  )
  card?.scrollIntoView({ block: 'center', behavior: 'auto' })
  return card
}

function setInputValue(input: HTMLInputElement, value: string): void {
  const descriptor = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')
  descriptor?.set?.call(input, value)
  input.dispatchEvent(new Event('input', { bubbles: true }))
  input.dispatchEvent(new Event('change', { bubbles: true }))
}

function clickNotificationTestButton(canvasElement: HTMLElement, channel: 'email' | 'webhook' | 'telegram' | 'webPush') {
  const button = canvasElement.querySelector<HTMLButtonElement>(`button[data-notification-test-channel="${channel}"]`)
  if (!button) throw new globalThis.Error(`notification test button not found: ${channel}`)
  button.click()
}

export const Default: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => renderSettingsPage(),
}

export const DefaultLight: Story = {
  ...Default,
  globals: {
    theme: 'light',
    backgrounds: { value: 'light' },
  },
}

export const MobileIdentityFirst: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    viewport: { defaultViewport: 'mobile' },
    docs: {
      description: {
        story: '在 393 × 852 CSS px 视口验证当前账户位于移动设置页首项，页头不再显示头像入口。',
      },
    },
  },
  render: () => renderSettingsPage(),
  play: async ({ canvasElement }) => {
    await new Promise((resolve) => setTimeout(resolve, 120))
    const settingsPage = canvasElement.querySelector<HTMLElement>('.settingsPage')
    const identity = settingsPage?.querySelector<HTMLElement>('.settingsMobileIdentity')
    if (!settingsPage || !identity) throw new globalThis.Error('expected mobile settings identity')
    if (settingsPage.firstElementChild !== identity) {
      throw new globalThis.Error('mobile identity must be the first settings page item')
    }
    if (!identity.textContent?.includes('alice') || !identity.textContent?.includes('Forward Auth')) {
      throw new globalThis.Error('expected current identity summary')
    }
    if (identity.getBoundingClientRect().height > 100) {
      throw new globalThis.Error('mobile identity summary must stay within 100px')
    }
    const destinations = settingsPage.querySelectorAll<HTMLButtonElement>('.settingsMobileIndexItem')
    if (destinations.length !== 8) throw new globalThis.Error('expected eight mobile settings destinations')
    if (canvasElement.ownerDocument.querySelector('.topbarUserSlotTopbar')) {
      throw new globalThis.Error('mobile topbar identity entry should not render')
    }
    destinations[0]?.click()
    for (let attempt = 0; attempt < 20 && currentRoutePathname() !== '/settings/account'; attempt += 1) {
      await new Promise((resolve) => setTimeout(resolve, 25))
    }
    if (currentRoutePathname() !== '/settings/account') {
      throw new globalThis.Error('mobile account destination should open a second-level route')
    }
    const activeCards = settingsPage.querySelectorAll<HTMLElement>('.settingsSectionCard[data-mobile-active="true"]')
    if (activeCards.length !== 1 || activeCards[0]?.dataset.settingsSection !== 'account') {
      throw new globalThis.Error('account route should expose only the account settings section')
    }
  },
}

export const MobileAccountSubpage: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    viewport: { defaultViewport: 'mobile' },
  },
  render: () => renderSettingsPage('Forward Auth · 用户/组鉴权', { section: 'account' }),
}

export const ResourceMonitorCoordinator: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => renderSettingsPage('验证全局协调的历史采样周期与实时采集复用语义'),
  play: async ({ canvasElement }) => {
    await new Promise((resolve) => setTimeout(resolve, 120))
    const card = scrollToResourceMonitorCard(canvasElement)
    if (!card) throw new globalThis.Error('expected resource monitor settings card')
    if (!card.textContent?.includes('历史采样频率（全局周期）')) {
      throw new globalThis.Error('expected global history cadence label')
    }
    if (!card.textContent?.includes('每个周期只发现一次运行容器')) {
      throw new globalThis.Error('expected coordinator sampling explanation')
    }
    if (!card.textContent?.includes('1 天（固定）')) {
      throw new globalThis.Error('expected one-day retention')
    }
  },
}

export const PublicBaseUrlSuggestion: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => renderSettingsPage('验证空 Public Base URL 时提示当前站点根地址并可自动填入'),
  play: async ({ canvasElement }) => {
    await new Promise((resolve) => setTimeout(resolve, 120))

    const bubble = canvasElement.ownerDocument.querySelector<HTMLElement>('[data-settings-public-base-url-suggestion="visible"]')
    if (!bubble) throw new globalThis.Error('public base url suggestion bubble missing')

    const expectedUrl = derivePublicBaseUrlSuggestion(currentRoutePathname(), window.location.origin, window.location.pathname)
    if (!expectedUrl) throw new globalThis.Error('expected public base url suggestion missing')
    if (!bubble.textContent?.includes(expectedUrl)) {
      throw new globalThis.Error(`unexpected suggested public base url: ${bubble.textContent ?? '<empty>'}`)
    }

    const autofillButton = Array.from(bubble.querySelectorAll<HTMLButtonElement>('button')).find(
      (button) => button.textContent?.trim() === '自动填入',
    )
    if (!autofillButton) throw new globalThis.Error('autofill button missing')
    autofillButton.click()

    await new Promise((resolve) => setTimeout(resolve, 40))

    const input = canvasElement.querySelector<HTMLInputElement>('input[placeholder="https://dockrev.example.com/"]')
    if (!input) throw new globalThis.Error('public base url input missing')
    if (input.value !== expectedUrl) {
      throw new globalThis.Error(`public base url input not autofilled: ${input.value || '<empty>'}`)
    }
    if (canvasElement.ownerDocument.querySelector('[data-settings-public-base-url-suggestion="visible"]')) {
      throw new globalThis.Error('public base url suggestion bubble should disappear after autofill')
    }
  },
}

export const PublicBaseUrlSuggestionDismissed: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () =>
    renderSettingsPage('验证点击“不”后使用 localStorage 记住偏好并在下次加载时不再显示', {
      publicBaseUrlSuggestion: 'dismissed',
    }),
  play: async ({ canvasElement }) => {
    await new Promise((resolve) => setTimeout(resolve, 120))
    if (canvasElement.ownerDocument.querySelector('[data-settings-public-base-url-suggestion="visible"]')) {
      throw new globalThis.Error('public base url suggestion bubble should stay hidden after dismissal')
    }
  },
}

export const ResolveLoading: Story = {
  parameters: { dockrevApiScenario: 'settings-configured-resolve-slow' },
  render: () => renderSettingsPage('验证 GHCR 解析并添加按钮在慢响应下的加载反馈'),
}

export const RepoPickerUx: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => renderSettingsPage('验证 GHCR 仓库选择弹窗：宽屏布局、范围筛选与拖动批量开关'),
}

export const GhcrPreview: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => renderSettingsPage('验证 GHCR Repos 区域仅预览前 6 条并通过“查看更多”进入维护页'),
}

export const OctoRillReleaseNotesCard: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => renderSettingsPage('聚焦 OctoRill 更新日志配置（Base URL / API Key / 默认视图）'),
  play: async ({ canvasElement }) => {
    await new Promise((resolve) => setTimeout(resolve, 120))
    scrollToOctoRillCard(canvasElement)
    await new Promise((resolve) => setTimeout(resolve, 80))

    const card = Array.from(canvasElement.querySelectorAll<HTMLElement>('.card')).find(
      (node) => node.querySelector('.title')?.textContent?.trim() === 'OctoRill 更新日志',
    )
    if (!card) throw new globalThis.Error('expected OctoRill release notes card')
    if (!card.textContent?.includes('数据源')) throw new globalThis.Error('expected provider selector row')
    if (!card.textContent?.includes('设成啥就用啥')) throw new globalThis.Error('expected fixed-provider helper copy')
    if (!card.textContent?.includes('默认视图')) throw new globalThis.Error('expected default view control')
    const providerTrigger = Array.from(card.querySelectorAll<HTMLElement>('button')).find((node) =>
      /GitHub Releases|OctoRill/.test(node.textContent?.trim() ?? ''),
    )
    if (!providerTrigger) throw new globalThis.Error('expected provider select trigger')
    const apiKeyInput = card.querySelector<HTMLInputElement>('input[type="password"]')
    if (!apiKeyInput) throw new globalThis.Error('expected OctoRill API key password input')
    if (!/^•{20}$/.test(apiKeyInput.value)) {
      throw new globalThis.Error(`expected equal-length OctoRill API key mask, got ${apiKeyInput.value.length}`)
    }
    apiKeyInput.focus()
    await new Promise((resolve) => setTimeout(resolve, 40))
    if (apiKeyInput.value !== '') throw new globalThis.Error('expected OctoRill API key mask to clear on focus')
    apiKeyInput.blur()
    await new Promise((resolve) => setTimeout(resolve, 40))
    if (!/^•{20}$/.test(apiKeyInput.value)) {
      throw new globalThis.Error('expected OctoRill API key mask to restore on blur')
    }
  },
}

export const NotificationCard: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => renderSettingsPage('聚焦通知卡片（每渠道独立测试按钮 + 气泡结果）'),
  play: async ({ canvasElement }) => {
    // Wait a tick for async settings data to paint, then jump to the card.
    await new Promise((resolve) => setTimeout(resolve, 120))
    scrollToNotificationCard(canvasElement)
  },
}

export const NotificationChannelTestBubbles: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => renderSettingsPage('验证独立测试按钮：Email 成功 + Web Push 失败气泡'),
  play: async ({ canvasElement }) => {
    await new Promise((resolve) => setTimeout(resolve, 120))
    scrollToNotificationCard(canvasElement)
    await new Promise((resolve) => setTimeout(resolve, 120))
    clickNotificationTestButton(canvasElement, 'email')
    await new Promise((resolve) => setTimeout(resolve, 80))
    clickNotificationTestButton(canvasElement, 'webPush')
    await new Promise((resolve) => setTimeout(resolve, 120))
  },
}

export const NotificationChannelDisabledError: Story = {
  parameters: { dockrevApiScenario: 'settings-notification-channel-errors' },
  render: () => renderSettingsPage('验证渠道关闭/缺配置时仍可测试并显示具体错误'),
  play: async ({ canvasElement }) => {
    await new Promise((resolve) => setTimeout(resolve, 120))
    scrollToNotificationCard(canvasElement)
    await new Promise((resolve) => setTimeout(resolve, 120))
    clickNotificationTestButton(canvasElement, 'telegram')
    await new Promise((resolve) => setTimeout(resolve, 120))
  },
}

export const TelegramTokenValidation: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => renderSettingsPage('验证 Telegram：未触碰不显示眼睛，输入无效 token 不会保存'),
  play: async ({ canvasElement }) => {
    await new Promise((resolve) => setTimeout(resolve, 120))
    scrollToNotificationCard(canvasElement)
    await new Promise((resolve) => setTimeout(resolve, 120))

    const botTokenInput = canvasElement.querySelector<HTMLInputElement>('input[autocomplete="new-password"]')
    if (!botTokenInput) return

    botTokenInput.focus()
    setInputValue(botTokenInput, 'invalid token')

    const chatIdInput = Array.from(canvasElement.querySelectorAll<HTMLInputElement>('input')).find(
      (input) => input !== botTokenInput && (input.value?.includes('-100') ?? false),
    )
    ;(chatIdInput ?? botTokenInput).focus()
    await new Promise((resolve) => setTimeout(resolve, 700))
  },
}

export const Error: Story = {
  parameters: { dockrevApiScenario: 'error' },
  render: () => {
    return (
      <PageHarness route={{ name: 'settings' }} title="系统设置">
        {({ onTopActions }) => <SettingsPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Empty: Story = {
  parameters: { dockrevApiScenario: 'empty' },
  render: () => {
    return (
      <PageHarness route={{ name: 'settings' }} title="系统设置">
        {({ onTopActions }) => <SettingsPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}
