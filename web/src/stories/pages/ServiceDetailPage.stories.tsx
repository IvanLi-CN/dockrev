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
        {({ onTopActions }) => <ServiceDetailPage stackId={stackId} serviceId={serviceId} onTopActions={onTopActions} />}
      </PageHarness>
    )
  }
}

export const Updatable: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api'),
}

export const Hint: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-infra', 'svc-infra-loki'),
}

export const ArchMismatch: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-infra', 'svc-infra-prom'),
}

export const CrossTag: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-infra', 'svc-infra-postgres'),
}

export const ResolvedTag: Story = {
  parameters: { dockrevApiScenario: 'resolved-tag-demo' },
  render: render('stack-resolved', 'svc-resolved-web'),
}

export const Blocked: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-worker'),
}

export const NoCandidate: Story = {
  parameters: { dockrevApiScenario: 'no-candidates' },
  render: render('stack-1', 'svc-a'),
}

export const ComposeFallbacks: Story = {
  parameters: { dockrevApiScenario: 'service-detail-compose-fallbacks' },
  render: render('stack-prod', 'svc-prod-api'),
}

export const VersionAnomalyUpdatable: Story = {
  parameters: { dockrevApiScenario: 'service-detail-version-anomaly' },
  render: render('stack-prod', 'svc-prod-api'),
}

export const InferencePendingCandidateLoading: Story = {
  parameters: { dockrevApiScenario: 'services-inference-pending-candidate-loading' },
  render: render('stack-inference-pending', 'svc-inference-pending'),
}

export const ResourceMonitorDisabled: Story = {
  parameters: { dockrevApiScenario: 'service-detail-resource-monitor-disabled' },
  render: render('stack-prod', 'svc-prod-api'),
}

export const ResourceMonitorEmpty: Story = {
  parameters: { dockrevApiScenario: 'service-detail-resource-monitor-empty' },
  render: render('stack-prod', 'svc-prod-api'),
}

export const ResourceMonitorStreamError: Story = {
  parameters: { dockrevApiScenario: 'service-detail-resource-monitor-stream-error' },
  render: render('stack-prod', 'svc-prod-api'),
}

export const RepoLinkEditing: Story = {
  parameters: { dockrevApiScenario: 'repo-link-editing' },
  render: render('stack-prod', 'svc-prod-api'),
}

export const Error: Story = {
  parameters: { dockrevApiScenario: 'error' },
  render: render('stack-prod', 'svc-prod-api'),
}
