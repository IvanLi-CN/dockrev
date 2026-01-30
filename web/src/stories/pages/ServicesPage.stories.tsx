import type { Meta, StoryObj } from '@storybook/react'
import { ServicesPage } from '../../pages/ServicesPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof ServicesPage> = {
  title: 'Pages/ServicesPage',
  component: ServicesPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof ServicesPage>

export const Default: Story = {
  parameters: { dockrevApiScenario: 'multi-stack-mixed' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务">
        {({ onComposeHint, onTopActions }) => (
          <ServicesPage onComposeHint={onComposeHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}

export const ResolvedTag: Story = {
  parameters: { dockrevApiScenario: 'resolved-tag-demo' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'services' }}
        title="服务"
        topbarHint="服务"
        pageSubtitle="浮动 tag：展示 resolvedTag（hover 可见原 tag）"
      >
        {({ onComposeHint, onTopActions }) => (
          <ServicesPage onComposeHint={onComposeHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}

export const Empty: Story = {
  parameters: { dockrevApiScenario: 'empty' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务">
        {({ onComposeHint, onTopActions }) => (
          <ServicesPage onComposeHint={onComposeHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}

export const Error: Story = {
  parameters: { dockrevApiScenario: 'error' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务">
        {({ onComposeHint, onTopActions }) => (
          <ServicesPage onComposeHint={onComposeHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}

export const DashboardDemo: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务" pageSubtitle="代表性：可更新/需确认/跨 tag/架构不匹配/被阻止 + 可交互">
        {({ onComposeHint, onTopActions }) => (
          <ServicesPage onComposeHint={onComposeHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}
