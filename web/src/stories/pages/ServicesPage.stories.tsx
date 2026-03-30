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

export const StatusBadgeLayout: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevServiceOverridesById: {
      'svc-prod-api': {
        newVersionDiscoveryCount: 3,
      },
    },
  },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'services' }}
        title="服务"
        topbarHint="服务"
        pageSubtitle="状态列：紧凑发现次数 badge 不应挤占备注行高"
      >
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}

export const HydratedRunningUpdate: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo-hydrated-update' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务" pageSubtitle="回归：首屏已存在 service update running job 时恢复按钮 spinner">
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}

export const RegistryAndRepoLinks: Story = {
  parameters: { dockrevApiScenario: 'link-icon-catalog' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务" pageSubtitle="镜像名旁展示 registry / repo icon 外链">
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(120)

    const githubRepo = canvasElement.querySelector<HTMLAnchorElement>('[data-link-kind="repo"][data-link-icon="github"]')
    expectStory(githubRepo?.target === '_blank', 'github repo link icon should open in a new window')

    const gitlabRepo = canvasElement.querySelector<HTMLAnchorElement>('[data-link-kind="repo"][data-link-icon="gitlab"]')
    expectStory(gitlabRepo?.href === 'https://gitlab.com/ops/web', 'gitlab repo icon missing or wrong href')

    const genericRepo = canvasElement.querySelector<HTMLAnchorElement>('[data-link-kind="repo"][data-link-icon="generic"]')
    expectStory(genericRepo?.href === 'https://codeberg.org/acme/api', 'generic repo icon missing or wrong href')

    const githubRegistry = canvasElement.querySelector<HTMLAnchorElement>('[data-link-kind="registry"][data-link-icon="ghcr"]')
    expectStory(githubRegistry?.href === 'https://ghcr.io/acme/api', 'ghcr registry icon missing or wrong href')
    expectStory(githubRegistry?.ariaLabel === '打开 GHCR 页面', 'ghcr registry icon should expose a GHCR-specific label')

    const dockerRegistry = canvasElement.querySelector<HTMLAnchorElement>('[data-link-kind="registry"][data-link-icon="docker"]')
    expectStory(dockerRegistry?.href === 'https://hub.docker.com/_/postgres', 'docker registry icon missing or wrong href')

    const genericRegistry = canvasElement.querySelector<HTMLAnchorElement>('[data-link-kind="registry"][data-link-icon="generic"]')
    expectStory(genericRegistry?.href === 'https://quay.io/repository/prometheus/prometheus', 'generic registry icon missing or wrong href')
  },
}

export const DigestPinnedImageDisplay: Story = {
  parameters: { dockrevApiScenario: 'digest-pinned-image-display' },
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务" pageSubtitle="digest-only 镜像应保持 @sha256 展示语义">
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(120)

    const imageRow = Array.from(canvasElement.querySelectorAll<HTMLElement>('.imageLinkRow')).find((element) =>
      element.textContent?.includes('acme/api')
    )
    expectStory(imageRow, 'digest-pinned image row missing')
    expectStory(
      imageRow?.title === 'acme/api@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
      'digest-pinned image title should keep the digest instead of falling back to :latest'
    )

    const registryLink = imageRow?.querySelector<HTMLAnchorElement>('[data-link-kind="registry"]')
    expectStory(registryLink?.href === 'https://ghcr.io/acme/api', 'digest-pinned registry icon should still link to the repo page')
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
