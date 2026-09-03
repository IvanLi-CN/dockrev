import type { Meta, StoryObj } from '@storybook/react'
import { useEffect } from 'react'
import App from '../../App'
import type { DeployCheckReportEnvelope } from '../../api'
import { RELEASE_DRAWER_QUERY_KEYS } from '../../releaseDrawer'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

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

function makeDeployCheckEnvelope(blocked: boolean): DeployCheckReportEnvelope {
  const status = blocked ? 'fail' : 'pass'
  return {
    status: 'ready',
    refreshing: false,
    report: {
      overall: {
        result: status,
        blockingCheckIds: blocked ? ['core.update_executor_ready'] : [],
        summary: blocked ? 'Compose V2 is required' : 'All required capabilities are available',
      },
      generatedAt: '2026-06-26T14:23:00.000Z',
      checks: [
        {
          id: 'core.update_executor_ready',
          title: '更新执行器可用',
          group: 'core',
          required: true,
          status,
          summary: blocked ? 'compose_v2_required' : 'Compose V2 available',
          impact: 'writes blocked',
          evidence: blocked ? 'Compose V1' : 'Compose V2 5.4.0',
          recommendation: 'install Compose V2+',
        },
      ],
    },
  }
}

function LocationReset(props: { pathname: string; search?: string }) {
  useEffect(() => {
    const normalized = props.pathname.startsWith('/') ? props.pathname : `/${props.pathname}`
    const url = new URL(window.location.href)
    url.pathname = normalized

    for (const key of RELEASE_DRAWER_QUERY_KEYS) {
      url.searchParams.delete(key)
    }

    const search = (props.search ?? '').trim()
    if (search) {
      const nextParams = new URLSearchParams(search.startsWith('?') ? search.slice(1) : search)
      for (const [key, value] of nextParams.entries()) {
        url.searchParams.set(key, value)
      }
    }

    window.history.replaceState({}, '', `${url.pathname}${url.search}`)
    window.dispatchEvent(new PopStateEvent('popstate'))
  }, [props.pathname, props.search])
  return null
}

const meta: Meta<typeof App> = {
  title: 'Pages/InteractiveApp',
  component: App,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof App>

export const Dashboard: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/" />
        <App />
      </>
    )
  },
}

export const ManagementSseRecovery: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevApiBehaviorByRoute: {
      'GET /api/events': { failTimes: 1, failureStatus: 503 },
    },
    dockrevManagementEventsPayload: 'event: management\ndata: {\n\n',
  },
  render: () => {
    return (
      <>
        <LocationReset pathname="/" />
        <App />
      </>
    )
  },
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('.shellStatusBanner-warning')))
    expectStory(Boolean(canvasElement.querySelector('[aria-label="立即重试管理事件流"]')), 'management retry must remain accessible')
    await waitForCondition(() => Number(globalThis.__DOCKREV_MOCK_DEBUG__?.managementEventSourceCalls ?? 0) === 2)
    await waitForCondition(() => !canvasElement.querySelector('.shellStatusBanner-warning'))
    await sleep(16_000)
    expectStory(Number(globalThis.__DOCKREV_MOCK_DEBUG__?.managementEventSourceCalls ?? 0) === 2, 'protocol-invalid payload must not create a third management source')
  },
}

export const ManagementSseForegroundResume: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: () => (
    <>
      <LocationReset pathname="/" />
      <App />
    </>
  ),
}


export const DashboardSlowUpdate: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo-slow-update' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/" />
        <App />
      </>
    )
  },
}

export const Queue: Story = {
  parameters: { dockrevApiScenario: 'queue-mixed' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/queue" />
        <App />
      </>
    )
  },
}

export const QueueLongLogs: Story = {
  parameters: { dockrevApiScenario: 'queue-long-logs' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/queue" />
        <App />
      </>
    )
  },
}

export const VersionInference: Story = {
  parameters: { dockrevApiScenario: 'version-inference-running' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/queue/version-inference" />
        <App />
      </>
    )
  },
}

export const Services: Story = {
  parameters: { dockrevApiScenario: 'multi-stack-mixed' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/services" />
        <App />
      </>
    )
  },
}

