import type { Meta, StoryObj } from '@storybook/react'
import { JobDetailPage } from '../../pages/JobDetailPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

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

export const HealthRollback: Story = {
  parameters: { dockrevApiScenario: 'queue-health-rollback' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'job', jobId: 'job-health-rollback' }}
        title="任务详情"
        topbarHint="任务队列"
        pageSubtitle="健康检查失败后已回滚：进度与日志都应明确表达 rollback，而不是误报 passed"
      >
        {({ onTopActions }) => <JobDetailPage jobId="job-health-rollback" onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(120)

    const pageText = canvasElement.textContent ?? ''
    if (!pageText.includes('rolled_back')) {
      throw new globalThis.Error('rolled_back status pill missing')
    }
    if (!pageText.includes('update rolled back after healthcheck failure')) {
      throw new globalThis.Error('final rollback progress message missing')
    }
    if (!pageText.includes('healthcheck failed for api; rolling back')) {
      throw new globalThis.Error('healthcheck failure log missing')
    }
    if (pageText.includes('healthcheck passed for api')) {
      throw new globalThis.Error('healthcheck passed log should not appear after rollback')
    }
  },
}
