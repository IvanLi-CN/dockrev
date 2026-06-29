import type { Meta, StoryObj } from '@storybook/react'
import { ServiceDetailPage } from '../../pages/ServiceDetailPage'
import { currentRoutePathname, type Route } from '../../routes'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof ServiceDetailPage> = {
  title: 'Pages/ServiceDetailPage',
  component: ServiceDetailPage,
  decorators: [withDockrevMockApi],
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof ServiceDetailPage>
type ServiceSection = 'overview' | 'monitoring' | 'backup' | 'settings'

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

function normalizeText(value: string | null | undefined): string {
  return value?.replace(/\s+/g, ' ').trim() ?? ''
}

function findButton(root: ParentNode, text: string): HTMLButtonElement | null {
  return (
    Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find(
      (button) => normalizeText(button.textContent) === text,
    ) ?? null
  )
}

function findButtons(root: ParentNode, text: string): HTMLButtonElement[] {
  return Array.from(root.querySelectorAll<HTMLButtonElement>('button')).filter(
    (button) => normalizeText(button.textContent) === text,
  )
}

function findActionButton(root: ParentNode, action: string, text: string): HTMLButtonElement | null {
  const scope = root.querySelector(`[data-service-detail-action="${action}"]`)
  if (!scope) return null
  return findButton(scope, text)
}

function findSectionCard(root: ParentNode, card: string): HTMLElement | null {
  return root.querySelector<HTMLElement>(`[data-service-detail-section-card="${card}"]`)
}

function findTab(root: ParentNode, section: ServiceSection): HTMLButtonElement | null {
  return root.querySelector<HTMLButtonElement>(`[data-service-detail-tab="${section}"]`)
}

function drawerText(doc: Document): string {
  return normalizeText(doc.querySelector('.settingsDrawerContent')?.textContent)
}

function routeFor(stackId: string, serviceId: string, section: ServiceSection = 'overview'): Route {
  return section === 'overview'
    ? { name: 'service', stackId, serviceId }
    : { name: 'service', stackId, serviceId, section }
}

function render(
  stackId: string,
  serviceId: string,
  section: ServiceSection = 'overview',
  pageSubtitle?: string,
): Story['render'] {
  return () => (
    <PageHarness
      route={routeFor(stackId, serviceId, section)}
      title="服务详情"
      pageSubtitle={pageSubtitle}
    >
      {({ route, onTopActions, onLastScanHint }) =>
        route.name === 'service' ? (
          <ServiceDetailPage
            stackId={route.stackId}
            serviceId={route.serviceId}
            section={route.section}
            onLastScanHint={onLastScanHint}
            onTopActions={onTopActions}
          />
        ) : null
      }
    </PageHarness>
  )
}

export const OverviewDefault: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'overview', '旧链接默认落到概览；保留共享顶部动作与最近更新记录'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('最近更新记录'))
    expectStory(currentRoutePathname() === '/services/stack-prod/svc-prod-api', 'legacy overview route should stay canonical')
    expectStory(findTab(canvasElement, 'overview')?.getAttribute('data-state') === 'active', 'overview tab should be active')
    expectStory(!normalizeText(canvasElement.textContent).includes('资源监控'), 'overview should not render monitoring panel')
    expectStory(!findSectionCard(canvasElement, 'auto-policy'), 'overview should not render settings cards')
    expectStory(findButton(canvasElement, 'Stack 详情'), 'stack detail top action missing')
  },
}

export const MonitoringSection: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'monitoring', '监控子页只承载资源监控面板'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('资源监控'))
    expectStory(currentRoutePathname() === '/services/stack-prod/svc-prod-api/monitoring', 'monitoring deep link missing')
    expectStory(findTab(canvasElement, 'monitoring')?.getAttribute('data-state') === 'active', 'monitoring tab should be active')
    expectStory(!normalizeText(canvasElement.textContent).includes('最近更新记录'), 'monitoring should not render recent updates')
    expectStory(!findSectionCard(canvasElement, 'auto-policy'), 'monitoring should not render settings cards')
  },
}

