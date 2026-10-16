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
      serverBootId: 'boot-story',
      reportBootId: 'boot-story',
      reportRevision: 1,
      startupStatus: 'fail',
      startupFailureKey: 'boot-story:core.compose_access',
      operationImpacts: [{
        checkId: 'core.compose_access',
        scope: 'affected_projects',
        objectIds: [],
        operations: ['update', 'rollback', 'start', 'stop'],
        reason: '服务解析不完整，更新目标不可信',
      }],
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

function makePassingEnvelope(): DeployCheckReportEnvelope {
  const envelope = makeRefreshingEnvelope()
  if (!envelope.report) throw new globalThis.Error('passing fixture requires a report')
  const report = envelope.report
  return {
    ...envelope,
    refreshing: false,
    report: {
      ...report,
      overall: {
        result: 'pass',
        blockingCheckIds: [],
        summary: 'All required capabilities are available',
      },
      checks: report.checks.map((check) =>
        check.id === 'core.compose_access'
          ? {
              ...check,
              status: 'pass',
              summary: 'compose paths are readable',
              evidence: '/srv/app/docker-compose.yml readable',
            }
          : check,
      ),
      startupStatus: 'pass',
      startupFailureKey: null,
      operationImpacts: [],
    },
  }
}

export const Default: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevDeployCheckReportOverride: makePassingEnvelope(),
  },
  render: () => renderPage(),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('整体结论') ?? false)
    const continueButton = Array.from(canvasElement.querySelectorAll('button')).find((button) => button.textContent?.includes('继续浏览'))
    expectStory(continueButton && !continueButton.disabled, 'healthy report should allow continuing to browse')
  },
}

export const CoreFailureDiagnostic: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevDeployCheckReportOverride: makeBlockedEnvelope(),
  },
  render: () => renderPage('核心故障按影响范围提示，诊断与继续浏览入口保持可用'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('影响提示') ?? false)
    const continueButton = Array.from(canvasElement.querySelectorAll('button')).find((button) => button.textContent?.includes('继续浏览'))
    expectStory(Boolean(continueButton && !continueButton.disabled), 'core failure should keep continuing to browse available')
  },
}

export const BlockedCoreFailureMobile: Story = {
  ...CoreFailureDiagnostic,
  parameters: {
    ...CoreFailureDiagnostic.parameters,
    viewport: { defaultViewport: 'mobile1' },
  },
}

export const CachedReportRefreshing: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevDeployCheckReportOverride: makeRefreshingEnvelope(),
  },
    render: () => renderPage('验证服务端复查期间诊断内容仍可使用，网页读取不触发复查'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('部署功能完整性检查清单') ?? false)
    await waitForCondition(() => canvasElement.textContent?.includes('服务端正在复查') ?? false)
    await waitForCondition(() => canvasElement.textContent?.includes('整体结论') ?? false)
  },
}

export const OptionalFeatureFailure: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevDeployCheckReportOverride: {
      ...makePassingEnvelope(),
      report: {
        ...makePassingEnvelope().report!,
        checks: [
          ...makePassingEnvelope().report!.checks,
          {
            id: 'feature.notifications.telegram',
            title: '通知能力：Telegram',
            group: 'feature',
            required: false,
            status: 'fail',
            summary: 'telegram notification config is incomplete',
            impact: '仅 Telegram 通知不可用',
            evidence: 'missing botToken',
            recommendation: '在设置中补齐 Telegram 配置',
          },
        ],
      },
    },
  },
  render: () => renderPage('可选通知故障只在清单中提示，不限制核心操作'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('Telegram') ?? false)
    expectStory(!(canvasElement.textContent?.includes('影响提示') ?? false), 'optional feature failure should not show the core impact notice')
  },
}

export const RecoveredBackgroundFailure: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevDeployCheckReportOverride: {
      ...makePassingEnvelope(),
      report: {
        ...makePassingEnvelope().report!,
        startupStatus: 'settled',
        startupFailureKey: null,
        operationImpacts: [],
      },
    },
  },
  render: () => renderPage('故障恢复后自动解除提示，保留当前诊断页'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('整体结论') ?? false)
    expectStory(!(canvasElement.textContent?.includes('影响提示') ?? false), 'recovered report should clear core impact notice')
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
  render: () => renderPage('无报告时仍保留设置、发现扫描与继续浏览入口'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('[data-async-data-phase="initial-loading"] .skeleton')))
    const buttons = Array.from(canvasElement.querySelectorAll('button')).map((button) => button.textContent ?? '')
    expectStory(buttons.some((label) => label.includes('前往设置')), 'pending diagnostic should keep settings available')
    expectStory(buttons.some((label) => label.includes('发现扫描')), 'pending diagnostic should keep discovery available')
    expectStory(buttons.some((label) => label.includes('继续浏览')), 'pending diagnostic should keep browsing available')
  },
}

export const InitialErrorAndRetry: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevApiBehaviorByRoute: {
      'GET /api/deploy-check/report': {
        delayMs: 80,
        failTimes: 1,
        failureStatus: 503,
        failureBody: { error: 'mock deploy report unavailable' },
      },
    },
  },
  render: () => renderPage(),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('[role="alert"]')))
    const retry = canvasElement.querySelector<HTMLButtonElement>('[aria-label="重试加载"]')
    expectStory(Boolean(retry), 'initial deploy report failure must expose retry')
    retry?.click()
    await waitForCondition(() => canvasElement.textContent?.includes('部署功能完整性检查清单') ?? false)
  },
}
