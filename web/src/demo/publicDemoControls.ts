import { appBasePath } from '../appBase'
import { buildFixture } from '../stories/mocks/dockrevMockApi/fixturesMisc'
import type { Fixture } from '../stories/mocks/dockrevMockApi/shared'
import {
  PAGES_DEMO_RESTORE_STORAGE_KEY,
  parsePagesDemoRestoreEntry,
} from './pagesDemoRestore'

export const PUBLIC_DEMO_FIXTURE_STORAGE_KEY = 'dockrev:public-demo:fixture:v1'
export const PUBLIC_DEMO_SCENARIO = 'settings-configured'
export const PUBLIC_DEMO_CLEANUP_SCENARIO = 'cleanup-console-storage-normal'

type StoredDemoFixture = {
  version: 1
  fixture: Fixture
}

export type PublicDemoSessionSummary = {
  cleanupScenario: typeof PUBLIC_DEMO_CLEANUP_SCENARIO
  fixtureBytes: number
  fixtureState: 'seeded' | 'modified'
  hasStoredFixture: boolean
  routeRestorePending: boolean
  scenario: typeof PUBLIC_DEMO_SCENARIO
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

export function applyPublicDemoOverrides(fixture: Fixture): Fixture {
  const next = cloneFixture(fixture)
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

export function buildPublicDemoSeedFixture(): Fixture {
  return applyPublicDemoOverrides(buildFixture(PUBLIC_DEMO_SCENARIO))
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
  try {
    const raw = target.getItem(PUBLIC_DEMO_FIXTURE_STORAGE_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw) as Partial<StoredDemoFixture>
    if (parsed.version !== 1 || !parsed.fixture) return null
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
    version: 1,
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