export const ServiceDetail: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/services/stack-prod/svc-prod-api" />
        <App />
      </>
    )
  },
}

export const ServiceHistory: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/services/stack-prod/svc-prod-api/history" />
        <App />
      </>
    )
  },
}

export const RepoLinkEditingFlow: Story = {
  parameters: { dockrevApiScenario: 'repo-link-editing' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/services/stack-prod/svc-prod-api" />
        <App />
      </>
    )
  },
}

export const Settings: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/settings" />
        <App />
      </>
    )
  },
}

export const DeployCheck: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/deploy-check" />
        <App />
      </>
    )
  },
}

export const DeployCheckGateBlocked: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevDeployCheckReportOverride: makeDeployCheckEnvelope(true),
    dockrevDeployWelcomeOverride: { neverAutoOpen: true },
  },
  render: () => {
    return (
      <>
        <LocationReset pathname="/" />
        <App />
      </>
    )
  },
  play: async ({ canvasElement }) => {
    await waitForCondition(() => window.location.pathname === '/deploy-check')
    await waitForCondition(() => canvasElement.textContent?.includes('BLOCKING') ?? false)
    const dashboardButton = Array.from(canvasElement.querySelectorAll('button')).find((button) => button.textContent?.includes('进入 Dashboard'))
    expectStory(Boolean(dashboardButton?.disabled), 'failed deploy-check must disable Dashboard entry')
  },
}

export const DeployCheckGateRefreshFailed: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevDeployCheckReportOverride: {
      ...makeDeployCheckEnvelope(false),
      lastError: 'deploy-check refresh failed',
    },
    dockrevDeployWelcomeOverride: { neverAutoOpen: true },
  },
  render: () => {
    return (
      <>
        <LocationReset pathname="/" />
        <App />
      </>
    )
  },
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('.appShell')))
    expectStory(window.location.pathname === '/', 'cached passing report must leave Dashboard accessible after refresh failure')
  },
}

export const DeployCheckGatePassed: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevDeployCheckReportOverride: makeDeployCheckEnvelope(false),
  },
  render: () => {
    return (
      <>
        <LocationReset pathname="/" />
        <App />
      </>
    )
  },
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('.appShell')))
    expectStory(window.location.pathname === '/', 'passing deploy-check must leave Dashboard accessible')
  },
}

export const DeployCheckGateRefreshBlocked: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured',
    dockrevDeployCheckReportSequence: [
      makeDeployCheckEnvelope(false),
      makeDeployCheckEnvelope(false),
      makeDeployCheckEnvelope(false),
      makeDeployCheckEnvelope(false),
      makeDeployCheckEnvelope(true),
    ],
    dockrevDeployWelcomeOverride: { neverAutoOpen: true },
  },
  render: () => {
    return (
      <>
        <LocationReset pathname="/" />
        <App />
      </>
    )
  },
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('.appShell')))
    await sleep(350)
    window.dispatchEvent(new Event('focus'))
    await waitForCondition(() => window.location.pathname === '/deploy-check')
    const dashboardButton = Array.from(canvasElement.querySelectorAll('button')).find((button) => button.textContent?.includes('进入 Dashboard'))
    expectStory(Boolean(dashboardButton?.disabled), 'foreground deploy-check failure must disable Dashboard entry')
  },
}

export const Cleanup: Story = {
  parameters: { dockrevApiScenario: 'cleanup-console-storage-normal' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/cleanup" />
        <App />
      </>
    )
  },
}

export const GhcrWebhookInbox: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/queue/ghcr-webhook-inbox" />
        <App />
      </>
    )
  },
}

export const GhcrWebhookRegistry: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => {
    return (
      <>
        <LocationReset pathname="/settings/ghcr-webhooks" />
        <App />
      </>
    )
  },
}

