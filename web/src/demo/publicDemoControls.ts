import { appBasePath } from '../appBase'
import { buildFixture } from '../stories/mocks/dockrevMockApi/fixturesMisc'
import { versionReleaseNotes } from '../stories/pages/serviceDetailPageStoryFixtures'
import type {
  DockrevMockGitHubReleasesDataset,
  Fixture,
} from '../stories/mocks/dockrevMockApi/shared'
import {
  PAGES_DEMO_RESTORE_STORAGE_KEY,
  parsePagesDemoRestoreEntry,
} from './pagesDemoRestore'

export const PUBLIC_DEMO_FIXTURE_STORAGE_KEY = 'dockrev:public-demo:fixture:v1'
export const PUBLIC_DEMO_SCENARIOS = [
  'settings-configured',
  'queue-long-logs',
  'service-action-progress',
  'dashboard-demo-hydrated-update',
] as const
export type PublicDemoScenario = (typeof PUBLIC_DEMO_SCENARIOS)[number]
export const PUBLIC_DEMO_SCENARIO: PublicDemoScenario = 'settings-configured'
export const PUBLIC_DEMO_CLEANUP_SCENARIO = 'cleanup-console-storage-normal'
const PUBLIC_DEMO_VERSION_SERVICE_ID = 'svc-prod-api'
const PUBLIC_DEMO_VERSION_REPO_URL = 'https://github.com/acme/api'

export const PUBLIC_DEMO_GITHUB_RELEASES_BY_SERVICE_ID = {
  [PUBLIC_DEMO_VERSION_SERVICE_ID]: {
    authMode: 'anonymous',
    repo: {
      fullName: 'acme/api',
      htmlUrl: PUBLIC_DEMO_VERSION_REPO_URL,
    },
    items: versionReleaseNotes,
  },
} satisfies Record<string, DockrevMockGitHubReleasesDataset>

type StoredDemoFixture = {
  version: 2
  scenario: PublicDemoScenario
  fixture: Fixture
}

export type PublicDemoSessionSummary = {
  cleanupScenario: typeof PUBLIC_DEMO_CLEANUP_SCENARIO
  fixtureBytes: number
  fixtureState: 'seeded' | 'modified'
  hasStoredFixture: boolean
  routeRestorePending: boolean
  scenario: PublicDemoScenario
  store: 'sessionStorage'
  writes: 'mock-only'
}

function cloneFixture<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

function currentDemoBaseUrl(): string | null {
  if (typeof window === 'undefined') return null
  try {
    return new URL(appBasePath(), window.location.origin).toString()
  } catch {
    return null
  }
}

function parsePublicDemoScenario(value: string | null | undefined): PublicDemoScenario {
  return value != null && PUBLIC_DEMO_SCENARIOS.includes(value as PublicDemoScenario)
    ? (value as PublicDemoScenario)
    : PUBLIC_DEMO_SCENARIO
}

export function readPublicDemoScenario(): PublicDemoScenario {
  if (typeof window === 'undefined') return PUBLIC_DEMO_SCENARIO
  try {
    return parsePublicDemoScenario(new URL(window.location.href).searchParams.get('demoScenario'))
  } catch {
    return PUBLIC_DEMO_SCENARIO
  }
}

function applyPublicDemoVersionOverrides(next: Fixture) {
  const service = next.stackById['stack-prod']?.services.find(
    (item) => item.id === PUBLIC_DEMO_VERSION_SERVICE_ID,
  )
  if (service) {
    service.settings = {
      ...service.settings,
      repoUrl: service.settings.repoUrl ?? PUBLIC_DEMO_VERSION_REPO_URL,
    }
    service.newVersionDiscoveryCount ??= 2
  }

  const serviceSettings = next.serviceSettingsById[PUBLIC_DEMO_VERSION_SERVICE_ID]
  if (serviceSettings) {
    next.serviceSettingsById[PUBLIC_DEMO_VERSION_SERVICE_ID] = {
      ...serviceSettings,
      repoUrl: serviceSettings.repoUrl ?? PUBLIC_DEMO_VERSION_REPO_URL,
    }
  }

  if (!next.rollbackTargetByServiceId[PUBLIC_DEMO_VERSION_SERVICE_ID]) {
    const currentDigest = service?.image.digest ?? ''
    next.rollbackTargetByServiceId[PUBLIC_DEMO_VERSION_SERVICE_ID] = {
      available: true,
      currentDigest,
      currentDisplayTag: service?.image.resolvedTag ?? service?.image.tag ?? '5.2.1',
      targetDigest:
        'sha256:0000000000000000000000000000000000000000000000000000000000000010',
      targetDisplayTag: '5.2.0',
      sourceUpdateJobId: 'job-auto-policy-api-5-2-3',
      sourceFinishedAt: '2026-07-12T13:45:00.000Z',
      unavailableReason: null,
      activeJobId: null,
      activeJobStatus: null,
    }
  }
}

