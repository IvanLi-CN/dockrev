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
  render: () => {
    return (
      <PageHarness route={{ name: 'queue' }} title="更新队列" topbarHint="更新队列">
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
