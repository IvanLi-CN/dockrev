import type { Meta, StoryObj } from '@storybook/react'
import type { ServiceLogSnapshotResponse } from '../../api'
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
type ServiceSection = 'overview' | 'monitoring' | 'backup' | 'logs' | 'settings'

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

function expectNearlyEqual(actual: number, expected: number, tolerance: number, message: string): void {
  if (Math.abs(actual - expected) > tolerance) {
    throw new globalThis.Error(`${message}: expected ${expected}, got ${actual}`)
  }
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

function findLogRowContaining(root: ParentNode, text: string): HTMLElement | null {
  return (
    Array.from(root.querySelectorAll<HTMLElement>('.serviceLogRow')).find((row) =>
      normalizeText(row.textContent).includes(text),
    ) ?? null
  )
}

function drawerText(doc: Document): string {
  return normalizeText(doc.querySelector('.settingsDrawerContent')?.textContent)
}

function routeFor(stackId: string, serviceId: string, section: ServiceSection = 'overview'): Route {
  return section === 'overview'
    ? { name: 'service', stackId, serviceId }
    : { name: 'service', stackId, serviceId, section }
}

function buildLongLogsSnapshot(serviceId: string, count = 1600): ServiceLogSnapshotResponse {
  const startedAt = Date.parse('2026-06-29T08:00:00.000Z')
  return {
    serviceId,
    lines: Array.from({ length: count }, (_, index) => {
      const ts = new Date(startedAt + index * 1_000).toISOString()
      const base =
        index % 7 === 0
          ? `GET /internal/metrics 200 trace=req-${String(index).padStart(4, '0')} cache=warm upstream=payments-v2 latency=${40 + (index % 11)}ms region=ap-southeast-1 release=2026.06.29-${(index % 5) + 1}`
          : `worker cycle=${index} queue=critical state=idle lease=svc-prod-api lock=refresh-${String(index).padStart(4, '0')}`
      const raw =
        index % 11 === 0
          ? `\u001b[33m${base}\u001b[0m`
          : index % 13 === 0
            ? `\u001b[31m${base}\u001b[0m`
            : base
      return { ts, raw, plain: raw }
    }),
    lastEventId: count,
    bufferLimit: 2000,
  }
}

function buildMultilineLogsSnapshot(serviceId: string): ServiceLogSnapshotResponse {
  const multilineRaw = [
    '\u001b[2m2026-07-01T08:12:51.833063Z\u001b[0m \u001b[33m WARN\u001b[0m failed to broadcast pool attempt start runtime snapshot err=error returned from database: (code: 5) database is locked',
    '',
    'Caused by:',
    '    (code: 5) database is locked invoke_id=proxy-1281-1782893570550',
  ].join('\n')
  return {
    serviceId,
    lines: [
      {
        ts: '2026-07-01T08:12:51.833063000Z',
        raw: multilineRaw,
        plain: multilineRaw,
      },
      {
        ts: '2026-07-01T08:12:53.763043000Z',
        raw: '\u001b[2m2026-07-01T08:12:53.763043Z\u001b[0m \u001b[32m INFO\u001b[0m openai proxy response headers ready proxy_request_id=1279 method=POST uri=/v1/responses status=200 OK elapsed_ms=10542',
        plain:
          '\u001b[2m2026-07-01T08:12:53.763043Z\u001b[0m \u001b[32m INFO\u001b[0m openai proxy response headers ready proxy_request_id=1279 method=POST uri=/v1/responses status=200 OK elapsed_ms=10542',
      },
    ],
    lastEventId: 2,
    bufferLimit: 2000,
  }
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

export const LogsSection: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'logs', '日志子页提供单服务实时日志、搜索与吸底'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('实时日志'))
    expectStory(currentRoutePathname() === '/services/stack-prod/svc-prod-api/logs', 'logs deep link missing')
    expectStory(findTab(canvasElement, 'logs')?.getAttribute('data-state') === 'active', 'logs tab should be active')
    expectStory(normalizeText(canvasElement.textContent).includes('boot complete'), 'logs should render stream lines')
    expectStory(normalizeText(canvasElement.textContent).includes('runtime perf'), 'logs should render structured message text')
    expectStory(normalizeText(canvasElement.textContent).includes('admin_read'), 'logs should render structured metadata chips')
    const tracingRow = findLogRowContaining(canvasElement, 'openai proxy request started')
    expectStory(tracingRow, 'logs should render parsed tracing text message')
    expectStory(tracingRow?.getAttribute('data-format') === 'text', 'tracing text row should stay text-formatted')
    expectStory(tracingRow?.getAttribute('data-level') === 'info', 'tracing text row should expose parsed info level')
    expectStory(
      normalizeText(tracingRow?.querySelector('.serviceLogLevel')?.textContent) === 'INFO',
      'tracing text row should show parsed level badge',
    )
    expectStory(
      !normalizeText(tracingRow?.querySelector('.serviceLogHumanMsg')?.textContent).includes('2026-07-07T05:54:01'),
      'human tracing message should omit the application timestamp prefix',
    )
    expectStory(
      normalizeText(tracingRow?.textContent).includes('proxy_request_id2722'),
      'tracing text row should render parsed metadata chips',
    )
    expectStory(normalizeText(canvasElement.textContent).includes('2026-06-29'), 'logs should render the log date')
    expectStory(normalizeText(canvasElement.textContent).includes('ERR'), 'logs should render inferred log levels')
    const input = canvasElement.querySelector<HTMLInputElement>('input[aria-label="搜索日志"]')
    expectStory(input, 'logs search input missing')
    expectStory(Boolean(findButton(canvasElement, 'Human')), 'logs human toggle missing')
    expectStory(Boolean(findButton(canvasElement, 'Raw')), 'logs raw toggle missing')
    expectStory(
      canvasElement.querySelector('[data-service-logs-virtualized="true"]')?.getAttribute('data-service-logs-view') === 'human',
      'logs should default to human mode',
    )
    expectStory(Boolean(findButton(canvasElement, '自动换行 关')), 'logs wrap toggle missing')
    expectStory(Boolean(findButton(canvasElement, 'UTC')), 'logs timezone toggle missing')
    expectStory(
      canvasElement.querySelector('[data-service-logs-virtualized="true"]')?.getAttribute('data-service-logs-wrap') === 'off',
      'logs should default to nowrap mode',
    )
    findButton(canvasElement, 'Raw')?.click()
    await waitForCondition(
      () =>
        canvasElement.querySelector('[data-service-logs-virtualized="true"]')?.getAttribute('data-service-logs-view') ===
        'raw',
    )
    expectStory(normalizeText(canvasElement.textContent).includes('"timestamp"'), 'raw mode should expose original JSON text')
    expectStory(
      normalizeText(canvasElement.textContent).includes('2026-07-07T05:54:01.126674Z INFO openai proxy request started'),
      'raw mode should expose original tracing text with application timestamp and level',
    )
    findButton(canvasElement, 'Human')?.click()
    await waitForCondition(
      () =>
        canvasElement.querySelector('[data-service-logs-virtualized="true"]')?.getAttribute('data-service-logs-view') ===
        'human',
    )
    input.value = 'slow query'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new Event('change', { bubbles: true }))
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('1 /'))
    input.value = 'freshness_probe'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new Event('change', { bubbles: true }))
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('runtime perf'))
  },
}

