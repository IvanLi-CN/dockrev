import type { Meta, StoryObj } from '@storybook/react'
import { useEffect } from 'react'
import App from '../../App'
import { RELEASE_DRAWER_QUERY_KEYS } from '../../releaseDrawer'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

function LocationReset(props: { pathname: string; search?: string }) {
  useEffect(() => {
    const normalized = props.pathname.startsWith('/') ? props.pathname : `/${props.pathname}`
    const url = new URL(window.location.href)
    url.hash = `#${normalized}`

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

    window.history.replaceState({}, '', `${url.pathname}${url.search}${url.hash}`)
    window.dispatchEvent(new HashChangeEvent('hashchange'))
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
            searchedCount: 20,
            matchedTag: '1.39.5',
            page: 1,
            indexWithinPage: 3,
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

      if (!window.location.hash.endsWith('#/settings')) {
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
