import type { Meta, StoryObj } from '@storybook/react'
import { ServiceDetailPage } from '../../pages/ServiceDetailPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof ServiceDetailPage> = {
  title: 'Pages/ServiceDetailPage',
  component: ServiceDetailPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof ServiceDetailPage>

function render(stackId: string, serviceId: string): Story['render'] {
  return () => {
    return (
      <PageHarness route={{ name: 'service', stackId, serviceId }} title="服务详情" topbarHint="服务详情">
        {({ onComposeHint, onTopActions }) => (
          <ServiceDetailPage
            stackId={stackId}
            serviceId={serviceId}
            onComposeHint={onComposeHint}
            onTopActions={onTopActions}
          />
        )}
      </PageHarness>
    )
  }
}

export const Updatable: Story = { render: render('stack-1', 'svc-a') }
export const Hint: Story = { render: render('stack-1', 'svc-b') }
export const Blocked: Story = { render: render('stack-1', 'svc-c') }

export const Error: Story = {
  parameters: { dockrevApiScenario: 'error' },
  render: render('stack-1', 'svc-a'),
}