export function applyPublicDemoOverrides(fixture: Fixture): Fixture {
  const next = cloneFixture(fixture)
  applyPublicDemoVersionOverrides(next)
  const demoBaseUrl = currentDemoBaseUrl()
  if (!demoBaseUrl) return next

  next.settings.instance = {
    ...next.settings.instance,
    publicBaseUrl: demoBaseUrl,
  }
  next.githubPackagesSettings = {
    ...next.githubPackagesSettings,
    enabled: true,
    callbackUrl: new URL('api/webhooks/github-packages', demoBaseUrl).toString(),
  }
  next.deployCheckReport = {
    ...next.deployCheckReport,
    report: next.deployCheckReport.report
      ? {
          ...next.deployCheckReport.report,
          overall: {
            ...next.deployCheckReport.report.overall,
            summary: 'Public demo uses seeded mock state and never talks to a live backend.',
          },
        }
      : next.deployCheckReport.report,
  }
  return next
}

export function buildPublicDemoSeedFixture(scenario: PublicDemoScenario = readPublicDemoScenario()): Fixture {
  return applyPublicDemoOverrides(buildFixture(scenario))
}

function readStorage(storage?: Storage | null): Storage | null {
  if (storage) return storage
  if (typeof window === 'undefined') return null
  return window.sessionStorage
}

export function readStoredPublicDemoFixture(
  storage?: Storage | null,
): Fixture | null {
  const target = readStorage(storage)
  if (!target) return null
  const currentScenario = readPublicDemoScenario()
  try {
    const raw = target.getItem(PUBLIC_DEMO_FIXTURE_STORAGE_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw) as Partial<StoredDemoFixture>
    if (parsed.version !== 2 || parsed.scenario !== currentScenario || !parsed.fixture) return null
    return applyPublicDemoOverrides(parsed.fixture)
  } catch {
    return null
  }
}

export function savePublicDemoFixture(
  fixture: Fixture,
  storage?: Storage | null,
) {
  const target = readStorage(storage)
  if (!target) return
  const payload: StoredDemoFixture = {
    version: 2,
    scenario: readPublicDemoScenario(),
    fixture: applyPublicDemoOverrides(fixture),
  }
  target.setItem(PUBLIC_DEMO_FIXTURE_STORAGE_KEY, JSON.stringify(payload))
}

export function loadPublicDemoFixture(storage?: Storage | null): Fixture {
  return readStoredPublicDemoFixture(storage) ?? buildPublicDemoSeedFixture()
}

export function readPublicDemoSessionSummary(
  storage?: Storage | null,
): PublicDemoSessionSummary {
  const target = readStorage(storage)
  const rawStoredFixture = target?.getItem(PUBLIC_DEMO_FIXTURE_STORAGE_KEY) ?? null
  const storedFixture = readStoredPublicDemoFixture(target)
  const seedFixture = buildPublicDemoSeedFixture()
  const routeRestorePending = Boolean(
    parsePagesDemoRestoreEntry(
      target?.getItem(PAGES_DEMO_RESTORE_STORAGE_KEY) ?? null,
    ),
  )

  return {
    cleanupScenario: PUBLIC_DEMO_CLEANUP_SCENARIO,
    fixtureBytes: rawStoredFixture?.length ?? 0,
    fixtureState:
      storedFixture && JSON.stringify(storedFixture) !== JSON.stringify(seedFixture)
        ? 'modified'
        : 'seeded',
    hasStoredFixture: rawStoredFixture != null,
    routeRestorePending,
    scenario: PUBLIC_DEMO_SCENARIO,
    store: 'sessionStorage',
    writes: 'mock-only',
  }
}

export function clearPendingPagesDemoRestoreState(storage?: Storage | null): boolean {
  const target = readStorage(storage)
  if (!target) return false
  const hadPending = target.getItem(PAGES_DEMO_RESTORE_STORAGE_KEY) != null
  target.removeItem(PAGES_DEMO_RESTORE_STORAGE_KEY)
  return hadPending
}

export function resetPublicDemoSessionState(storage?: Storage | null) {
  const target = readStorage(storage)
  target?.removeItem(PUBLIC_DEMO_FIXTURE_STORAGE_KEY)
  target?.removeItem(PAGES_DEMO_RESTORE_STORAGE_KEY)
  if (typeof window === 'undefined') return
  window.location.assign(appBasePath())
}
