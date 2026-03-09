import type { Meta, StoryObj } from '@storybook/react'
import { OverviewPage } from '../../pages/OverviewPage'
import { DOCKREV_AGGREGATE_GUARD_HINT } from '../../aggregateUpdateGuard'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const jobsCardOnlyScopeCss = `
.appShell {
  display: block !important;
  min-height: 0 !important;
  height: auto !important;
}

.topbar,
.sidebar,
.mobileDockrevPanel,
.pageHead {
  display: none !important;
}

.content {
  padding: 0 !important;
  overflow: visible !important;
}

.page {
  gap: 0 !important;
}

.overviewJobsCardStoryFocusFrame {
  width: min(100%, 760px);
  margin: 18px;
}

.overviewJobsCardStoryFocusFrame-actual {
  width: min(100%, 560px);
}

.twoCol {
  display: block !important;
}

.twoCol > .card:not(:first-of-type),
.overviewIndent {
  display: none !important;
}
`

const meta: Meta<typeof OverviewPage> = {
  title: 'Pages/OverviewPage',
  component: OverviewPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof OverviewPage>

const TOOLTIP_WAIT_MS = 240

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function findButtonByText(root: ParentNode, text: string): HTMLButtonElement | null {
  return Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => button.textContent?.trim() === text) ?? null
}

async function openTooltip(trigger: HTMLElement): Promise<void> {
  trigger.dispatchEvent(new PointerEvent('pointermove', { bubbles: true }))
  trigger.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }))
  trigger.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
  await sleep(TOOLTIP_WAIT_MS)
}

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

export const JobsCardRunningProgressModes: Story = {
  parameters: { dockrevApiScenario: 'overview-jobs-card-running-progress-modes' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="回归：第 1 条为 determinate（75%），流光仅在已完成区域">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const JobsCardRunningProgressOnly: Story = {
  parameters: { dockrevApiScenario: 'overview-jobs-card-running-progress-modes', layout: 'fullscreen' },
  decorators: [
    (Story) => (
      <div className="overviewJobsCardStoryFocus">
        <style>{jobsCardOnlyScopeCss}</style>
        <div className="overviewJobsCardStoryFocusFrame">
          <Story />
        </div>
      </div>
    ),
  ],
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="单卡聚焦：运行态与结果（仅卡片）">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const JobsCardRunningProgressOnlyActualWidth: Story = {
  parameters: { dockrevApiScenario: 'overview-jobs-card-running-progress-modes', layout: 'fullscreen' },
  decorators: [
    (Story) => (
      <div className="overviewJobsCardStoryFocus">
        <style>{jobsCardOnlyScopeCss}</style>
        <div className="overviewJobsCardStoryFocusFrame overviewJobsCardStoryFocusFrame-actual">
          <Story />
        </div>
      </div>
    ),
  ],
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="单卡聚焦：运行态与结果（接近真实宽度）">
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
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="对齐回归：长 stack / service 名称布局">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const ResolvedTag: Story = {
  parameters: { dockrevApiScenario: 'resolved-tag-demo' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="回归：resolvedTag 展示与触发器内容">
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

export const AggregateDockrevGuard: Story = {
  parameters: { dockrevApiScenario: 'aggregate-dockrev-guard' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="聚合更新保护：Dockrev 预览保留但不会参与执行">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(180)
    const doc = canvasElement.ownerDocument
    const updateAllButton = findButtonByText(canvasElement, '更新全部')
    expectStory(updateAllButton, 'missing aggregate update-all button')

    updateAllButton.click()
    await sleep(160)

    const dialog = doc.querySelector<HTMLElement>('[role="alertdialog"]')
    expectStory(dialog, 'confirm dialog missing after opening aggregate update-all preview')
    expectStory(dialog.textContent?.includes('1 个（可更新/需确认）'), 'aggregate candidate count should exclude guarded dockrev')

    const guardedItems = doc.querySelectorAll('.modalListItemGuarded')
    expectStory(guardedItems.length === 1, `expected 1 guarded dockrev preview row, got ${guardedItems.length}`)

    const guardTrigger = doc.querySelector<HTMLButtonElement>('.modalListGuardHintTrigger')
    expectStory(guardTrigger, 'guard tooltip trigger missing in aggregate preview row')
    guardTrigger.focus()
    await sleep(TOOLTIP_WAIT_MS)

    const tooltip = doc.querySelector<HTMLElement>('[role="tooltip"]')
    expectStory(tooltip?.textContent?.includes(DOCKREV_AGGREGATE_GUARD_HINT), 'guard tooltip text missing for preview row')
  },
}

export const AggregateDockrevOnlyDisabled: Story = {
  parameters: { dockrevApiScenario: 'aggregate-dockrev-only' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="聚合更新保护：仅剩 Dockrev 时直接禁用更新全部">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(180)
    const doc = canvasElement.ownerDocument
    const updateAllButton = findButtonByText(canvasElement, '更新全部')
    expectStory(updateAllButton, 'missing aggregate update-all button in dockrev-only story')
    expectStory(updateAllButton.disabled, 'update-all button should be disabled when only dockrev is guardable')

    const tooltipAnchor = updateAllButton.closest<HTMLElement>('.btnTooltipAnchor')
    expectStory(tooltipAnchor, 'disabled aggregate button should be wrapped with tooltip anchor')
    await openTooltip(tooltipAnchor)

    const tooltip = doc.querySelector<HTMLElement>('[role="tooltip"]')
    expectStory(tooltip?.textContent?.includes(DOCKREV_AGGREGATE_GUARD_HINT), 'disabled aggregate button tooltip missing')
  },
}
