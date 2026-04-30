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

async function waitForCondition(check: () => boolean, timeoutMs = 3000): Promise<void> {
  const started = Date.now()
  while (!check()) {
    if (Date.now() - started > timeoutMs) throw new globalThis.Error('condition timeout')
    await sleep(60)
  }
}

function findButton(root: ParentNode, text: string): HTMLButtonElement | null {
  return (
    Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find(
      (button) => button.textContent?.replace(/\s+/g, ' ').trim() === text,
    ) ?? null
  )
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
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => canvasElement.textContent?.includes('自动更新结果') ?? false)
    expectStory(canvasElement.textContent?.includes('Stable semver'), 'stack policy rule missing')
    expectStory(canvasElement.textContent?.includes('延迟 1h'), 'stack policy time slider label missing')
    expectStory(canvasElement.textContent?.includes('落后 2 个匹配版本'), 'stack policy version lag label missing')
    expectStory(canvasElement.textContent?.includes('最近更新记录'), 'stack recent update records missing')

    const settingsTrigger = findButton(doc, '设置')
    expectStory(settingsTrigger, 'stack settings drawer trigger missing')
    settingsTrigger.click()
    await waitForCondition(() => doc.body.textContent?.includes('Stack 设置') ?? false)
    expectStory(doc.body.textContent?.includes('Stable semver'), 'stack policy editor missing in drawer')
  },
}

export const PolicyDisabled: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-infra'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('自动更新结果') ?? false)
    expectStory(canvasElement.textContent?.includes('未启用'), 'disabled stack policy state missing')
  },
}
