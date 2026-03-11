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