export const SettingsOfflineReadonly: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    pwaStatus: { isOnline: false },
  },
  render: render('stack-prod', 'svc-prod-api', 'settings', '离线时设置页应明确阻断，不伪装成本地可编辑'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('设置页需要联网'))
    expectStory(currentRoutePathname() === '/services/stack-prod/svc-prod-api/settings', 'offline settings deep link missing')
    expectStory(
      normalizeText(canvasElement.textContent).includes('当前离线'),
      'offline readonly banner missing',
    )
    expectStory(
      normalizeText(canvasElement.textContent).includes('设置页包含敏感配置与写操作'),
      'settings offline gate detail missing',
    )
    expectStory(!findSectionCard(canvasElement, 'auto-policy'), 'offline settings should not render editable cards')
  },
}

export const LogsSectionVirtualized: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevServiceLogsByServiceId: {
      'svc-prod-api': {
        snapshot: buildLongLogsSnapshot('svc-prod-api'),
        eventsPayload: ': keep-alive\n\n',
      },
    },
  },
  render: render('stack-prod', 'svc-prod-api', 'logs', '日志子页在大缓冲下继续使用虚拟列表，并提供自动换行切换'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('实时日志'))
    const terminal = canvasElement.querySelector<HTMLElement>('[data-service-logs-virtualized="true"]')
    expectStory(terminal, 'virtualized logs terminal missing')
    const totalCount = Number(terminal?.getAttribute('data-service-logs-total-count') ?? '0')
    const visibleCount = Number(terminal?.getAttribute('data-service-logs-visible-count') ?? '0')
    expectStory(totalCount >= 1600, 'virtualized story should expose a large in-memory buffer')
    expectStory(visibleCount > 0 && visibleCount < totalCount, 'virtualized story should only render the visible window')
    expectStory(
      canvasElement.querySelectorAll('.serviceLogRow').length === visibleCount,
      'rendered row count should match the virtualized visible window',
    )

    const wrapButton = findButton(canvasElement, '自动换行 关')
    expectStory(wrapButton, 'wrap toggle missing in virtualized story')
    wrapButton.click()
    await waitForCondition(() => Boolean(findButton(canvasElement, '自动换行 开')))
    expectStory(
      terminal?.getAttribute('data-service-logs-wrap') === 'on',
      'wrap toggle should update terminal wrap state',
    )

    const utcButton = findButton(canvasElement, 'UTC')
    expectStory(utcButton, 'timezone toggle missing in virtualized story')
  },
}