export const SettingsSection: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'settings', '设置子页集中自动更新、Compose、保护项与维护动作'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, 'auto-policy')))
    expectStory(currentRoutePathname() === '/services/stack-prod/svc-prod-api/settings', 'settings deep link missing')
    expectStory(findTab(canvasElement, 'settings')?.getAttribute('data-state') === 'active', 'settings tab should be active')
    expectStory(Boolean(findSectionCard(canvasElement, 'auto-policy')), 'settings should render auto policy card')
    expectStory(Boolean(findSectionCard(canvasElement, 'ignore-rules')), 'settings should render ignore rules')
    expectStory(Boolean(findSectionCard(canvasElement, 'danger-zone')), 'settings should render maintenance actions')
    expectStory(!normalizeText(canvasElement.textContent).includes('最近更新记录'), 'settings should not render overview card')
  },
}

export const BackupSection: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'backup', '备份子页集中备份摘要、记录列表与设置入口'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, 'backup-summary')))
    expectStory(currentRoutePathname() === '/services/stack-prod/svc-prod-api/backup', 'backup deep link missing')
    expectStory(findTab(canvasElement, 'backup')?.getAttribute('data-state') === 'active', 'backup tab should be active')
    expectStory(Boolean(findSectionCard(canvasElement, 'backup-records')), 'backup should render backup records card')
    expectStory(normalizeText(canvasElement.textContent).includes('当前服务相关的备份记录'), 'backup records heading missing')
    expectStory(normalizeText(canvasElement.textContent).includes('备份时间'), 'backup record card content missing')
  },
}

export const TabNavigation: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'overview', '页头 Tabs 直接驱动 service section 路由'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => findTab(canvasElement, 'overview') != null)

    findTab(canvasElement, 'monitoring')?.click()
    await waitForCondition(() => currentRoutePathname() === '/services/stack-prod/svc-prod-api/monitoring')
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('资源监控'))
    expectStory(findTab(canvasElement, 'monitoring')?.getAttribute('data-state') === 'active', 'monitoring tab active state missing after switch')

    findTab(canvasElement, 'backup')?.click()
    await waitForCondition(() => currentRoutePathname() === '/services/stack-prod/svc-prod-api/backup')
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, 'backup-summary')))
    expectStory(findTab(canvasElement, 'backup')?.getAttribute('data-state') === 'active', 'backup tab active state missing after switch')

    findTab(canvasElement, 'settings')?.click()
    await waitForCondition(() => currentRoutePathname() === '/services/stack-prod/svc-prod-api/settings')
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, 'auto-policy')))
    expectStory(findTab(canvasElement, 'settings')?.getAttribute('data-state') === 'active', 'settings tab active state missing after switch')
  },
}

export const AutoPolicyInherited: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'settings'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, 'auto-policy')))
    expectStory(normalizeText(canvasElement.textContent).includes('继承 Stack'), 'service auto policy inherited summary missing')
    expectStory(findButton(canvasElement, 'Stack 详情'), 'stack detail top action missing')
  },
}

export const AutoPolicyOverrideDelayed: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevServiceOverridesById: {
      'svc-prod-api': {
        settings: {
          autoRollback: true,
          backupTargets: { bindPaths: { '/var/lib/api/data': 'inherit' }, volumeNames: {} },
          repoUrl: 'https://codeberg.org/acme/api',
          autoUpdatePolicy: {
            mode: 'override',
            enabled: true,
            rules: [
              {
                id: 'svc-stable',
                name: 'Service stable',
                enabled: true,
                matcher: { type: 'glob', pattern: '5.2.*' },
                action: 'delayed',
                delay: { minAgeSeconds: 10800, minVersionLag: 3 },
              },
            ],
          },
        },
      },
    },
  },
  render: render('stack-prod', 'svc-prod-api', 'settings'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('Service stable'))
    expectStory(normalizeText(canvasElement.textContent).includes('延迟 3h'), 'nonlinear time slider label missing')
    expectStory(normalizeText(canvasElement.textContent).includes('落后 3 个匹配版本'), 'version lag copy missing')

    const settingsTrigger = findActionButton(doc, 'open-auto-policy', '设置')
    expectStory(settingsTrigger, 'service auto policy drawer trigger missing')
    settingsTrigger.click()
    await waitForCondition(() => drawerText(doc).includes('自动更新策略'))
    await waitForCondition(() => drawerText(doc).includes('Service stable'))
    expectStory(!drawerText(doc).includes('更新前备份 / 回滚'), 'auto policy drawer must not include backup settings')
    expectStory(drawerText(doc).includes('Service stable'), 'service policy editor missing in drawer')
    expectStory(drawerText(doc).includes('历史版本命中预览'), 'history match preview missing')
    await waitForCondition(() => drawerText(doc).includes('命中'))
    expectStory(doc.querySelector('[data-settings-drawer-drag-zone="true"]'), 'drawer drag zone missing')
    expectStory(doc.querySelector('[data-vaul-handle]'), 'drawer handle missing')

    const timeSlider = doc.querySelector<HTMLInputElement>('input[type="range"][aria-label="时间"]')
    expectStory(timeSlider, 'time slider missing')
    timeSlider.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    timeSlider.value = '4'
    timeSlider.dispatchEvent(new Event('input', { bubbles: true }))
    timeSlider.dispatchEvent(new Event('change', { bubbles: true }))
    await waitForCondition(() => drawerText(doc).includes('延迟 6h'))

    const ruleInput = doc.querySelector<HTMLInputElement>('.autoPolicyPattern input')
    expectStory(ruleInput, 'policy rule input missing')
    ruleInput.focus()
    ruleInput.setSelectionRange(0, Math.min(2, ruleInput.value.length))
    expectStory(
      ruleInput.selectionStart === 0 && ruleInput.selectionEnd === Math.min(2, ruleInput.value.length),
      'rule input text selection blocked',
    )
  },
}

