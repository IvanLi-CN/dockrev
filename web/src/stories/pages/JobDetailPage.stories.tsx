import type { Meta, StoryObj } from '@storybook/react'
import { JobDetailPage } from '../../pages/JobDetailPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof JobDetailPage> = {
  title: 'Pages/JobDetailPage',
  component: JobDetailPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof JobDetailPage>

export const LongLogs: Story = {
  parameters: { dockrevApiScenario: 'queue-long-logs' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'job', jobId: 'job-long' }}
        title="任务详情"
        topbarHint="任务队列"
        pageSubtitle="代表性：长 URL / digest / 多行日志（堆栈/命令输出）应在容器内滚动，且可读可复制"
      >
        {({ onTopActions }) => <JobDetailPage jobId="job-long" onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const RunningDualProgress: Story = {
  parameters: { dockrevApiScenario: 'queue-long-logs' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'job', jobId: 'job-short' }}
        title="任务详情"
        topbarHint="任务队列"
        pageSubtitle="运行中：安排进度与完成进度同时显示"
      >
        {({ onTopActions }) => <JobDetailPage jobId="job-short" onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const LegacyProgressFallback: Story = {
  parameters: { dockrevApiScenario: 'queue-legacy-progress' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'job', jobId: 'job-legacy-running' }}
        title="任务详情"
        topbarHint="任务队列"
        pageSubtitle="兼容场景：旧任务缺失 planned* 字段时，UI 自动回退 planned=completed"
      >
        {({ onTopActions }) => <JobDetailPage jobId="job-legacy-running" onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}
