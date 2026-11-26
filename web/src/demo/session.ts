import { buildFixture } from '../mock/dockrevMockApi/fixturesMisc'
import { versionReleaseNotes } from '../mock/serviceDetailFixtures'
import type {
  DockrevMockGitHubReleasesDataset,
  Fixture,
} from '../mock/dockrevMockApi/shared'
import { demoScene } from './scenes'
import type { DemoMutationRecord, DemoSceneId, DemoSessionSnapshot, DemoShareState } from './types'

export const DEMO_FIXTURE_STORAGE_KEY = 'dockrev:demo:fixture:v1'
export const DEMO_MUTATIONS_STORAGE_KEY = 'dockrev:demo:mutations:v1'
export const DEMO_INSPECTOR_LAYOUT_STORAGE_KEY = 'dockrev:demo:inspector-layout:v1'
const MUTATION_LIMIT = 12
const VERSION_SERVICE_ID = 'svc-prod-api'
const VERSION_REPO_URL = 'https://github.com/acme/api'

export const DEMO_GITHUB_RELEASES_BY_SERVICE_ID = {
  [VERSION_SERVICE_ID]: {
    authMode: 'anonymous',
    repo: {
      fullName: 'acme/api',
      htmlUrl: VERSION_REPO_URL,
    },
    items: versionReleaseNotes,
  },
} satisfies Record<string, DockrevMockGitHubReleasesDataset>

type StoredDemoFixture = {
  version: 2
  sceneId: DemoSceneId
  fixture: Fixture
}

function cloneFixture<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

function demoStorage(): Storage | null {
  if (typeof window === 'undefined') return null
  return window.sessionStorage
}

function currentDemoBaseUrl(): string | null {
  if (typeof window === 'undefined') return null
  try {
    return new URL(import.meta.env.BASE_URL ?? '/', window.location.origin).toString()
  } catch {
    return null
  }
}

function seedVersionService(next: Fixture) {
  const service = next.stackById['stack-prod']?.services.find((item) => item.id === VERSION_SERVICE_ID)
  if (service) {
    service.settings = {
      ...service.settings,
      repoUrl: service.settings.repoUrl ?? VERSION_REPO_URL,
    }
    service.newVersionDiscoveryCount ??= 2
  }
  const serviceSettings = next.serviceSettingsById[VERSION_SERVICE_ID]
  if (serviceSettings) {
    next.serviceSettingsById[VERSION_SERVICE_ID] = {
      ...serviceSettings,
      repoUrl: serviceSettings.repoUrl ?? VERSION_REPO_URL,
    }
  }
  if (!next.rollbackTargetByServiceId[VERSION_SERVICE_ID]) {
    next.rollbackTargetByServiceId[VERSION_SERVICE_ID] = {
      available: true,
      currentDigest: service?.image.digest ?? '',
      currentDisplayTag: service?.image.resolvedTag ?? service?.image.tag ?? '5.2.1',
      targetDigest: 'sha256:0000000000000000000000000000000000000000000000000000000000000010',
      targetDisplayTag: '5.2.0',
      sourceUpdateJobId: 'job-auto-policy-api-5-2-3',
      sourceFinishedAt: '2026-07-12T13:45:00.000Z',
      unavailableReason: null,
      activeJobId: null,
      activeJobStatus: null,
    }
  }
}

/**
 * Demo fixtures must describe the demo site itself, not a live deployment:
 * public base URL, webhook callback and the deploy-check summary all point at
 * the mock surface so a reviewer never mistakes a seeded value for their host.
 */
