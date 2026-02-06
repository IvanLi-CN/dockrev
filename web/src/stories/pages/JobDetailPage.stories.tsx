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
        topbarHint="更新队列"
        pageSubtitle="代表性：长 URL / digest 日志应断行或限制在容器内滚动"
      >
        {({ onTopActions }) => <JobDetailPage jobId="job-long" onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

