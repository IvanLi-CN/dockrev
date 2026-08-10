import type { Meta, StoryObj } from '@storybook/react'
import type { DeployCheckReportEnvelope } from '../../api'
import { DeployWelcomePage } from '../../pages/DeployWelcomePage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof DeployWelcomePage> = {
  title: 'Pages/DeployWelcomePage',
  component: DeployWelcomePage,
  decorators: [withDockrevMockApi],
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof DeployWelcomePage>

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

async function waitForCondition(check: () => boolean, timeoutMs = 3_000): Promise<void> {
  const started = Date.now()
  while (!check()) {
    if (Date.now() - started > timeoutMs) throw new globalThis.Error('condition timeout')
    await sleep(60)
  }
}

function expectStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function renderPage(pageSubtitle = '部署功能完整性检查') {
  return (
    <PageHarness route={{ name: 'deploy-check' }} title="部署检查" pageSubtitle={pageSubtitle}>
      {() => <DeployWelcomePage />}
    </PageHarness>
  )
}

function makeRefreshingEnvelope(): DeployCheckReportEnvelope {
  return {
    status: 'ready',
    refreshing: true,
    retryAfterMs: 450,
    report: {
      overall: {
        result: 'fail',
        blockingCheckIds: ['core.compose_access'],
        summary: 'Compose config is temporarily unavailable',
      },
      generatedAt: '2026-06-26T14:22:00.000Z',
      checks: [
        {
          id: 'core.docker_engine',
          title: 'Docker 引擎可用',
          group: 'core',
          required: true,
          status: 'pass',
          summary: 'docker daemon reachable',
          impact: '不可用时无法执行更新',
          evidence: 'docker info ok',
          recommendation: '',
        },
        {
          id: 'core.compose_access',
          title: 'Compose 配置可访问',
          group: 'core',
          required: true,
          status: 'fail',
          summary: 'compose paths are temporarily unavailable',
          impact: '服务解析不完整，更新目标不可信',
          evidence: '/srv/app/docker-compose.yml missing',
          recommendation: '',
        },
        {
          id: 'feature.notifications.webhook',
          title: '通知能力：Webhook',
          group: 'feature',
          required: false,
          status: 'na',
          summary: 'webhook notification is disabled',
          impact: '该功能未启用；不纳入阻塞判定',
          evidence: 'enabled=false',
          recommendation: '',
        },
      ],
    },
  }
}

function makeBlockedEnvelope(): DeployCheckReportEnvelope {
  return { ...makeRefreshingEnvelope(), refreshing: false }
}

export const Default: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => renderPage(),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('整体结论') ?? false)
    const dashboardButton = Array.from(canvasElement.querySelectorAll('button')).find((button) => button.textContent?.includes('进入 Dashboard'))
    expectStory(dashboardButton && !dashboardButton.disabled, 'passing report should allow entering Dashboard')
  },
}

export const BlockedCoreFailure: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevDeployCheckReportOverride: makeBlockedEnvelope(),
    dockrevDeployWelcomeOverride: { neverAutoOpen: true },
  },
  render: () => renderPage('核心检查失败时必须停留在故障门禁页，不能绕过进入 Dashboard'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('BLOCKING') ?? false)
    const dashboardButton = Array.from(canvasElement.querySelectorAll('button')).find((button) => button.textContent?.includes('进入 Dashboard'))
    expectStory(Boolean(dashboardButton?.disabled), 'blocking report should disable Dashboard entry')
  },
}

export const BlockedCoreFailureMobile: Story = {
  ...BlockedCoreFailure,
  parameters: {
    ...BlockedCoreFailure.parameters,
    viewport: { defaultViewport: 'mobile1' },
  },
}

export const CachedReportRefreshing: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevDeployCheckReportOverride: makeRefreshingEnvelope(),
  },
  render: () => renderPage('验证 cached report 可先显示，后台 refresh 不阻塞首屏'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('部署功能完整性检查清单') ?? false)
    await waitForCondition(() => canvasElement.textContent?.includes('正在后台刷新最新检查结果…') ?? false)
    await waitForCondition(() => canvasElement.textContent?.includes('整体结论') ?? false)
  },
}

export const InitialPending: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevDeployCheckReportOverride: {
      status: 'pending',
      refreshing: true,
      retryAfterMs: 450,
      report: null,
    } satisfies Partial<DeployCheckReportEnvelope>,
  },
  render: () => renderPage('验证首次无缓存时仅显示 pending shell，待 poll 后再进入 checklist'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('正在加载部署检查报告…') ?? false)
    await sleep(120)
    expectStory(
      !(canvasElement.textContent?.includes('无法加载检查报告') ?? false),
      'initial pending should not flash the failure copy while the first report is still pending',
    )
    await waitForCondition(() => canvasElement.textContent?.includes('重试') ?? false)
  },
}