export function applyDemoOverrides(fixture: Fixture, shareState: DemoShareState): Fixture {
  const next = cloneFixture(fixture)
  seedVersionService(next)

  // The persona maps onto the product's own auth fields: guest has no user
  // object, `user` is signed in without admin rights, `admin` gains the demo
  // admin group. Nothing here grants real access.
  const personaAuth =
    shareState.persona === 'guest'
      ? { currentUser: null, currentGroups: [] as string[], matchedBy: 'anonymous_dev' }
      : shareState.persona === 'admin'
        ? { currentUser: 'demo-admin', currentGroups: ['demo-admins'], matchedBy: 'group' }
        : { currentUser: 'demo-user', currentGroups: [] as string[], matchedBy: 'user' }
  next.settings.auth = {
    ...next.settings.auth,
    ...personaAuth,
    avatarUrl: null,
  }

  const demoBaseUrl = currentDemoBaseUrl()
  if (!demoBaseUrl) return next
  next.settings.instance = { ...next.settings.instance, publicBaseUrl: demoBaseUrl }
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

export function buildDemoSeedFixture(shareState: DemoShareState): Fixture {
  return applyDemoOverrides(buildFixture(demoScene(shareState.sceneId).fixtureScenario), shareState)
}

export function readStoredDemoFixture(shareState: DemoShareState): Fixture | null {
  const storage = demoStorage()
  if (!storage) return null
  try {
    const raw = storage.getItem(DEMO_FIXTURE_STORAGE_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw) as Partial<StoredDemoFixture>
    if (parsed.version !== 2 || parsed.sceneId !== shareState.sceneId || !parsed.fixture) return null
    return applyDemoOverrides(parsed.fixture, shareState)
  } catch {
    return null
  }
}

export function saveDemoFixture(fixture: Fixture, shareState: DemoShareState) {
  const storage = demoStorage()
  if (!storage) return
  const payload: StoredDemoFixture = {
    version: 2,
    sceneId: shareState.sceneId,
    fixture: applyDemoOverrides(fixture, shareState),
  }
  try {
    storage.setItem(DEMO_FIXTURE_STORAGE_KEY, JSON.stringify(payload))
  } catch {
    // Quota failures must not break the demo runtime.
  }
}

export function loadDemoFixture(shareState: DemoShareState): Fixture {
  return readStoredDemoFixture(shareState) ?? buildDemoSeedFixture(shareState)
}

export function clearDemoFixture() {
  demoStorage()?.removeItem(DEMO_FIXTURE_STORAGE_KEY)
}

export function readDemoMutations(): DemoMutationRecord[] {
  const storage = demoStorage()
  if (!storage) return []
  try {
    const raw = storage.getItem(DEMO_MUTATIONS_STORAGE_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return []
    return parsed.filter(
      (item): item is DemoMutationRecord =>
        Boolean(item) &&
        typeof (item as DemoMutationRecord).id === 'string' &&
        typeof (item as DemoMutationRecord).label === 'string' &&
        typeof (item as DemoMutationRecord).detail === 'string' &&
        typeof (item as DemoMutationRecord).route === 'string' &&
        typeof (item as DemoMutationRecord).at === 'string',
    )
  } catch {
    return []
  }
}

export function appendDemoMutation(record: DemoMutationRecord): DemoMutationRecord[] {
  const next = [...readDemoMutations(), record].slice(-MUTATION_LIMIT)
  try {
    demoStorage()?.setItem(DEMO_MUTATIONS_STORAGE_KEY, JSON.stringify(next))
  } catch {
    // Best-effort audit trail only.
  }
  return next
}

export function clearDemoMutations() {
  demoStorage()?.removeItem(DEMO_MUTATIONS_STORAGE_KEY)
}

export function demoSessionSnapshot(shareState: DemoShareState): DemoSessionSnapshot {
  const storage = demoStorage()
  const raw = storage?.getItem(DEMO_FIXTURE_STORAGE_KEY) ?? null
  const stored = readStoredDemoFixture(shareState)
  return {
    shareState,
    fixtureState:
      stored && JSON.stringify(stored) !== JSON.stringify(buildDemoSeedFixture(shareState))
        ? 'modified'
        : 'seeded',
    fixtureBytes: raw?.length ?? 0,
    mutations: readDemoMutations(),
  }
}