export const AutoPolicyInvalidRegexPreview: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevServiceOverridesById: {
      'svc-prod-api': {
        settings: {
          autoRollback: true,
          backupTargets: { bindPaths: { '/var/lib/api/data': 'inherit' }, volumeNames: {} },
          repoUrl: null,
          autoUpdatePolicy: {
            mode: 'override',
            enabled: true,
            rules: [
              {
                id: 'bad-regex',
                name: 'Broken regex',
                enabled: true,
                matcher: { type: 'regex', pattern: '[' },
                action: 'delayed',
                delay: { minAgeSeconds: 900, minVersionLag: 1 },
              },
            ],
          },
        },
      },
    },
  },
  render: render('stack-prod', 'svc-prod-api', 'settings'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('Broken regex'))
    findActionButton(doc, 'open-auto-policy', '设置')?.click()
    await waitForCondition(() => drawerText(doc).includes('不确定'))
    expectStory(drawerText(doc).includes('规则无法预览'), 'invalid regex preview state missing')
  },
}

export const AutoPolicyEmptyHistoryPreview: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevDiscoveryTimelineByServiceId: {
      'svc-prod-api': { items: [] },
    },
  },
  render: render('stack-prod', 'svc-prod-api', 'settings'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, 'auto-policy')))
    findActionButton(doc, 'open-auto-policy', '设置')?.click()
    await waitForCondition(() => drawerText(doc).includes('暂无历史版本记录'))
  },
}

export const AutoPolicyHistoryPreviewError: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevDiscoveryTimelineErrorServiceIds: ['svc-prod-api'],
  },
  render: render('stack-prod', 'svc-prod-api', 'settings'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, 'auto-policy')))
    findActionButton(doc, 'open-auto-policy', '设置')?.click()
    await waitForCondition(() => drawerText(doc).includes('mock discovery timeline failed'))
  },
}

