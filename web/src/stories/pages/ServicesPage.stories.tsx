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
        newVersionDiscoveryCount: 123,
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
  play: async ({ canvasElement }) => {
    await sleep(120)

    const row = Array.from(canvasElement.querySelectorAll<HTMLElement>('.rowLine')).find(
      (element) =>
        element.textContent?.includes('api') &&
        Boolean(element.querySelector('.discoveryHistoryTriggerCompact')),
    )
    expectStory(row, 'expected services row with compact discovery badge')
    expectStory(
      !row.querySelector(':scope > .statusCol > .discoveryHistoryTriggerCompact'),
      'compact badge should not be anchored to the entire status column',
    )

    const labelAnchor = row.querySelector<HTMLElement>('.statusLineLabelAnchor')
    expectStory(labelAnchor, 'expected discovery badge to anchor to the status label wrapper')
    expectStory(
      labelAnchor?.querySelector('.statusLineLabelText')?.textContent?.includes('可更新'),
      'expected status label text to stay inside the anchor wrapper',
    )
    expectStory(
      Boolean(labelAnchor?.querySelector('.discoveryHistoryTriggerCompact')),
      'expected compact discovery badge inside the status label wrapper',
    )
    expectStory(
      Boolean(row.querySelector('.statusNote')?.textContent?.trim()),
      'expected status remark note to remain visible under the badge',
    )
  },
}

export const DiscoveryTimelineOpenFromBadge: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevServiceOverridesById: {
      'svc-prod-api': {
        settings: {
          autoRollback: true,
          backupTargets: { bindPaths: { '/var/lib/api/data': 'inherit' }, volumeNames: {} },
          repoUrl: 'https://github.com/ivanli-cn/codex-vibe-monitor',
        },
        newVersionDiscoveryCount: 3,
      },
    },
    dockrevGitHubReleasesByServiceId: {
      'svc-prod-api': {
        authMode: 'anonymous',
        repo: {
          fullName: 'ivanli-cn/codex-vibe-monitor',
          htmlUrl: 'https://github.com/ivanli-cn/codex-vibe-monitor',
        },
        items: [
          {
            id: 4100,
            tagName: '1.41.0',
            name: '1.41.0',
            body: 'Latest release notes',
            htmlUrl: 'https://github.com/ivanli-cn/codex-vibe-monitor/releases/tag/1.41.0',
            draft: false,
            prerelease: false,
            publishedAt: '2026-04-07T00:22:00Z',
            createdAt: '2026-04-07T00:20:00Z',
          },
          {
            id: 4101,
            tagName: '1.40.0',
            name: '1.40.0',
            body: 'Current candidate release notes',
            htmlUrl: 'https://github.com/ivanli-cn/codex-vibe-monitor/releases/tag/1.40.0',
            draft: false,
            prerelease: false,
            publishedAt: '2026-04-06T16:22:00Z',
            createdAt: '2026-04-06T16:10:00Z',
          },
        ],
      },
    },
  },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'services' }}
        title="服务"
        topbarHint="服务"
        pageSubtitle="验证点击 compact discovery badge 只打开版本时间线，不直接打开 GitHub Releases 抽屉"
      >
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(180)
    const row = Array.from(canvasElement.querySelectorAll<HTMLElement>('.rowLine')).find(
      (element) =>
        element.textContent?.includes('api') &&
        Boolean(element.querySelector('.discoveryHistoryTriggerCompact')),
    )
    expectStory(row, 'expected services row with compact discovery badge for timeline popover')

    const badge = row.querySelector<HTMLButtonElement>('.discoveryHistoryTriggerCompact')
    expectStory(badge, 'expected compact discovery badge trigger')
    badge.click()

    await sleep(260)

    expectStory(
      !window.location.search.includes('releaseDrawer=github'),
      'expected compact discovery badge click to avoid opening the GitHub release drawer directly',
    )
    expectStory(
      !document.querySelector('[data-release-drawer="true"]'),
      'expected GitHub release drawer to stay closed after clicking the badge',
    )
    expectStory(
      Boolean(document.querySelector('.discoveryHistoryPopover')),
      'expected discovery history popover to open after clicking the badge',
    )
  },
}