export const LogsSectionMultilineGrouping: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevServiceLogsByServiceId: {
      'svc-prod-api': {
        snapshot: buildMultilineLogsSnapshot('svc-prod-api'),
        eventsPayload: ': keep-alive\n\n',
      },
    },
  },
  render: render('stack-prod', 'svc-prod-api', 'logs', '多行应用错误保持为一条日志组'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('database is locked'))
    const rows = canvasElement.querySelectorAll<HTMLElement>('.serviceLogRow')
    expectStory(rows.length === 2, 'multiline snapshot should render two logical log rows')
    const firstRow = rows[0]
    expectStory(firstRow?.getAttribute('data-multiline') === 'true', 'error row should be marked multiline')
    expectStory(firstRow?.getAttribute('data-inline-level') === 'true', 'inline tracing level should suppress duplicate badge text')
    expectStory(
      normalizeText(firstRow?.querySelector('.serviceLogMsg')?.textContent).includes('Caused by:'),
      'multiline row should keep continuation text in the message column',
    )
    expectStory(
      firstRow?.querySelector('.serviceLogLevel')?.classList.contains('serviceLogLevelInline'),
      'inline tracing level should render with the compact marker style in the level column',
    )
    expectStory(
      normalizeText(firstRow?.querySelector('.serviceLogLevel')?.textContent) === '',
      'inline tracing level should not repeat the textual level badge',
    )
  },
}