export const ComposeTagEditorSuggestions: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevServiceTagSuggestionsById: {
      'svc-prod-api': [
        { tag: '5.3.0', lastUsedAt: '2026-05-05T14:20:00Z', source: 'manual', useCount: 3 },
        { tag: '5.2.7', lastUsedAt: '2026-05-01T09:00:00Z', source: 'update', useCount: 2 },
        { tag: 'stable', lastUsedAt: '2026-04-25T18:30:00Z', source: 'manual', useCount: 1 },
      ],
    },
  },
  render: render('stack-prod', 'svc-prod-api', 'settings'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '编辑 tag') != null)
    const tagTrigger = findButton(doc, '编辑 tag')
    expectStory(tagTrigger, 'compose tag drawer trigger missing')
    tagTrigger.click()
    await waitForCondition(() => doc.body.textContent?.includes('部署 tag') ?? false)
    expectStory(!drawerText(doc).includes('更新前备份 / 回滚'), 'compose tag drawer should not include service protection settings')
    const input = Array.from(doc.body.querySelectorAll<HTMLInputElement>('input')).find(
      (item) => item.placeholder === '例如 5.2.3 或 stable',
    )
    expectStory(input, 'compose tag input missing')
    expectStory(Number(globalThis.__DOCKREV_MOCK_DEBUG__?.serviceTagSuggestionCalls ?? -1) === 0, 'suggestions should be lazy')
    input.focus()
    await waitForCondition(() => doc.body.textContent?.includes('5.3.0') ?? false)
    expectStory(doc.body.textContent?.includes('2026'), 'suggestion subtitle should include last used time')
    expectStory(!doc.body.textContent?.includes('次'), 'suggestion subtitle should not show use count')
    expectStory(Number(globalThis.__DOCKREV_MOCK_DEBUG__?.serviceTagSuggestionCalls ?? -1) === 1, 'suggestions should load once')
    input.value = '5.2'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    await waitForCondition(() => doc.body.textContent?.includes('5.2.7') ?? false)
    expectStory(!doc.body.textContent?.includes('5.3.0'), 'autocomplete should filter non-matching tag suggestions')
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await waitForCondition(() => input.value === '5.2.7')
    input.blur()
    input.focus()
    await sleep(80)
    expectStory(Number(globalThis.__DOCKREV_MOCK_DEBUG__?.serviceTagSuggestionCalls ?? -1) === 1, 'suggestions should not reload')
  },
}

export const ComposeTagEditorSaveError: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'settings'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '编辑 tag') != null)
    findButton(doc, '编辑 tag')?.click()
    await waitForCondition(() => doc.body.textContent?.includes('部署 tag') ?? false)
    const input = Array.from(doc.body.querySelectorAll<HTMLInputElement>('input')).find(
      (item) => item.placeholder === '例如 5.2.3 或 stable',
    )
    expectStory(input, 'compose tag input missing')
    input.focus()
    input.value = 'compose-error'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    findButton(doc, '保存 tag')?.click()
    await waitForCondition(() => doc.body.textContent?.includes('variable interpolation') ?? false)
  },
}

export const ComposeTagEditorMobileDrawer: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevServiceTagSuggestionsById: {
      'svc-prod-api': [
        { tag: '5.3.0', lastUsedAt: '2026-05-05T14:20:00Z', source: 'manual', useCount: 3 },
        { tag: '5.2.7', lastUsedAt: '2026-05-01T09:00:00Z', source: 'update', useCount: 2 },
      ],
    },
    docs: {
      description: {
        story: 'Capture this story with a narrow viewport to verify the bottom settings drawer tag editor.',
      },
    },
  },
  render: render('stack-prod', 'svc-prod-api', 'settings'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '编辑 tag') != null)
    findButton(doc, '编辑 tag')?.click()
    await waitForCondition(() => doc.body.textContent?.includes('部署 tag') ?? false)
    expectStory(!drawerText(doc).includes('更新前备份 / 回滚'), 'compose tag drawer should not include service protection settings')
    const input = Array.from(doc.body.querySelectorAll<HTMLInputElement>('input')).find(
      (item) => item.placeholder === '例如 5.2.3 或 stable',
    )
    expectStory(input, 'compose tag input missing')
    input.focus()
    await waitForCondition(() => doc.body.textContent?.includes('5.3.0') ?? false)
  },
}

export const ServiceProtectionBackupTargets: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'backup'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '编辑备份设置') != null)
    findButton(doc, '编辑备份设置')?.click()
    await waitForCondition(() => drawerText(doc).includes('备份项（服务级）'))
    expectStory(drawerText(doc).includes('Volumes'), 'volume section missing')
    expectStory(drawerText(doc).includes('Bind paths'), 'bind path section missing')
    expectStory(drawerText(doc).includes('/srv/dockrev/backups'), 'backup storage summary missing')
    expectStory(drawerText(doc).includes('gzip'), 'backup compression copy missing')
    expectStory(drawerText(doc).includes('停机备份'), 'stop-related policy missing')
    expectStory(drawerText(doc).includes('在线备份'), 'live-backup policy missing')
  },
}

export const ServiceProtectionSharedTargetOff: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'backup'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '编辑备份设置') != null)
    findButton(doc, '编辑备份设置')?.click()
    await waitForCondition(() => drawerText(doc).includes('/srv/app/../shared/assets'))
    expectStory(drawerText(doc).includes('关联 2 个服务'), 'related service count missing')
    expectStory(drawerText(doc).includes('当前服务不会为这个 target 触发自动备份'), 'disabled policy copy missing')
  },
}

