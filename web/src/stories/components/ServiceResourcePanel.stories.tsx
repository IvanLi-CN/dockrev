import type { Meta, StoryObj } from '@storybook/react'
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
