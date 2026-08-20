import { writeDockrevRuntimeMode } from './runtime'
import { installDockrevMockApi } from '../stories/mocks/dockrevMockApi/install'
import { buildReadonlySnapshotKey, deleteReadonlySnapshot, writeReadonlySnapshot } from '../readonlySnapshotCache'
import {
  loadPublicDemoFixture,
  PUBLIC_DEMO_CLEANUP_SCENARIO,
  PUBLIC_DEMO_GITHUB_RELEASES_BY_SERVICE_ID,
  readPublicDemoAsyncState,
  readPublicDemoScenario,
  savePublicDemoFixture,
} from './publicDemoControls'
import type { Fixture } from '../stories/mocks/dockrevMockApi/shared'

type DemoInstallResult = {
  enabled: boolean
  mode: 'app'
}

let installed = false

const QUEUE_SNAPSHOT_KEY = buildReadonlySnapshotKey('queue', 'jobs-overview')

function queueSummarySnapshot(fixture: Fixture) {
  const repos = fixture.githubPackagesRepos.filter((repo) => repo.selected)
  const count = (state: string) => repos.filter((repo) => repo.webhookState === state).length
  const versionSummary = fixture.versionInferenceOverview.summary
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
      snapshotsTotal: versionSummary.snapshotsTotal,
      queued: versionSummary.queued,
      running: versionSummary.running,
      ready: versionSummary.ready,
      stale: versionSummary.stale,
      allFailed: versionSummary.allFailed,
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

function asyncBehavior(state: ReturnType<typeof readPublicDemoAsyncState>) {
  const delayedQueueRead = {
    'GET /api/jobs': { delayMs: 3_000 },
    'GET /api/version-inference/overview': { delayMs: 3_000 },
    'GET /api/github-packages/webhook/overview': { delayMs: 3_000 },
  }
  if (state === 'cold' || state === 'cache-refresh') return delayedQueueRead
  if (state === 'error') {
    return {
      'GET /api/jobs': {
        delayMs: 350,
        // The development shell starts five initial readers; all must fail so
        // the error overlay remains visible until an explicit retry.
        failTimes: 5,
        failureStatus: 503,
        failureBody: { error: '任务队列暂时不可用，请重试。' },
      },
    }
  }
  return undefined
}

export async function installAppDemoApi(): Promise<DemoInstallResult> {
  if (installed) return { enabled: true, mode: 'app' }

  const scenario = readPublicDemoScenario()
  const asyncState = readPublicDemoAsyncState()
  const initialFixture = loadPublicDemoFixture()
  savePublicDemoFixture(initialFixture)
  if (asyncState === 'cold') {
    // A deterministic cold-start demo cannot inherit a prior local preview snapshot.
    await deleteReadonlySnapshot(QUEUE_SNAPSHOT_KEY)
  } else if (asyncState === 'cache-refresh') {
    await writeReadonlySnapshot(QUEUE_SNAPSHOT_KEY, queueSummarySnapshot(initialFixture), {
      staleAfterMs: 60_000,
    })
  }
  writeDockrevRuntimeMode('app-demo')
  installDockrevMockApi(scenario, {
    cleanupScenario: PUBLIC_DEMO_CLEANUP_SCENARIO,
    githubReleasesByServiceId: PUBLIC_DEMO_GITHUB_RELEASES_BY_SERVICE_ID,
    initialFixture,
    onStateChange: savePublicDemoFixture,
    dockrevApiBehaviorByRoute: asyncBehavior(asyncState),
  })
  installed = true
  return { enabled: true, mode: 'app' }
}
