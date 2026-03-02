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