export const GitHubReleaseDrawerTargetVersionFromTimeline: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevDiscoveryTimelineByServiceId: {
      'svc-prod-api': {
        items: [
          { kind: 'currentCandidate', version: '1.40.0', occurredAt: '2026-04-07T00:22:00+08:00' },
          { kind: 'historicalCandidate', version: '1.39.5', occurredAt: '2026-04-07T00:37:00+08:00' },
          { kind: 'currentRunning', version: '1.39.4', occurredAt: '2026-04-06T18:20:00+08:00' },
        ],
      },
    },
    dockrevServiceOverridesById: {
      'svc-prod-api': {
        settings: {
          autoRollback: true,
          backupTargets: { bindPaths: { '/var/lib/api/data': 'inherit' }, volumeNames: {} },
          repoUrl: 'https://github.com/ivanli-cn/codex-vibe-monitor',
        },
        newVersionDiscoveryCount: 3,
      },
    },
    dockrevGitHubReleasesByServiceId: {
      'svc-prod-api': {
        authMode: 'anonymous',
        repo: {
          fullName: 'ivanli-cn/codex-vibe-monitor',
          htmlUrl: 'https://github.com/ivanli-cn/codex-vibe-monitor',
        },
        items: [
          {
            id: 4200,
            tagName: '1.41.0',
            name: '1.41.0',
            body: 'Latest release notes',
            htmlUrl: 'https://github.com/ivanli-cn/codex-vibe-monitor/releases/tag/1.41.0',
            draft: false,
            prerelease: false,
            publishedAt: '2026-04-07T00:22:00Z',
            createdAt: '2026-04-07T00:20:00Z',
          },
          {
            id: 4201,
            tagName: '1.40.0',
            name: '1.40.0',
            body: 'Current candidate release notes',
            htmlUrl: 'https://github.com/ivanli-cn/codex-vibe-monitor/releases/tag/1.40.0',
            draft: false,
            prerelease: false,
            publishedAt: '2026-04-07T00:02:00Z',
            createdAt: '2026-04-06T23:48:00Z',
          },
          {
            id: 4202,
            tagName: '1.39.5',
            name: '1.39.5',
            body: 'Previous release notes',
            htmlUrl: 'https://github.com/ivanli-cn/codex-vibe-monitor/releases/tag/1.39.5',
            draft: false,
            prerelease: false,
            publishedAt: '2026-04-06T16:37:00Z',
            createdAt: '2026-04-06T16:30:00Z',
          },
        ],
        locateByVersion: {
          '1.39.5': {
            status: 'found',
            searchedCount: 20,
            matchedTag: '1.39.5',
            page: 1,
            indexWithinPage: 2,
            absoluteIndex: 2,
          },
        },
      },
    },
  },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'services' }}
        title="服务"
        topbarHint="服务"
        pageSubtitle="验证点击时间线中的具体版本会打开抽屉并带上 target version"
      >
        {({ onLastScanHint, onTopActions }) => (
          <ServicesPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(180)

    const row = Array.from(canvasElement.querySelectorAll<HTMLElement>('.rowLine')).find(
      (element) =>
        element.textContent?.includes('api') &&
        Boolean(element.querySelector('.discoveryHistoryTriggerCompact')),
    )
    expectStory(row, 'expected services row with compact discovery badge for timeline story')

    const badge = row.querySelector<HTMLButtonElement>('.discoveryHistoryTriggerCompact')
    expectStory(badge, 'expected compact discovery badge trigger for timeline story')
    badge.click()
    await sleep(220)

    const versionButton = document.querySelector<HTMLButtonElement>('.discoveryTimelineVersionBtn')
    expectStory(versionButton, 'expected timeline version button in popover')
    versionButton.click()

    await sleep(320)

    expectStory(
      window.location.search.includes('releaseVersion=1.39.5'),
      'expected URL to include the selected target release version',
    )
    expectStory(
      Boolean(document.querySelector('[data-release-tag="1.39.5"]')),
      'expected release drawer to include the targeted GitHub release entry',
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
