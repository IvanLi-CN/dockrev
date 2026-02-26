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
      <PageHarness route={{ name: 'queue' }} title="任务队列" topbarHint="任务队列">
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const DashboardDemo: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: () => {
    return (
      <PageHarness route={{ name: 'queue' }} title="任务队列" topbarHint="任务队列" pageSubtitle="代表性：任务列表（点击进入任务详情页查看日志）">
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const LegacyProgressFallback: Story = {
  parameters: { dockrevApiScenario: 'queue-legacy-progress' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'queue' }}
        title="任务队列"
        topbarHint="任务队列"
        pageSubtitle="兼容场景：旧任务仅有 completed 进度字段，UI 自动回退 planned=completed"
      >
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const ProgressSmoothing: Story = {
  parameters: { dockrevApiScenario: 'queue-progress-smoothing' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'queue' }}
        title="任务队列"
        topbarHint="任务队列"
        pageSubtitle="演示：running 任务会自动推送进度，观察 420ms 宽度平滑过渡"
      >
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Empty: Story = {
  parameters: { dockrevApiScenario: 'empty' },
  render: () => {
    return (
      <PageHarness route={{ name: 'queue' }} title="任务队列" topbarHint="任务队列">
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Error: Story = {
  parameters: { dockrevApiScenario: 'error' },
  render: () => {
    return (
      <PageHarness route={{ name: 'queue' }} title="任务队列" topbarHint="任务队列">
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}
