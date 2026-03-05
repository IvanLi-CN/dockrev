import type { Meta, StoryObj } from '@storybook/react'
import { GhcrWebhookRegistryPage } from '../../pages/GhcrWebhookRegistryPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof GhcrWebhookRegistryPage> = {
  title: 'Pages/GhcrWebhookRegistryPage',
  component: GhcrWebhookRegistryPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof GhcrWebhookRegistryPage>

export const Default: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'ghcr-webhook-registry' }}
        title="GHCR Webhook 维护"
        pageSubtitle="集中维护仓库 webhook 注册状态、重试注册与删除反注册任务"
        topbarHint="系统设置"
      >
        {({ onTopActions }) => <GhcrWebhookRegistryPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Empty: Story = {
  parameters: { dockrevApiScenario: 'empty' },
  render: () => {
    return (
      <PageHarness route={{ name: 'ghcr-webhook-registry' }} title="GHCR Webhook 维护" topbarHint="系统设置">
        {({ onTopActions }) => <GhcrWebhookRegistryPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const RegistryLinks: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'ghcr-webhook-registry' }}
        title="GHCR Webhook 维护"
        pageSubtitle="验证仓库标题链接与 webhook 页面跳转规则"
        topbarHint="系统设置"
      >
        {({ onTopActions }) => <GhcrWebhookRegistryPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await new Promise((resolve) => setTimeout(resolve, 120))

    const firstTitleLink = canvasElement.querySelector<HTMLAnchorElement>('.ghcrRegistryTitleLink')
    if (!firstTitleLink) throw new globalThis.Error('repo title link missing')
    const firstHref = firstTitleLink.getAttribute('href')
    if (firstHref !== 'https://github.com/IvanLi-CN/dockrev') {
      throw new globalThis.Error(`unexpected repo link href: ${firstHref ?? '<null>'}`)
    }

    const rows = Array.from(canvasElement.querySelectorAll<HTMLElement>('.ghcrRegistryRow'))
    if (rows.length < 2) throw new globalThis.Error('insufficient rows for webhook link checks')

    const originalOpen = window.open
    const openedUrls: string[] = []
    window.open = ((url?: string | URL) => {
      openedUrls.push(String(url ?? ''))
      return null
    }) as typeof window.open

    try {
      const firstWebhookButton = rows[0]?.querySelector<HTMLButtonElement>('button[aria-label="Webhook 页面"]')
      if (!firstWebhookButton) throw new globalThis.Error('first row webhook button missing')
      firstWebhookButton.click()

      const secondWebhookButton = rows[1]?.querySelector<HTMLButtonElement>('button[aria-label="Webhook 页面"]')
      if (!secondWebhookButton) throw new globalThis.Error('second row webhook button missing')
      secondWebhookButton.click()

      if (openedUrls[0] !== 'https://github.com/IvanLi-CN/dockrev/settings/hooks/1234567') {
        throw new globalThis.Error(`unexpected hook detail URL: ${openedUrls[0] ?? '<null>'}`)
      }
      if (openedUrls[1] !== 'https://github.com/IvanLi-CN/dockrev-supervisor/settings/hooks') {
        throw new globalThis.Error(`unexpected hook list URL: ${openedUrls[1] ?? '<null>'}`)
      }
    } finally {
      window.open = originalOpen
    }
  },
}
