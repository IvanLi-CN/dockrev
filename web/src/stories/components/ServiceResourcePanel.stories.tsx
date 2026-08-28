import type { Meta, StoryObj } from '@storybook/react'
import { fireEvent } from 'storybook/test'
import type { ServiceLifecycleProjection, ServiceResourceSample, ServiceResourceSnapshot } from '../../api'
import { ServiceResourcePanel } from '../../components/ServiceResourcePanel'
import { useServiceDetailResourceMonitor } from '../../pages/useServiceDetailResourceMonitor'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'
import { waitForCondition } from '../pages/storyAssertions'

type ResourcePanelStoryProps = {
  serviceId?: string
  readonly?: boolean
  initialSnapshot?: ServiceResourceSnapshot | null
  lifecycle?: ServiceLifecycleProjection | null
}

function ResourcePanelStory({ serviceId = 'svc-prod-api', readonly = false, initialSnapshot = null, lifecycle = null }: ResourcePanelStoryProps) {
  const monitor = useServiceDetailResourceMonitor({ serviceId, readonly, initialSnapshot, isOnline: true })
  const panel = lifecycle
    ? {
        ...monitor.panel,
        samples: initialSnapshot?.samples ?? monitor.panel.samples,
        historyLoaded: true,
        historyLoading: false,
        lifecycle,
      }
    : monitor.panel
  return <ServiceResourcePanel monitor={panel} />
}

const meta: Meta<ResourcePanelStoryProps> = {
  title: 'Components/ServiceResourcePanel',
  component: ResourcePanelStory,
  decorators: [withDockrevMockApi],
  args: {
    serviceId: 'svc-prod-api',
  },
}

export default meta

type Story = StoryObj<ResourcePanelStoryProps>

function withEvidenceFrame(StoryComponent: React.ComponentType) {
  return (
    <div
      className="serviceResourceEvidenceFrame"
      style={{ background: '#000', boxSizing: 'border-box', padding: 24 }}
    >
      <StoryComponent />
    </div>
  )
}

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function metricTab(root: ParentNode, label: string): HTMLButtonElement | null {
  return Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => button.textContent?.trim() === label) ?? null
}

function windowButton(root: ParentNode, label: string): HTMLButtonElement | null {
  return Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => {
    const text = button.textContent?.replace(/\s+/g, ' ').trim() ?? ''
    return text === label
  }) ?? null
}

