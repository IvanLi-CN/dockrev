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
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="聚焦：可更新 / 需提示 / 架构不匹配 / 被阻止">
        {({ onComposeHint, onTopActions }) => <OverviewPage onComposeHint={onComposeHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Empty: Story = {
  parameters: { dockrevApiScenario: 'empty' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览">
        {({ onComposeHint, onTopActions }) => <OverviewPage onComposeHint={onComposeHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Error: Story = {
  parameters: { dockrevApiScenario: 'error' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览">
        {({ onComposeHint, onTopActions }) => <OverviewPage onComposeHint={onComposeHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const NoCandidatesButHasServices: Story = {
  parameters: { dockrevApiScenario: 'no-candidates' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="回归：services>0 且无 candidate">
        {({ onComposeHint, onTopActions }) => <OverviewPage onComposeHint={onComposeHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}
