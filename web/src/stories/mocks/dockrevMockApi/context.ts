import type {
  NewVersionDiscoveryTimelineResponse,
  ServiceGitHubReleasesResponse,
  StackDetail,
} from '../../../api'
import type { CleanupMockRuntimeState } from '../cleanupMockData'
import type {
  DockrevApiScenario,
  DockrevMockGitHubReleasesDataset,
  Fixture,
  MockDebug,
  MockServiceLogEventGateState,
} from './shared'

export type FindServiceResult = {
  stack: StackDetail
  svc: StackDetail['services'][number]
} | null

export type MockRouteContext = {
  scenario: DockrevApiScenario
  state: Fixture
  method: string
  init?: RequestInit
  url: URL | null
  urlPath: string
  urlPathWithQuery: string
  urlString: string
  json: (data: unknown, init?: ResponseInit) => Response
  parseJsonBody: (body: unknown) => unknown
  getString: (value: unknown) => string | null
  getBoolean: (value: unknown) => boolean | null
  isRecord: (value: unknown) => value is Record<string, unknown>
  nowIso: (offsetMs?: number) => string
  makeMockDebug: () => MockDebug
  findService: (serviceId: string) => FindServiceResult
  normalizeDigestValue: (value: string | null | undefined) => string
  buildMockDigestTagData: (
    serviceId: string,
    imageTag: string,
    digestNorm: string,
    refreshed: boolean,
  ) => { repoTags: string[]; tags: string[] }
  buildMockDiscoveryTimeline: (serviceId: string) => NewVersionDiscoveryTimelineResponse
  buildMockGitHubReleasesResponse: (
    serviceId: string,
    page: number,
    perPage: number,
  ) => ServiceGitHubReleasesResponse
  buildMockGitHubReleasesDataset: (
    serviceId: string,
  ) => DockrevMockGitHubReleasesDataset
  applyMockUpdateSettlement: (
    serviceId: string,
    targetTag: string,
    targetDigest: string,
    pullTags: string[],
  ) => void
  selectUpdateServiceIds: (scope: string, stackId: string | null, serviceId: string | null) => string[]
  syncStackListItem: (stackId: string) => void
  advanceQueueProgressDemo: () => number | null
  ignoreSeqRef: { value: number }
  jobSeqRef: { value: number }
  jobsEventsSeqRef: { value: number }
  digestSnapshotPendingAttempts: Map<string, number>
  forcedDigestSnapshotPendingAttempts: Map<string, number>
  cleanupRuntime: CleanupMockRuntimeState
  serviceLogEventGates: MockServiceLogEventGateState
}
