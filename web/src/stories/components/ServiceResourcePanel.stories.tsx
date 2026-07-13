import type { Meta, StoryObj } from '@storybook/react'
import type { ServiceResourceSample } from '../../api'
import { ServiceResourcePanel } from '../../components/ServiceResourcePanel'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof ServiceResourcePanel> = {
  title: 'Components/ServiceResourcePanel',
  component: ServiceResourcePanel,
  decorators: [withDockrevMockApi],
  args: {
    serviceId: 'svc-prod-api',
  },
}

export default meta

type Story = StoryObj<typeof ServiceResourcePanel>

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function metricTab(root: ParentNode, label: string): HTMLButtonElement | null {
  return Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => button.textContent?.trim() === label) ?? null
}

function tick(): Promise<void> {
  return new Promise((resolve) => window.requestAnimationFrame(() => resolve()))
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

export const Default: Story = {
  parameters: { dockrevApiScenario: 'default' },
}

export const EmptyHistory: Story = {
  parameters: { dockrevApiScenario: 'service-detail-resource-monitor-empty' },
}

export const MonitorDisabled: Story = {
  parameters: { dockrevApiScenario: 'service-detail-resource-monitor-disabled' },
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
