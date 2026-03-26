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

const discoveryCardOnlyScopeCss = `
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

.discoveryCardStoryFocusFrame {
  width: min(100%, 780px);
  margin: 18px;
}

.twoCol {
  display: block !important;
}

.twoCol > .card:first-of-type,
.overviewIndent {
  display: none !important;
}
`

const meta: Meta<typeof OverviewPage> = {
  title: 'Pages/OverviewPage',
  component: OverviewPage,
  decorators: [withDockrevMockApi],
  tags: ['autodocs'],
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

function findGroup(root: ParentNode, stackName: string): HTMLElement | null {
  return Array.from(root.querySelectorAll<HTMLElement>('.tableGroup')).find((group) => group.textContent?.includes(stackName)) ?? null
}

function findRowLine(root: ParentNode, text: string): HTMLElement | null {
  return Array.from(root.querySelectorAll<HTMLElement>('.rowLine')).find((row) => row.textContent?.includes(text)) ?? null
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

export const RegistryAndRepoLinks: Story = {
  parameters: { dockrevApiScenario: 'link-icon-catalog' },
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="更新候选里的镜像名展示 registry / repo icon 外链">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(180)

    const doc = canvasElement.ownerDocument
    const apiRow = findRowLine(canvasElement, 'acme/api')
    expectStory(apiRow, 'api row missing in overview link-icon story')

    const apiRegistry = apiRow?.querySelector<HTMLAnchorElement>('[data-link-kind="registry"][data-link-icon="ghcr"]')
    expectStory(apiRegistry?.href === 'https://ghcr.io/acme/api', 'overview GHCR registry icon missing or wrong href')

    const apiRepo = apiRow?.querySelector<HTMLAnchorElement>('[data-link-kind="repo"][data-link-icon="generic"]')
    expectStory(apiRepo?.href === 'https://codeberg.org/acme/api', 'overview generic repo icon missing or wrong href')

    const hashBeforeClick = window.location.hash
    apiRepo?.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
    await sleep(80)
    expectStory(window.location.hash === hashBeforeClick, 'clicking overview image icon should not trigger row navigation')

    const webRow = findRowLine(canvasElement, 'ops/web')
    expectStory(webRow, 'web row missing in overview link-icon story')
    expectStory(!webRow?.querySelector('[data-link-kind="registry"]'), 'unknown registry should not render a registry icon in overview rows')
    const webRepo = webRow?.querySelector<HTMLAnchorElement>('[data-link-kind="repo"][data-link-icon="gitlab"]')
    expectStory(webRepo?.href === 'https://gitlab.com/ops/web', 'overview GitLab repo icon missing or wrong href')

    const infraGroup = findGroup(canvasElement, 'infra')
    expectStory(infraGroup, 'infra group missing in overview link-icon story')
    const infraHead = infraGroup?.querySelector<HTMLElement>('.groupHead')
    infraHead?.click()
    await sleep(120)

    const prometheusRow = findRowLine(canvasElement, 'prometheus/prometheus')
    expectStory(prometheusRow, 'prometheus row missing after expanding infra group')
    const quayRegistry = prometheusRow?.querySelector<HTMLAnchorElement>('[data-link-kind="registry"][data-link-icon="generic"]')
    expectStory(quayRegistry?.href === 'https://quay.io/repository/prometheus/prometheus', 'overview Quay registry icon missing or wrong href')

    const postgresRow = findRowLine(canvasElement, 'library/postgres')
    expectStory(postgresRow, 'postgres row missing after expanding infra group')
    const dockerRegistry = postgresRow?.querySelector<HTMLAnchorElement>('[data-link-kind="registry"][data-link-icon="docker"]')
    expectStory(dockerRegistry?.href === 'https://hub.docker.com/_/postgres', 'overview Docker Hub registry icon missing or wrong href')

    const updateButton = findButtonByText(apiRow ?? canvasElement, '执行更新')
    expectStory(updateButton, 'api row update button missing in overview link-icon story')
    updateButton.click()
    await sleep(160)

    const dialog = doc.querySelector<HTMLElement>('[role="alertdialog"]')
    expectStory(dialog, 'service update confirm dialog missing in overview link-icon story')
    const dialogRegistry = dialog?.querySelector<HTMLAnchorElement>('[data-link-kind="registry"][data-link-icon="ghcr"]')
    expectStory(dialogRegistry?.href === 'https://ghcr.io/acme/api', 'overview service dialog should reuse GHCR registry icon')
    const dialogRepo = dialog?.querySelector<HTMLAnchorElement>('[data-link-kind="repo"][data-link-icon="generic"]')
    expectStory(dialogRepo?.href === 'https://codeberg.org/acme/api', 'overview service dialog should reuse repo icon rendering')
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

export const DiscoveryCardReadable: Story = {
  parameters: { dockrevApiScenario: 'overview-discovery-readable', layout: 'fullscreen' },
  decorators: [
    (Story) => (
      <div className="discoveryCardStoryFocus">
        <style>{discoveryCardOnlyScopeCss}</style>
        <div className="discoveryCardStoryFocusFrame">
          <Story />
        </div>
      </div>
    ),
  ],
  render: () => {
    return (
      <PageHarness route={{ name: 'overview' }} title="概览" pageSubtitle="聚焦：结构化发现异常列表与长错误次级暴露">
        {({ onLastScanHint, onTopActions }) => <OverviewPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(180)
    const doc = canvasElement.ownerDocument
    const statChips = Array.from(canvasElement.querySelectorAll<HTMLElement>('.discoveryStatChip')).map((chip) => chip.textContent ?? '')
    expectStory(statChips.some((text) => text.includes('异常项目') && text.includes('4')), 'discovery summary should surface total issue count first')

    const rows = Array.from(canvasElement.querySelectorAll<HTMLElement>('.discoveryIssueRow'))
    expectStory(rows.length === 4, `expected 4 discovery issue rows, got ${rows.length}`)
    expectStory(rows[0]?.textContent?.includes('forward-auth'), 'newest project should lead the discovery issue list')

    const warningRow = rows.find((row) => row.textContent?.includes('forward-auth'))
    expectStory(warningRow, 'warning row missing in discovery readable story')
    const detailsButton = warningRow?.querySelector<HTMLButtonElement>('.discoveryIssueDetailsBtn')
    expectStory(detailsButton, 'warning row should expose a secondary details button for long errors')
    detailsButton.focus()
    await sleep(TOOLTIP_WAIT_MS)

    const tooltip = doc.querySelector<HTMLElement>('[role="tooltip"]')
    expectStory(
      tooltip?.textContent?.includes('DOCKREV_SUPERVISOR_STATE_PATH'),
      'tooltip should preserve the full discovery warning details',
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