export const ServiceProtectionEmptyBackupTargets: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-worker', 'backup'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '编辑备份设置') != null)
    findButton(doc, '编辑备份设置')?.click()
    await waitForCondition(() => drawerText(doc).includes('当前服务在 Compose 中未发现可备份 volume 或 bind path'))
  },
}

export const ServiceProtectionStorageSummaryOnly: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'backup'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '编辑备份设置') != null)
    findButton(doc, '编辑备份设置')?.click()
    await waitForCondition(() => drawerText(doc).includes('最近 1 份保留'))
    expectStory(drawerText(doc).includes('.tar.gz'), 'artifact extension summary missing')
    expectStory(drawerText(doc).includes('稳定 1h 后清理'), 'retention summary missing')
  },
}

export const BackupRecordsEmpty: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-worker', 'backup'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, 'backup-records')))
    expectStory(normalizeText(canvasElement.textContent).includes('当前服务暂无相关备份记录'), 'backup empty state missing')
  },
}

export const AutoPolicyDisabled: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevServiceOverridesById: {
      'svc-prod-api': {
        settings: {
          autoRollback: true,
          backupTargets: { bindPaths: { '/var/lib/api/data': 'inherit' }, volumeNames: {} },
          repoUrl: null,
          autoUpdatePolicy: {
            mode: 'disabled',
            enabled: false,
            rules: [],
          },
        },
      },
    },
  },
  render: render('stack-prod', 'svc-prod-api', 'settings'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('不会执行 Stack 级自动部署策略'))
  },
}

export const HydratedRunningUpdate: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo-hydrated-update' },
  render: render('stack-prod', 'svc-prod-api', 'overview'),
}

export const Hint: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-infra', 'svc-infra-loki', 'overview'),
}

export const ArchMismatch: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-infra', 'svc-infra-prom', 'overview'),
}

export const CrossTag: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-infra', 'svc-infra-postgres', 'overview'),
}

export const ResolvedTag: Story = {
  parameters: { dockrevApiScenario: 'resolved-tag-demo' },
  render: render('stack-resolved', 'svc-resolved-web', 'overview'),
}

export const Blocked: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-worker', 'overview'),
}

export const NoCandidate: Story = {
  parameters: { dockrevApiScenario: 'no-candidates' },
  render: render('stack-1', 'svc-a', 'overview'),
}

export const ComposeFallbacks: Story = {
  parameters: { dockrevApiScenario: 'service-detail-compose-fallbacks' },
  render: render('stack-prod', 'svc-prod-api', 'settings'),
}

export const VersionAnomalyUpdatable: Story = {
  parameters: { dockrevApiScenario: 'service-detail-version-anomaly' },
  render: render('stack-prod', 'svc-prod-api', 'overview'),
}

export const InferencePendingCandidateLoading: Story = {
  parameters: { dockrevApiScenario: 'services-inference-pending-candidate-loading' },
  render: render('stack-inference-pending', 'svc-inference-pending', 'overview'),
}

export const ResourceMonitorDisabled: Story = {
  parameters: { dockrevApiScenario: 'service-detail-resource-monitor-disabled' },
  render: render('stack-prod', 'svc-prod-api', 'monitoring'),
}

export const ResourceMonitorEmpty: Story = {
  parameters: { dockrevApiScenario: 'service-detail-resource-monitor-empty' },
  render: render('stack-prod', 'svc-prod-api', 'monitoring'),
}

export const ResourceMonitorStreamError: Story = {
  parameters: { dockrevApiScenario: 'service-detail-resource-monitor-stream-error' },
  render: render('stack-prod', 'svc-prod-api', 'monitoring'),
}

export const RollbackAvailable: Story = {
  parameters: { dockrevApiScenario: 'service-detail-rollback-available' },
  render: render('stack-prod', 'svc-prod-api', 'overview'),
}

