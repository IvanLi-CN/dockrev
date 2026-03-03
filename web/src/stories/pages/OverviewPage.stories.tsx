import type { Meta, StoryObj } from '@storybook/react'
import { OverviewPage } from '../../pages/OverviewPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof OverviewPage> = {
  title: 'Pages/OverviewPage',
  component: OverviewPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof OverviewPage>

export const Default: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="聚焦：运行态/结果 + 发现异常 + 更新候选筛选">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const JobsCardHeavyInFlight: Story = {
  parameters: { dockrevApiScenario: 'overview-jobs-card-heavy-inflight' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="回归：未终止任务 >5 时，最多展示 10 条未终止">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const JobsCardTerminalOnly: Story = {
  parameters: { dockrevApiScenario: 'overview-jobs-card-terminal-only' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="回归：未终止任务=0 时，仅展示最多 5 条终止状态任务">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const JobsCardMixedFallback: Story = {
  parameters: { dockrevApiScenario: 'queue-mixed' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="回归：未终止任务不足 5 条时由终止状态补齐到 5 条">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const JobsCardExactFiveNonTerminal: Story = {
  parameters: { dockrevApiScenario: 'overview-jobs-card-exact-five-non-terminal' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="回归：未终止任务=5 时只显示 5 条未终止任务，不补终止状态">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const JobsCardEmpty: Story = {
  parameters: { dockrevApiScenario: 'empty' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="回归：无任务空态文案">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const GuideLineLongNames: Story = {
  parameters: { dockrevApiScenario: 'guide-line-long-names' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="对齐回归：长 service name（最多两行）">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const ResolvedTag: Story = {
  parameters: { dockrevApiScenario: 'resolved-tag-demo' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Empty: Story = {
  parameters: { dockrevApiScenario: 'empty' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Error: Story = {
  parameters: { dockrevApiScenario: 'error' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const NoCandidatesButHasServices: Story = {
  parameters: { dockrevApiScenario: 'no-candidates' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="回归：services>0 且无 candidate">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const MultiStackMixed: Story = {
  parameters: { dockrevApiScenario: 'multi-stack-mixed' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'overview' }}
        title="概览"
        pageSubtitle="代表性场景：多 stacks / 归档对象 / discovered projects"
      >
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const VersionAnomalyBatchList: Story = {
  parameters: { dockrevApiScenario: 'service-detail-version-anomaly' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="批量更新弹窗：版本异常服务高亮与单项提示">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}
