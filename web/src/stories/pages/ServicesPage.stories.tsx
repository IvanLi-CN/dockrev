import type { Meta, StoryObj } from '@storybook/react'
import { ServicesPage } from '../../pages/ServicesPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof ServicesPage> = {
  title: 'Pages/ServicesPage',
  component: ServicesPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof ServicesPage>

export const Default: Story = {
  render: () => {
    return (
      <PageHarness route={{ name: 'services' }} title="服务" topbarHint="服务">
        {({ onComposeHint, onTopActions }) => (
          <ServicesPage onComposeHint={onComposeHint} onTopActions={onTopActions} />
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
        {({ onComposeHint, onTopActions }) => (
          <ServicesPage onComposeHint={onComposeHint} onTopActions={onTopActions} />
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
        {({ onComposeHint, onTopActions }) => (
          <ServicesPage onComposeHint={onComposeHint} onTopActions={onTopActions} />
        )}
      </PageHarness>
    )
  },
}
