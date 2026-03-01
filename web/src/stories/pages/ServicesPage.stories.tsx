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
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}

export const GuideLineLongNames: Story = {
  parameters: { dockrevApiScenario: 'guide-line-long-names' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务" pageSubtitle="对齐回归：长 service name（最多两行）">
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}

export const ResolvedTag: Story = {
  parameters: { dockrevApiScenario: 'resolved-tag-demo' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务">
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
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
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
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
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}

export const DashboardDemo: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'services' }}
        title="服务"
        topbarHint="服务"
        pageSubtitle="代表性：可更新/需确认/架构不匹配/被阻止 + 可交互"
      >
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}

export const VersionAnomalyBatchList: Story = {
  parameters: { dockrevApiScenario: 'service-detail-version-anomaly' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务" pageSubtitle="批量更新弹窗：版本异常服务高亮与单项提示">
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}

export const InferencePendingCandidateLoading: Story = {
  parameters: { dockrevApiScenario: 'services-inference-pending-candidate-loading' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'services' }}
        title="服务"
        topbarHint="服务"
        pageSubtitle="回归：versionInference pending + candidate snapshot pending（加载中… -> 加载中…）"
      >
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}
