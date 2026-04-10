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

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

async function waitForCondition(check: () => boolean, timeoutMs = 3000): Promise<void> {
  const started = Date.now()
  while (!check()) {
    if (Date.now() - started > timeoutMs) throw new globalThis.Error('condition timeout')
    await sleep(60)
  }
}

function findButton(root: ParentNode, text: string): HTMLButtonElement | null {
  return (
    Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find(
      (button) => button.textContent?.replace(/\s+/g, ' ').trim() === text,
    ) ?? null
  )
}

function render(stackId: string, serviceId: string): Story['render'] {
  return () => {
    return (
      <PageHarness route={{ name: 'service', stackId, serviceId }} title="服务详情" topbarHint="服务详情">
        {({ onTopActions, onLastScanHint }) => (
          <ServiceDetailPage
            stackId={stackId}
            serviceId={serviceId}
            onLastScanHint={onLastScanHint}
            onTopActions={onTopActions}
          />
        )}
      </PageHarness>
    )
  }
}

export const Updatable: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api'),
}

export const HydratedRunningUpdate: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo-hydrated-update' },
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

export const RollbackAvailable: Story = {
  parameters: { dockrevApiScenario: 'service-detail-rollback-available' },
  render: render('stack-prod', 'svc-prod-api'),
}

export const RollbackUnavailable: Story = {
  parameters: { dockrevApiScenario: 'service-detail-rollback-unavailable' },
  render: render('stack-prod', 'svc-prod-api'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '回滚') != null)

    const trigger = findButton(doc, '回滚')
    expectStory(trigger, 'rollback action missing')
    expectStory(trigger.disabled, 'rollback action should be disabled when no target is available')
    expectStory(
      trigger.title.includes('未找到可回滚到升级前版本的成功升级记录'),
      'rollback disabled reason missing',
    )
  },
}

export const RollbackActive: Story = {
  parameters: { dockrevApiScenario: 'service-detail-rollback-active' },
  render: render('stack-prod', 'svc-prod-api'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '回滚中…') != null)

    const trigger = findButton(doc, '回滚中…')
    expectStory(trigger, 'active rollback action missing')
    trigger.click()

    await waitForCondition(() => window.location.hash.includes('/queue/job-rollback-service'))
  },
}

export const RollbackConfirmOpen: Story = {
  parameters: { dockrevApiScenario: 'service-detail-rollback-confirm-open' },
  render: render('stack-prod', 'svc-prod-api'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '回滚') != null)

    const trigger = findButton(doc, '回滚')
    expectStory(trigger, 'rollback action missing')
    trigger.click()

    await waitForCondition(() => doc.body.textContent?.includes('确认回滚服务 api？') ?? false)
    expectStory(doc.body.textContent?.includes('当前版本'), 'rollback confirm current version missing')
    expectStory(doc.body.textContent?.includes('回滚目标'), 'rollback confirm target version missing')
    expectStory(doc.body.textContent?.includes('来源任务'), 'rollback confirm source job missing')
    expectStory(doc.body.textContent?.includes('执行回滚'), 'rollback confirm action missing')
  },
}

export const RepoLinkEditing: Story = {
  parameters: { dockrevApiScenario: 'repo-link-editing' },
  render: render('stack-prod', 'svc-prod-api'),
  play: async ({ canvasElement }) => {
    const helper = Array.from(canvasElement.querySelectorAll<HTMLElement>('.muted')).find((node) =>
      node.textContent?.includes('清空并保存会禁用后续自动补齐'),
    )
    expectStory(helper, 'repoUrl auto-backfill helper copy missing in service detail story')
  },
}

export const Error: Story = {
  parameters: { dockrevApiScenario: 'error' },
  render: render('stack-prod', 'svc-prod-api'),
}
