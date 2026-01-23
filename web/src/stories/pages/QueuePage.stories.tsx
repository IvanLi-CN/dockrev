import type { Meta, StoryObj } from '@storybook/react'
import { QueuePage } from '../../pages/QueuePage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof QueuePage> = {
  title: 'Pages/QueuePage',
  component: QueuePage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof QueuePage>

export const Default: Story = {
  parameters: { dockrevApiScenario: 'queue-mixed' },
  render: () => {
    return (
      <PageHarness route={{ name: 'queue' }} title="更新队列" topbarHint="更新队列">
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const DashboardDemo: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: () => {
    return (
      <PageHarness route={{ name: 'queue' }} title="更新队列" topbarHint="更新队列" pageSubtitle="代表性：单 job + 可点选看日志">
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Empty: Story = {
  parameters: { dockrevApiScenario: 'empty' },
  render: () => {
    return (
      <PageHarness route={{ name: 'queue' }} title="更新队列" topbarHint="更新队列">
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Error: Story = {
  parameters: { dockrevApiScenario: 'error' },
  render: () => {
    return (
      <PageHarness route={{ name: 'queue' }} title="更新队列" topbarHint="更新队列">
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}