export const LogsSectionEvidence: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: render('stack-prod', 'svc-prod-api', 'logs', '日志子页提供单服务实时日志、搜索与吸底'),
  play: async ({ canvasElement, step }) => {
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('实时日志'))
    expectStory(currentRoutePathname() === '/services/stack-prod/svc-prod-api/logs', 'logs deep link missing')
    expectStory(findTab(canvasElement, 'logs')?.getAttribute('data-state') === 'active', 'logs tab should be active')
    expectStory(normalizeText(canvasElement.textContent).includes('runtime perf'), 'logs evidence story should render structured summary')
    expectStory(normalizeText(canvasElement.textContent).includes('dashboard_overview_phase'), 'logs evidence story should render structured metadata')
    expectStory(normalizeText(canvasElement.textContent).includes('openai proxy request started'), 'logs evidence story should render tracing text summary')
    expectStory(
      !normalizeText(findLogRowContaining(canvasElement, 'openai proxy request started')?.querySelector('.serviceLogHumanMsg')?.textContent).includes('2026-07-07T05:54:01'),
      'logs evidence story should omit tracing timestamp from the human message',
    )
    expectStory(normalizeText(canvasElement.textContent).includes('worker sync complete jobs=18 queue=critical'), 'logs evidence story should render denser stream lines')
    expectStory(normalizeText(canvasElement.textContent).includes('WARN'), 'logs evidence story should expose inferred warning level')
    const input = canvasElement.querySelector<HTMLInputElement>('input[aria-label="搜索日志"]')
    expectStory(input, 'logs search input missing')
    expectStory(input?.value === '', 'logs evidence story should stay in default non-filtered state')
    expectStory(Boolean(findButton(canvasElement, 'Human')), 'logs evidence story should expose human toggle')
    expectStory(Boolean(findButton(canvasElement, 'Raw')), 'logs evidence story should expose raw toggle')
    expectStory(Boolean(findButton(canvasElement, '自动换行 关')), 'logs evidence story should expose wrap toggle')

    const assertAligned = () => {
      const headerCells = canvasElement.querySelectorAll<HTMLElement>('.serviceLogsTerminalHead > span')
      const firstRowCells = canvasElement.querySelectorAll<HTMLElement>('.serviceLogRow:first-of-type > span')
      expectStory(headerCells.length === 3, 'logs header should render three columns')
      expectStory(firstRowCells.length === 3, 'logs first row should render three columns')
      for (let index = 0; index < 3; index += 1) {
        const headerLeft = Math.round(headerCells[index]!.getBoundingClientRect().left)
        const rowLeft = Math.round(firstRowCells[index]!.getBoundingClientRect().left)
        expectNearlyEqual(rowLeft, headerLeft, 1, `logs column ${index + 1} should align between header and body`)
      }
    }

    await step('desktop columns stay aligned', async () => {
      globalThis.innerWidth = 1280
      globalThis.dispatchEvent(new Event('resize'))
      await waitForCondition(() => canvasElement.querySelectorAll('.serviceLogRow:first-of-type > span').length === 3)
      assertAligned()
    })

    await step('mobile columns stay aligned', async () => {
      globalThis.innerWidth = 390
      globalThis.dispatchEvent(new Event('resize'))
      await waitForCondition(() => canvasElement.querySelectorAll('.serviceLogRow:first-of-type > span').length === 3)
      assertAligned()
      globalThis.innerWidth = 1280
      globalThis.dispatchEvent(new Event('resize'))
    })
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

    findTab(canvasElement, 'logs')?.click()
    await waitForCondition(() => currentRoutePathname() === '/services/stack-prod/svc-prod-api/logs')
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('实时日志'))
    expectStory(findTab(canvasElement, 'logs')?.getAttribute('data-state') === 'active', 'logs tab active state missing after switch')

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

export const BackupRecordsLegacyMissingAssets: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevServiceBackupRecordsById: {
      'svc-prod-api': {
        records: [
          {
            backupId: 'bkp_legacy',
            jobId: 'job_legacy',
            scope: 'service',
            status: 'skipped',
            createdAt: '2026-06-28T18:15:24.960797189Z',
            finishedAt: '2026-06-28T18:15:24.960797189Z',
          },
        ],
      },
    },
  },
  render: render('stack-prod', 'svc-prod-api', 'backup', '旧版备份记录缺少 assets 字段时仍稳定渲染'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(findSectionCard(canvasElement, 'backup-records')))
    await waitForCondition(() => normalizeText(canvasElement.textContent).includes('未记录资产明细'))
    expectStory(findTab(canvasElement, 'backup')?.getAttribute('data-state') === 'active', 'backup tab should stay active')
    expectStory(normalizeText(canvasElement.textContent).includes('已跳过'), 'legacy skipped backup status missing')
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
