import type { Meta, StoryObj } from '@storybook/react'
import { ServicesPage } from '../../pages/ServicesPage'
import { DOCKREV_AGGREGATE_GUARD_HINT } from '../../aggregateUpdateGuard'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof ServicesPage> = {
  title: 'Pages/ServicesPage',
  component: ServicesPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof ServicesPage>

const TOOLTIP_WAIT_MS = 240

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function findStackGroup(root: ParentNode, stackName: string): HTMLElement | null {
  return Array.from(root.querySelectorAll<HTMLElement>('.tableGroup')).find((group) => group.textContent?.includes(stackName)) ?? null
}

async function openTooltip(trigger: HTMLElement): Promise<void> {
  trigger.dispatchEvent(new PointerEvent('pointermove', { bubbles: true }))
  trigger.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }))
  trigger.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
  await sleep(TOOLTIP_WAIT_MS)
}

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

export const VersionTagsPopoverDemo: Story = {
  parameters: { dockrevApiScenario: 'version-tags-popover-demo' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务" pageSubtitle="回归：popover 局部刷新回填 resolvedTag（不触发整页加载）">
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

export const AggregateDockrevGuard: Story = {
  parameters: { dockrevApiScenario: 'aggregate-dockrev-guard' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务" pageSubtitle="聚合更新保护：Dockrev 在确认框中只读展示">
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(180)
    const doc = canvasElement.ownerDocument
    const group = findStackGroup(canvasElement, 'aggregate-demo')
    expectStory(group, 'aggregate-demo stack group missing')

    const updateStackButton = Array.from(group.querySelectorAll<HTMLButtonElement>('button')).find(
      (button) => button.textContent?.trim() === '更新此 stack',
    )
    expectStory(updateStackButton, 'stack aggregate update button missing')

    updateStackButton.click()
    await sleep(160)

    const dialog = doc.querySelector<HTMLElement>('[role="alertdialog"]')
    expectStory(dialog, 'confirm dialog missing after opening stack aggregate preview')
    expectStory(dialog.textContent?.includes('1 个（可更新/需确认）'), 'stack aggregate count should exclude guarded dockrev')

    const guardedItems = doc.querySelectorAll('.modalListItemGuarded')
    expectStory(guardedItems.length === 1, `expected 1 guarded dockrev preview row, got ${guardedItems.length}`)

    const guardTrigger = doc.querySelector<HTMLButtonElement>('.modalListGuardHintTrigger')
    expectStory(guardTrigger, 'guard tooltip trigger missing in stack preview row')
    guardTrigger.focus()
    await sleep(TOOLTIP_WAIT_MS)

    const tooltip = doc.querySelector<HTMLElement>('[role="tooltip"]')
    expectStory(tooltip?.textContent?.includes(DOCKREV_AGGREGATE_GUARD_HINT), 'guard tooltip text missing for stack preview row')
  },
}

export const AggregateDockrevOnlyDisabled: Story = {
  parameters: { dockrevApiScenario: 'aggregate-dockrev-only' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务" pageSubtitle="聚合更新保护：仅剩 Dockrev 时直接禁用 stack 更新">
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(180)
    const doc = canvasElement.ownerDocument
    const group = findStackGroup(canvasElement, 'dockrev-only')
    expectStory(group, 'dockrev-only stack group missing')

    const updateStackButton = Array.from(group.querySelectorAll<HTMLButtonElement>('button')).find(
      (button) => button.textContent?.trim() === '更新此 stack',
    )
    expectStory(updateStackButton, 'dockrev-only aggregate stack button missing')
    expectStory(updateStackButton.disabled, 'stack update button should be disabled when only dockrev is guardable')

    const tooltipAnchor = updateStackButton.closest<HTMLElement>('.btnTooltipAnchor')
    expectStory(tooltipAnchor, 'disabled stack update button should be wrapped with tooltip anchor')
    await openTooltip(tooltipAnchor)

    const tooltip = doc.querySelector<HTMLElement>('[role="tooltip"]')
    expectStory(tooltip?.textContent?.includes(DOCKREV_AGGREGATE_GUARD_HINT), 'disabled stack button tooltip missing')
  },
}
