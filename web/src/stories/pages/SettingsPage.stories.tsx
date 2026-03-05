import type { Meta, StoryObj } from '@storybook/react'
import { SettingsPage } from '../../pages/SettingsPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof SettingsPage> = {
  title: 'Pages/SettingsPage',
  component: SettingsPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof SettingsPage>

function renderSettingsPage(
  pageSubtitle = '单用户 / Forward Header · 认证配置 · 通知配置 · 备份默认策略',
) {
  return (
    <PageHarness route={{ name: 'settings' }} title="系统设置" pageSubtitle={pageSubtitle} topbarHint="系统设置">
      {({ onTopActions }) => <SettingsPage onTopActions={onTopActions} />}
    </PageHarness>
  )
}

function scrollToNotificationCard(root: HTMLElement): void {
  const cards = Array.from(root.querySelectorAll<HTMLElement>('.card'))
  const notificationCard = cards.find((card) => card.querySelector('.title')?.textContent?.trim() === '通知')
  notificationCard?.scrollIntoView({ block: 'start', behavior: 'auto' })
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
  },
}

export const ResolveLoading: Story = {
  parameters: { dockrevApiScenario: 'settings-configured-resolve-slow' },
  render: () => renderSettingsPage('验证 GHCR 解析并添加按钮在慢响应下的加载反馈'),
}

export const RepoPickerUx: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => renderSettingsPage('验证 GHCR 仓库选择弹窗：默认最近活动排序、搜索筛选与拖动批量开关'),
}

export const GhcrPreview: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => renderSettingsPage('验证 GHCR Repos 区域仅预览前 6 条并通过“查看更多”进入维护页'),
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
      <PageHarness route={{ name: 'settings' }} title="系统设置" topbarHint="系统设置">
        {({ onTopActions }) => <SettingsPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Empty: Story = {
  parameters: { dockrevApiScenario: 'empty' },
  render: () => {
    return (
      <PageHarness route={{ name: 'settings' }} title="系统设置" topbarHint="系统设置">
        {({ onTopActions }) => <SettingsPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}
