import type { Meta, StoryObj } from '@storybook/react'
import { VersionInferencePage } from '../../pages/VersionInferencePage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof VersionInferencePage> = {
  title: 'Pages/VersionInferencePage',
  component: VersionInferencePage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof VersionInferencePage>

export const Idle: Story = {
  parameters: { dockrevApiScenario: 'version-inference-idle' },
  render: () => {
    return (
      <PageHarness route={{ name: 'version-inference' }} title="版本推测" topbarHint="版本推测可观测性" pageSubtitle="空闲态：仅缓存快照，无 in-flight 任务">
        {({ onTopActions }) => <VersionInferencePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Running: Story = {
  parameters: { dockrevApiScenario: 'version-inference-running' },
  render: () => {
    return (
      <PageHarness route={{ name: 'version-inference' }} title="版本推测" topbarHint="版本推测可观测性" pageSubtitle="运行态：展示镜像级实时进度">
        {({ onTopActions }) => <VersionInferencePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const QueueBacklog: Story = {
  parameters: { dockrevApiScenario: 'version-inference-queue-backlog' },
  render: () => {
    return (
      <PageHarness route={{ name: 'version-inference' }} title="版本推测" topbarHint="版本推测可观测性" pageSubtitle="队列堆积：queued 主导，便于观察排队压力">
        {({ onTopActions }) => <VersionInferencePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const StaleAndAllFailed: Story = {
  parameters: { dockrevApiScenario: 'version-inference-stale-all-failed' },
  render: () => {
    return (
      <PageHarness route={{ name: 'version-inference' }} title="版本推测" topbarHint="版本推测可观测性" pageSubtitle="异常态：stale + all_failed 混合展示">
        {({ onTopActions }) => <VersionInferencePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}