export const RollbackUnavailable: Story = {
  parameters: { dockrevApiScenario: 'service-detail-rollback-unavailable' },
  render: render('stack-prod', 'svc-prod-api', 'overview'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '回滚') != null)

    const trigger = findButton(doc, '回滚')
    expectStory(trigger, 'rollback action missing')
    expectStory(trigger.disabled, 'rollback action should be disabled when no target is available')
    expectStory(
      trigger.getAttribute('data-hint')?.includes('未找到可回滚到升级前版本的成功升级记录'),
      'rollback disabled reason missing',
    )
  },
}

export const RollbackActive: Story = {
  parameters: { dockrevApiScenario: 'service-detail-rollback-active' },
  render: render('stack-prod', 'svc-prod-api', 'overview'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '回滚中…') != null)

    const trigger = findButton(doc, '回滚中…')
    expectStory(trigger, 'active rollback action missing')
    trigger.click()

    await waitForCondition(() => window.location.hash.includes('/queue/job-rollback-service'))
  },
}

export const RollbackRefreshRaceAfterUpdate: Story = {
  parameters: { dockrevApiScenario: 'service-detail-rollback-stale-after-update' },
  render: render('stack-prod', 'svc-prod-api', 'overview'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '执行更新') != null)

    const updateTrigger = findButton(doc, '执行更新')
    expectStory(updateTrigger, 'service update action missing')
    updateTrigger.click()

    await waitForCondition(() => doc.body.textContent?.includes('确认更新服务 api？') ?? false)
    const confirmButtons = findButtons(doc.body, '执行更新').filter((button) => !button.disabled)
    const confirmTrigger = confirmButtons.at(-1) ?? null
    expectStory(confirmTrigger, 'service update confirm action missing')
    confirmTrigger.click()

    await waitForCondition(() => findButton(doc, '刷新中…') != null, 8_000)
    const refreshingRollback = findButton(doc, '刷新中…')
    expectStory(refreshingRollback, 'rollback refresh state missing during update settlement')
    expectStory(refreshingRollback.disabled, 'rollback refresh state should stay disabled')
    expectStory(
      refreshingRollback.getAttribute('data-hint') === '回滚信息刷新中…',
      'rollback refresh hint should hide stale unavailable reason',
    )

    await waitForCondition(() => {
      const rollback = findButton(doc, '回滚')
      return Boolean(
        rollback &&
          !rollback.disabled &&
          !rollback.getAttribute('data-hint') &&
          rollback.getAttribute('aria-busy') !== 'true',
      )
    }, 8_000)

    const rollback = findButton(doc, '回滚')
    expectStory(rollback, 'rollback action missing after update settlement')
    expectStory(!rollback.disabled, 'rollback action should recover to enabled state after refresh settles')
    expectStory(
      !rollback.getAttribute('data-hint')?.includes('未找到可回滚到升级前版本的成功升级记录'),
      'rollback action should never restore stale unavailable history hint',
    )
  },
}

export const UpdateConfirmOpen: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'overview'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '执行更新') != null)

    const updateTrigger = findButton(doc, '执行更新')
    expectStory(updateTrigger, 'service update action missing')
    updateTrigger.click()

    await waitForCondition(() => doc.body.textContent?.includes('确认更新服务 api？') ?? false)
    expectStory(doc.body.textContent?.includes('版本'), 'service update confirm version summary missing')
    expectStory(doc.body.textContent?.includes('目标 digest'), 'service update confirm target digest missing')
    expectStory(doc.body.textContent?.includes('架构策略'), 'service update confirm arch policy missing')
  },
}

export const RollbackConfirmOpen: Story = {
  parameters: { dockrevApiScenario: 'service-detail-rollback-confirm-open' },
  render: render('stack-prod', 'svc-prod-api', 'overview'),
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
  render: render('stack-prod', 'svc-prod-api', 'settings'),
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findActionButton(doc, 'open-service-settings', '打开') != null)
    findActionButton(doc, 'open-service-settings', '打开')?.click()
    await waitForCondition(() => doc.body.textContent?.includes('服务保护设置') ?? false)
    const helper = Array.from(doc.body.querySelectorAll<HTMLElement>('.muted')).find((node) =>
      node.textContent?.includes('清空并保存会禁用后续自动补齐'),
    )
    expectStory(helper, 'repoUrl auto-backfill helper copy missing in service detail story')
  },
}

export const Error: Story = {
  parameters: { dockrevApiScenario: 'error' },
  render: render('stack-prod', 'svc-prod-api', 'overview'),
}
