import { installDockrevMockApi } from '../mock/dockrevMockApi/install'
import { realFetch, type DockrevApiRouteBehavior, type Fixture } from '../mock/dockrevMockApi/shared'
import { buildReadonlySnapshotKey, deleteReadonlySnapshot, writeReadonlySnapshot } from '../readonlySnapshotCache'
import { currentDemoShareState, demoRequestDelayMs, demoScene } from './scenes'
import {
  DEMO_GITHUB_RELEASES_BY_SERVICE_ID,
  appendDemoMutation,
  applyDemoOverrides,
  loadDemoFixture,
  saveDemoFixture,
} from './session'
import type { DemoShareState } from './types'
import { writeDockrevRuntimeMode } from './runtime'

const QUEUE_SNAPSHOT_KEY = buildReadonlySnapshotKey('queue', 'jobs-overview')

/**
 * The environment policy is read on every intercepted request, so connection
 * and latency changes take effect immediately without reinstalling the mock
 * or reloading the document.
 */
let activeEnvironment: DemoShareState | null = null
let recordWrites = false
/** Notifies the store after a simulated write so the inspector re-reads the log. */
let notifyMutation: (() => void) | null = null

function isProductRequest(urlPath: string) {
  return urlPath.startsWith('/api/')
}

/** Scenario-specific async cases stay per-route; environment is global. */
function routeBehaviorFor(state: DemoShareState): Record<string, DockrevApiRouteBehavior> {
  const slowRead = {
    'GET /api/jobs': { delayMs: 3_000 },
    'GET /api/version-inference/overview': { delayMs: 3_000 },
    'GET /api/github-packages/webhook/overview': { delayMs: 3_000 },
  }
  if (state.asyncCase === 'cold' || state.asyncCase === 'cache-refresh') return slowRead
  if (state.asyncCase === 'error') {
    return {
      'GET /api/jobs': {
        delayMs: 350,
        failTimes: 5,
        failureStatus: 503,
        failureBody: { error: '任务队列暂时不可用，请重试。' },
      },
    }
  }
  if (state.asyncCase === 'job-detail-retry') {
    return {
      'GET /api/jobs/job-live-long': {
        failTimes: 2,
        failureStatus: 503,
        failureBody: { error: '任务详情暂时不可用，请重试。' },
      },
    }
  }
  if (state.events === 'reconnecting') {
    return { 'GET /api/events': { failTimes: 100, failureStatus: 503 } }
  }
  return {}
}

function buildQueueSnapshot(fixture: Fixture) {
  const repos = fixture.githubPackagesRepos.filter((repo) => repo.selected)
  const count = (state: string) => repos.filter((repo) => repo.webhookState === state).length
  const summary = fixture.versionInferenceOverview.summary
  return {
    version: 2 as const,
    readiness: { jobs: true, versionInference: true, ghcr: true },
    committedQueryKey: 'all::',
    jobs: fixture.jobs,
    filter: 'all' as const,
    currentCursor: null,
    nextCursor: null,
    cursorStack: [],
    versionInferenceSummary: {
      snapshotsTotal: summary.snapshotsTotal,
      queued: summary.queued,
      running: summary.running,
      ready: summary.ready,
      stale: summary.stale,
      allFailed: summary.allFailed,
    },
    versionInferenceLoaded: true,
    ghcrSummary: {
      tracked: repos.length,
      ok: count('ok'),
      missing: count('missing'),
      error: count('error'),
      conflict: count('conflict'),
      jobsQueued: fixture.jobs.filter((job) => job.type === 'github_packages_webhook' && job.status === 'queued').length,
      jobsRunning: fixture.jobs.filter((job) => job.type === 'github_packages_webhook' && job.status === 'running').length,
    },
    ghcrLoaded: true,
  }
}

/**
 * Wraps the installed mock fetch with the environment policy:
 *  - offline fails product requests the way a browser does (TypeError), never
 *    with a fake 503 and never by falling back to the live backend;
 *  - latency adds one documented delay per intercepted product request.
 */
function installEnvironmentPolicy() {
  const mockFetch = globalThis.fetch
  globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
    const environment = activeEnvironment
    const method = (init?.method ?? (input instanceof Request ? input.method : 'GET')).toUpperCase()
    const rawUrl = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url
    const urlPath = (() => {
      try {
        return new URL(rawUrl, window.location.href).pathname
      } catch {
        return rawUrl
      }
    })()

    if (environment && isProductRequest(urlPath)) {
      if (environment.connection === 'offline') {
        throw new TypeError('Failed to fetch')
      }
      const delayMs = demoRequestDelayMs(environment)
      if (delayMs > 0) {
        await new Promise<void>((resolve) => {
          globalThis.setTimeout(resolve, delayMs)
        })
      }
    }

    const response = await mockFetch(input, init)

    if (recordWrites && environment && method !== 'GET' && isProductRequest(urlPath) && response.ok) {
      appendDemoMutation({
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        at: new Date().toISOString(),
        label: `${method} ${urlPath}`,
        detail: '写入内存 mock，未触达真实后端',
        route: `${window.location.pathname}${window.location.search}`,
      })
      notifyMutation?.()
    }
    return response
  }
}

export type DemoInstallResult = { enabled: true; shareState: DemoShareState }

/**
 * A demo build must intercept before the first render. Anything less would let
 * the app tree issue live requests against a non-existent backend.
 */
export async function installDemoRuntime(
  shareState: DemoShareState = currentDemoShareState(),
  onMutation?: () => void,
): Promise<DemoInstallResult> {
  const initialFixture = applyDemoOverrides(loadDemoFixture(shareState), shareState)
  saveDemoFixture(initialFixture, shareState)

  if (shareState.asyncCase === 'cold') {
    await deleteReadonlySnapshot(QUEUE_SNAPSHOT_KEY)
  } else if (shareState.asyncCase === 'cache-refresh') {
    await writeReadonlySnapshot(QUEUE_SNAPSHOT_KEY, buildQueueSnapshot(initialFixture), { staleAfterMs: 60_000 })
  }

  writeDockrevRuntimeMode('app-demo')
  installDockrevMockApi(demoScene(shareState.sceneId).fixtureScenario, {
    cleanupScenario: shareState.cleanup,
    githubReleasesByServiceId: DEMO_GITHUB_RELEASES_BY_SERVICE_ID,
    initialFixture,
    onStateChange: (fixture) => {
      saveDemoFixture(fixture, shareState)
      onMutation?.()
    },
    dockrevApiBehaviorByRoute: routeBehaviorFor(shareState),
  })

  if (globalThis.fetch === realFetch) {
    throw new Error('demo mock layer did not take over fetch; refusing to render the live app tree')
  }
  if (!window.__DOCKREV_MOCK_DEBUG__) {
    throw new Error('demo mock layer published no debug state; refusing to render the live app tree')
  }

  activeEnvironment = shareState
  notifyMutation = onMutation ?? null
  recordWrites = true
  installEnvironmentPolicy()
  return { enabled: true, shareState }
}

/** Applies a connection/latency change to the already-installed mock. */
export function applyDemoEnvironment(shareState: DemoShareState) {
  activeEnvironment = shareState
}

/** Keeps the inspector's mutation observer attached across model rebuilds. */
export function setDemoMutationObserver(observer: (() => void) | null) {
  notifyMutation = observer
}

export function currentDemoEnvironment(): DemoShareState | null {
  return activeEnvironment
}