function tick(): Promise<void> {
  return new Promise((resolve) => window.requestAnimationFrame(() => resolve()))
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

const operationalCpuBaseline = [31, 32, 31, 33, 32, 34, 33, 32, 33, 34, 33, 32, 34, 35, 37, 39, 36, 34, 33, 32, 33, 34, 33, 32, 33]
const operationalPidBaseline = [17, 17, 17, 17, 17, 18, 18, 18, 18, 18, 18, 18, 18, 19, 19, 19, 19, 18, 18, 18, 18, 18, 18, 18, 18]

const highVariationReadings: Array<{
  cpuPercent: number
  memUsedBytes?: number
  netRxRateBps: number
  netTxRateBps: number
  blockReadRateBps: number
  blockWriteRateBps: number
  pids: number
}> = operationalCpuBaseline.map((cpuPercent, index) => ({
  cpuPercent,
  memUsedBytes: index === 11 ? undefined : 920_000_000 + index * 2_400_000,
  netRxRateBps: 24_000 + (cpuPercent - 31) * 1_100,
  netTxRateBps: 11_000 + (cpuPercent - 31) * 650,
  blockReadRateBps: 3_600 + (cpuPercent - 31) * 240,
  blockWriteRateBps: 1_900 + (cpuPercent - 31) * 130,
  pids: operationalPidBaseline[index] ?? 18,
}))

const highVariationSamples = highVariationReadings.reduce<ServiceResourceSample[]>((samples, reading, index) => {
  const previous = samples[index - 1]
  const intervalSeconds = 150
  samples.push({
    sampledAt: new Date(Date.UTC(2026, 6, 13, 7, 1, 10) + index * intervalSeconds * 1_000).toISOString(),
    cpuPercent: reading.cpuPercent,
    memUsedBytes: reading.memUsedBytes,
    memLimitBytes: 2_147_000_000,
    netRxBytes: (previous?.netRxBytes ?? 0) + reading.netRxRateBps * intervalSeconds,
    netTxBytes: (previous?.netTxBytes ?? 0) + reading.netTxRateBps * intervalSeconds,
    blockReadBytes: (previous?.blockReadBytes ?? 0) + reading.blockReadRateBps * intervalSeconds,
    blockWriteBytes: (previous?.blockWriteBytes ?? 0) + reading.blockWriteRateBps * intervalSeconds,
    pids: reading.pids,
    containerCount: 1,
  })
  return samples
}, [])

const lifecycleEvidence: ServiceLifecycleProjection = {
  retentionSince: '2026-07-08T10:00:00.000Z',
  lastEventId: 5,
  nextCursor: 5,
  events: [
    {
      id: 1,
      serviceId: 'svc-prod-api',
      stackId: 'stack-prod',
      operationGroupId: 'op-long',
      jobId: 'job-restart-long',
      origin: 'manual_service',
      transition: 'stopped',
      observedAt: '2026-07-08T11:08:00.000Z',
      boundaryPrecision: 'exact',
      evidence: { engineEvent: 'stop' },
      details: {},
      createdAt: '2026-07-08T11:08:01.000Z',
    },
    {
      id: 2,
      serviceId: 'svc-prod-api',
      stackId: 'stack-prod',
      operationGroupId: 'op-long',
      jobId: 'job-restart-long',
      origin: 'manual_service',
      transition: 'started',
      observedAt: '2026-07-08T11:30:00.000Z',
      boundaryPrecision: 'exact',
      evidence: { startedAt: '2026-07-08T11:30:00.000Z' },
      details: {},
      createdAt: '2026-07-08T11:30:01.000Z',
    },
    {
      id: 3,
      serviceId: 'svc-prod-api',
      stackId: 'stack-prod',
      operationGroupId: 'op-short',
      jobId: 'job-restart-short',
      origin: 'managed_override',
      transition: 'stopped',
      observedAt: '2026-07-08T11:40:00.000Z',
      boundaryPrecision: 'exact',
      evidence: { engineEvent: 'stop' },
      details: {},
      createdAt: '2026-07-08T11:40:01.000Z',
    },
    {
      id: 4,
      serviceId: 'svc-prod-api',
      stackId: 'stack-prod',
      operationGroupId: 'op-short',
      jobId: 'job-restart-short',
      origin: 'managed_override',
      transition: 'started',
      observedAt: '2026-07-08T11:40:01.000Z',
      boundaryPrecision: 'exact',
      evidence: { startedAt: '2026-07-08T11:40:01.000Z' },
      details: {},
      createdAt: '2026-07-08T11:40:02.000Z',
    },
    {
      id: 5,
      serviceId: 'svc-prod-api',
      stackId: 'stack-prod',
      operationGroupId: 'op-incomplete',
      jobId: 'job-restart-incomplete',
      origin: 'backup',
      transition: 'stopped',
      observedAt: '2026-07-08T11:44:00.000Z',
      boundaryPrecision: 'incomplete',
      evidence: { reason: 'events_permission_denied' },
      details: {},
      createdAt: '2026-07-08T11:44:01.000Z',
    },
  ],
  availabilityIntervals: [
    {
      operationGroupId: 'op-long',
      startedAt: '2026-07-08T11:30:00.000Z',
      stoppedAt: '2026-07-08T11:08:00.000Z',
      startEventId: 2,
      stopEventId: 1,
      complete: true,
    },
    {
      operationGroupId: 'op-short',
      startedAt: '2026-07-08T11:40:01.000Z',
      stoppedAt: '2026-07-08T11:40:00.000Z',
      startEventId: 4,
      stopEventId: 3,
      complete: true,
    },
  ],
}

const lifecycleSampleMinutes = [0, 1, 2, 3, 4, 5, 6, 7, 31, 32, 33, 34, 37, 38, 39, 41, 42, 43, 45]

const lifecycleSamples = lifecycleSampleMinutes.map((minute, index): ServiceResourceSample => ({
  sampledAt: new Date(Date.UTC(2026, 6, 8, 11, minute)).toISOString(),
  cpuPercent: 22 + (index % 6) * 1.5,
  memUsedBytes: 1_100_000_000 + index * 8_000_000,
  memLimitBytes: 2_147_000_000,
  netRxBytes: 20_000_000 + index * 1_600_000,
  netTxBytes: 12_000_000 + index * 1_100_000,
  blockReadBytes: 4_000_000 + index * 420_000,
  blockWriteBytes: 1_000_000 + index * 180_000,
  pids: 18 + (index > 10 ? 1 : 0),
  containerCount: 1,
}))

const lifecycleSnapshot: ServiceResourceSnapshot = {
  fetchedAt: '2026-07-08T11:45:00.000Z',
  windowKey: '1h',
  monitorDisabled: false,
  samples: lifecycleSamples,
}

export const Default: Story = {
  parameters: { dockrevApiScenario: 'default' },
}

export const EmptyHistory: Story = {
  parameters: { dockrevApiScenario: 'service-detail-resource-monitor-empty' },
}

export const InitialHistoryErrorAndRetry: Story = {
  parameters: {
    dockrevApiScenario: 'default',
    dockrevApiBehaviorByRoute: {
      'GET /api/services/svc-prod-api/resource-usage/history': {
        delayMs: 80,
        failTimes: 1,
        failureStatus: 503,
        failureBody: { error: 'mock resource history unavailable' },
      },
    },
  },
  play: async ({ canvasElement }) => {
    await sleep(140)
    const retry = canvasElement.querySelector<HTMLButtonElement>('[aria-label="重试加载"]')
    expectStory(Boolean(retry && canvasElement.querySelector('[role="alert"]')), 'initial history failure must expose a retry overlay')
    retry?.click()
    await sleep(140)
    expectStory(Boolean(canvasElement.querySelector('.svcResourceChart')), 'resource history retry should restore the chart')
  },
}

export const MonitorDisabled: Story = {
  parameters: { dockrevApiScenario: 'service-detail-resource-monitor-disabled' },
  play: async ({ canvasElement }) => {
    await sleep(100)
    expectStory(canvasElement.textContent?.includes('资源监控已关闭'), 'disabled monitor should render its explicit disabled state')
    expectStory(Number(globalThis.__DOCKREV_MOCK_DEBUG__?.resourceUsageEventSourceCalls ?? 0) === 0, 'disabled monitor should not create SSE')
  },
}

export const StreamError: Story = {
  parameters: { dockrevApiScenario: 'service-detail-resource-monitor-stream-error' },
}

export const OfflineSnapshot: Story = {
  args: {
    readonly: true,
    initialSnapshot: {
      fetchedAt: '2026-07-08T11:45:00.000Z',
      windowKey: '1h',
      monitorDisabled: false,
      samples: [
        {
          sampledAt: '2026-07-08T11:00:00.000Z',
          cpuPercent: 12.4,
          memUsedBytes: 1_237_000_000,
          memLimitBytes: 2_147_000_000,
          netRxBytes: 20_000_000,
          netTxBytes: 12_000_000,
          blockReadBytes: 4_000_000,
          blockWriteBytes: 1_000_000,
          pids: 18,
          containerCount: 1,
        },
        {
          sampledAt: '2026-07-08T11:20:00.000Z',
          cpuPercent: 28.6,
          memUsedBytes: 1_456_000_000,
          memLimitBytes: 2_147_000_000,
          netRxBytes: 35_000_000,
          netTxBytes: 24_000_000,
          blockReadBytes: 8_500_000,
          blockWriteBytes: 2_200_000,
          pids: 21,
          containerCount: 1,
        },
        {
          sampledAt: '2026-07-08T11:45:00.000Z',
          cpuPercent: 16.1,
          memUsedBytes: 1_402_000_000,
          memLimitBytes: 2_147_000_000,
          netRxBytes: 48_000_000,
          netTxBytes: 31_000_000,
          blockReadBytes: 11_300_000,
          blockWriteBytes: 3_100_000,
          pids: 19,
          containerCount: 1,
        },
      ],
    },
  },
  play: async ({ canvasElement }) => {
    await sleep(40)
    expectStory(canvasElement.textContent?.includes('离线缓存'), 'offline snapshot should stay read-only')
    expectStory(Number(globalThis.__DOCKREV_MOCK_DEBUG__?.resourceUsageEventSourceCalls ?? 0) === 0, 'offline snapshot should not create SSE')
  },
}

export const HighVariationCurves: Story = {
  args: {
    readonly: true,
    initialSnapshot: {
      fetchedAt: '2026-07-13T08:01:10.000Z',
      windowKey: '1h',
      monitorDisabled: false,
      samples: highVariationSamples,
    },
  },
  play: async ({ canvasElement }) => {
    const cpuPath = canvasElement.querySelector<SVGPathElement>('.svcResourceLine')?.getAttribute('d') ?? ''
    expectStory(cpuPath.includes(' H ') && cpuPath.includes(' V ') && cpuPath.includes(' Q ') && !cpuPath.includes(' C '), 'CPU should retain sampled plateaus with subtly rounded transitions')
    expectStory(canvasElement.querySelectorAll('.svcResourceArea').length === 1, 'single-series CPU chart should retain one restrained area')

    const memoryTab = metricTab(canvasElement, '内存')
    expectStory(memoryTab, 'memory metric tab should be available')
    memoryTab.click()
    await tick()
    expectStory(memoryTab.getAttribute('data-state') === 'active', 'memory metric tab should become active')
    const memoryPath = canvasElement.querySelector<SVGPathElement>('.svcResourceLine')?.getAttribute('d') ?? ''
    expectStory((memoryPath.match(/M /g) ?? []).length === 2, 'missing memory data should split the rendered path')

    const pidsTab = metricTab(canvasElement, 'PIDs')
    expectStory(pidsTab, 'PIDs metric tab should be available')
    pidsTab.click()
    await tick()
    expectStory(pidsTab.getAttribute('data-state') === 'active', 'PIDs metric tab should become active')
    const pidsPath = canvasElement.querySelector<SVGPathElement>('.svcResourceLine')?.getAttribute('d') ?? ''
    expectStory(pidsPath.includes(' H ') && pidsPath.includes(' V '), 'PIDs should render as right-continuous steps')
    expectStory(!pidsPath.includes(' C '), 'PIDs should not interpolate synthetic curves')

    const networkTab = metricTab(canvasElement, '网络')
    expectStory(networkTab, 'network metric tab should be available')
    networkTab.click()
    await tick()
    expectStory(networkTab.getAttribute('data-state') === 'active', 'network metric tab should become active')
    const networkPaths = Array.from(canvasElement.querySelectorAll<SVGPathElement>('.svcResourceLine')).map((path) => path.getAttribute('d') ?? '')
    expectStory(networkPaths.length === 2 && networkPaths.every((path) => path.includes(' H ') && path.includes(' V ') && path.includes(' Q ') && !path.includes(' C ')), 'network RX and TX should retain sampled plateaus with subtly rounded transitions')
    expectStory(canvasElement.querySelectorAll('.svcResourceArea').length === 0, 'dual-series network chart should not render an area fill')
  },
}

export const LifecycleMarkers: Story = {
  args: { readonly: true, initialSnapshot: lifecycleSnapshot, lifecycle: lifecycleEvidence },
  decorators: [withEvidenceFrame],
  play: async ({ canvasElement }) => {
    await tick()
    expectStory(canvasElement.querySelectorAll('.svcResourceLifecycleBand').length === 1, 'long lifecycle interval should render as a band')
    expectStory(canvasElement.querySelectorAll('.svcResourceLifecycleLine').length === 1, 'sub-6px lifecycle interval should render as a line')
    expectStory(canvasElement.querySelectorAll('svg circle').length === 0, 'resource charts should not render point markers')
    expectStory(canvasElement.querySelectorAll('.svcResourceGapServiceStopped').length === 2, 'service downtime markers should use a neutral interval')
    expectStory(canvasElement.querySelectorAll('.svcResourceGapWarning').length === 1, 'continuous unexplained gap should use a warning interval')
    expectStory(canvasElement.querySelectorAll('.svcResourceLifecycleDiagnosticLine').length === 1, 'incomplete lifecycle observations should use a diagnostic line')
    expectStory(canvasElement.querySelectorAll('.svcResourceGapSingle').length === 0, 'single missing sample should not render a gap marker')

    const hoverSurface = canvasElement.querySelector<SVGRectElement>('.svcResourceHoverSurface')
    expectStory(hoverSurface, 'resource chart should expose a hover surface')
    const svg = canvasElement.querySelector<SVGSVGElement>('.svcResourceChartSvg')
    expectStory(svg, 'resource chart svg should be available for hover coordinates')
    const pointerAtMinute = (minute: number) => {
      const viewBoxX = 50 + 850 * (minute / 45)
      const screenTransform = svg.getScreenCTM()
      if (!screenTransform) return
      const screenPoint = new DOMPoint(viewBoxX, 140).matrixTransform(screenTransform)
      fireEvent.pointerMove(hoverSurface, {
        clientX: screenPoint.x,
        clientY: screenPoint.y,
      })
    }
    pointerAtMinute(20)
    await tick()
    expectStory(canvasElement.querySelector('[role="tooltip"][data-hover-kind="lifecycle"]')?.textContent?.includes('服务停止区间'), 'hovering downtime should expose interval details')

    pointerAtMinute(35)
    await tick()
    expectStory(canvasElement.querySelector('[role="tooltip"][data-hover-kind="gap"]')?.textContent?.includes('监控采样缺口'), 'hovering an unexplained gap should expose gap details')

    pointerAtMinute(44)
    await tick()
    expectStory(canvasElement.querySelector('[role="tooltip"][data-hover-kind="lifecycle"]')?.textContent?.includes('生命周期事件'), 'hovering an incomplete observation should expose diagnostic details')
  },
}

export const WindowSwitchContract: Story = {
  parameters: { dockrevApiScenario: 'default' },
  decorators: [withEvidenceFrame],
  play: async ({ canvasElement }) => {
    expectStory(canvasElement.textContent?.includes('页面打开后会叠加 1 秒 SSE 实时点'), 'short windows should keep live samples')

    const labels = ['3m', '1h', '24h', '7d', '30d'] as const
    for (const label of labels) {
      const button = windowButton(canvasElement, label)
      expectStory(button, `${label} window button should be visible`)
    }

    const button24h = windowButton(canvasElement, '24h')
    expectStory(button24h, '24h window button should be available')
    button24h.click()
    await tick()
    expectStory(button24h.getAttribute('data-state') === 'on', '24h window button should become active')

    const button3m = windowButton(canvasElement, '3m')
    expectStory(button3m, '3m window button should be available')
    button3m.click()
    await tick()
    expectStory(button3m.getAttribute('data-state') === 'on', '3m window button should become active')

    const button30d = windowButton(canvasElement, '30d')
    expectStory(button30d, '30d window button should be available')
    button30d.click()
    await tick()
    expectStory(button30d.getAttribute('data-state') === 'on', '30d window button should become active')
    expectStory(canvasElement.textContent?.includes('长时间窗口按时间桶展示历史均值'), 'long windows should remain aggregated')
    expectStory(canvasElement.textContent?.includes('聚合历史'), 'long windows should not attach a realtime stream')
    expectStory(
      Number(canvasElement.querySelector('.svcResourceChart')?.getAttribute('data-point-count')) <= 480,
      'long windows should downsample chart points to the rendering budget',
    )
    const hoverSurface = canvasElement.querySelector<SVGRectElement>('.svcResourceHoverSurface')
    expectStory(hoverSurface, 'aggregated chart should expose a hover surface')
    const bounds = hoverSurface.getBoundingClientRect()
    fireEvent.pointerMove(hoverSurface, { clientX: bounds.right - 4, clientY: bounds.top + bounds.height * 0.5 })
    await tick()
    expectStory(canvasElement.querySelector('[role="tooltip"][data-hover-kind="sample"]')?.textContent?.includes('此桶峰值 CPU'), 'the latest aggregated sample should expose its CPU peak in the hover details')
  },
}

export const VisibilityPauseResume: Story = {
  parameters: { dockrevApiScenario: 'default' },
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Number(globalThis.__DOCKREV_MOCK_DEBUG__?.resourceUsageHistoryCalls ?? 0) === 1)
    await waitForCondition(() => Number(globalThis.__DOCKREV_MOCK_DEBUG__?.resourceUsageEventSourceCalls ?? 0) === 1)
    const previousVisibility = document.visibilityState
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' })
    document.dispatchEvent(new Event('visibilitychange'))
    await waitForCondition(() => Number(globalThis.__DOCKREV_MOCK_DEBUG__?.resourceUsageEventSourceCloseCalls ?? 0) >= 1)
    await new Promise((resolve) => setTimeout(resolve, 120))
    expectStory(Number(globalThis.__DOCKREV_MOCK_DEBUG__?.resourceUsageHistoryCalls ?? 0) === 1, 'hidden page should not reload resource history')
    expectStory(canvasElement.textContent?.includes('页面不可见，实时连接已暂停'), 'hidden page should pause its resource stream')

    Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'visible' })
    document.dispatchEvent(new Event('visibilitychange'))
    await waitForCondition(() => Number(globalThis.__DOCKREV_MOCK_DEBUG__?.resourceUsageHistoryCalls ?? 0) === 2)
    await waitForCondition(() => Number(globalThis.__DOCKREV_MOCK_DEBUG__?.resourceUsageEventSourceCalls ?? 0) === 2)
    expectStory(Number(globalThis.__DOCKREV_MOCK_DEBUG__?.resourceUsageHistoryCalls ?? 0) === 2, 'foreground page should reload resource history once')
    expectStory(Number(globalThis.__DOCKREV_MOCK_DEBUG__?.resourceUsageEventSourceCalls ?? 0) === 2, 'foreground page should resume with a fresh resource stream')
    Object.defineProperty(document, 'visibilityState', { configurable: true, value: previousVisibility })
  },
}
