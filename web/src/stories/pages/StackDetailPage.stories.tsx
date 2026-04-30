import type { Meta, StoryObj } from '@storybook/react'
import { StackDetailPage } from '../../pages/StackDetailPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof StackDetailPage> = {
  title: 'Pages/StackDetailPage',
  component: StackDetailPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof StackDetailPage>

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function render(stackId: string): Story['render'] {
  return () => (
    <PageHarness route={{ name: 'stack', stackId }} title="Stack 详情" topbarHint="Stack 详情">
      {({ onTopActions, onLastScanHint }) => (
        <StackDetailPage stackId={stackId} onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
      )}
    </PageHarness>
  )
}

export const PolicyEnabled: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod'),
  play: async ({ canvasElement }) => {
    await sleep(260)
    expectStory(canvasElement.textContent?.includes('Stable semver'), 'stack policy rule missing')
    expectStory(canvasElement.textContent?.includes('延迟 1h'), 'stack policy time slider label missing')
    expectStory(canvasElement.textContent?.includes('落后 2 个匹配版本'), 'stack policy version lag label missing')
  },
}

export const PolicyDisabled: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-infra'),
  play: async ({ canvasElement }) => {
    await sleep(260)
    expectStory(canvasElement.textContent?.includes('未启用'), 'disabled stack policy state missing')
  },
}