export const DashboardReleaseDrawerHydrated: Story = {
  parameters: {
    dockrevApiScenario: 'dashboard-demo',
    dockrevServiceOverridesById: {
      'svc-prod-api': {
        settings: {
          autoRollback: true,
          backupTargets: { bindPaths: { '/var/lib/api/data': 'inherit' }, volumeNames: {} },
          repoUrl: 'https://github.com/ivanli-cn/codex-vibe-monitor',
        },
        newVersionDiscoveryCount: 3,
      },
    },
    dockrevGitHubReleasesByServiceId: {
      'svc-prod-api': {
        authMode: 'anonymous',
        repo: {
          fullName: 'ivanli-cn/codex-vibe-monitor',
          htmlUrl: 'https://github.com/ivanli-cn/codex-vibe-monitor',
        },
        items: Array.from({ length: 18 }, (_, index) => ({
          id: 50_000 + index,
          tagName: index === 3 ? '1.39.5' : index === 0 ? '1.41.0' : `1.40.${18 - index}`,
          name: index === 3 ? '1.39.5' : undefined,
          body: `Release notes ${index + 1}`,
          htmlUrl: `https://github.com/ivanli-cn/codex-vibe-monitor/releases/tag/${index === 3 ? '1.39.5' : `1.40.${18 - index}`}`,
          draft: false,
          prerelease: false,
          publishedAt: new Date(Date.UTC(2026, 3, 7, 0, 22) - index * 36 * 60 * 1000).toISOString(),
          createdAt: new Date(Date.UTC(2026, 3, 7, 0, 8) - index * 36 * 60 * 1000).toISOString(),
        })),
        locateByVersion: {
          '1.39.5': {
            status: 'found',
            matchedTag: '1.39.5',
            indexWithinWindow: 3,
            absoluteIndex: 3,
          },
        },
      },
    },
  },
  render: () => {
    return (
      <>
        <LocationReset
          pathname="/"
          search="?releaseDrawer=github&releaseServiceId=svc-prod-api&releaseVersion=1.39.5"
        />
        <App />
      </>
    )
  },
}

export const ReleaseDrawerPermissionDeniedOpenSettings: Story = {
  parameters: {
    dockrevApiScenario: 'settings-configured-load-slow',
    dockrevServiceOverridesById: {
      'svc-prod-api': {
        settings: {
          autoRollback: true,
          backupTargets: { bindPaths: { '/var/lib/api/data': 'inherit' }, volumeNames: {} },
          repoUrl: 'https://github.com/ivanli-cn/private-monitor',
        },
        newVersionDiscoveryCount: 3,
      },
    },
    dockrevGitHubReleasesByServiceId: {
      'svc-prod-api': {
        authMode: 'anonymous',
        repo: {
          fullName: 'ivanli-cn/private-monitor',
          htmlUrl: 'https://github.com/ivanli-cn/private-monitor',
        },
        listStatus: 'permissionDenied',
      },
    },
  },
  render: () => {
    return (
      <>
        <LocationReset pathname="/services" search="?releaseDrawer=github&releaseServiceId=svc-prod-api" />
        <App />
      </>
    )
  },
  play: async () => {
    await new Promise((resolve) => setTimeout(resolve, 220))
    const originalScrollIntoView = Element.prototype.scrollIntoView
    let scrolledTargetId: string | null = null
    Element.prototype.scrollIntoView = function scrollIntoViewPatched(...args) {
      scrolledTargetId = this instanceof HTMLElement ? this.id || null : null
      return originalScrollIntoView.apply(this, args)
    }

    try {
      const openSettingsButton = Array.from(document.querySelectorAll<HTMLButtonElement>('button')).find((button) =>
        button.textContent?.includes('打开设置'),
      )
      if (!openSettingsButton) throw new Error('expected permission denied drawer CTA to render')

      openSettingsButton.click()
      await new Promise((resolve) => setTimeout(resolve, 900))

      if (!window.location.pathname.endsWith('/settings')) {
        throw new Error('expected CTA to navigate to the settings route')
      }
      if (!document.getElementById('settings-ghcr-webhook')) {
        throw new Error('expected GitHub Packages settings card to render after navigation')
      }
      if (scrolledTargetId !== 'settings-ghcr-webhook') {
        throw new Error('expected CTA to scroll the GitHub Packages settings card into view')
      }
    } finally {
      Element.prototype.scrollIntoView = originalScrollIntoView
    }
  },
}
