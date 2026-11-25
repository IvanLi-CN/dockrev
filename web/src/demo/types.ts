import type { Fixture } from '../mock/dockrevMockApi/shared'

export type DemoSceneId =
  | 'overview'
  | 'services'
  | 'service-actions'
  | 'service-versions'
  | 'queue'
  | 'queue-logs'
  | 'settings'
  | 'ghcr-registry'
  | 'cleanup'
  | 'deploy-check'

export type DemoPersonaId = 'anonymous' | 'admin'

export type DemoNetworkMode = 'normal' | 'slow' | 'faulty'

export type DemoAsyncCase =
  | 'default'
  | 'cold'
  | 'cache-refresh'
  | 'error'
  | 'job-detail-retry'

export type DemoManagementEventsMode = 'default' | 'reconnecting'

export type DemoCleanupScenario =
  | 'cleanup-console-storage-normal'
  | 'cleanup-console-confirm-pending'
  | 'cleanup-console-confirm-failed'

/**
 * Everything a reviewer can reproduce from a URL. Scene selection and share
 * state live in the query string; panel geometry stays local-only.
 */
export type DemoShareState = {
  sceneId: DemoSceneId
  persona: DemoPersonaId
  network: DemoNetworkMode
  asyncCase: DemoAsyncCase
  events: DemoManagementEventsMode
  cleanup: DemoCleanupScenario
  controlsHidden: boolean
}

export type DemoShareStatePatch = Partial<DemoShareState>

export type DemoMutationRecord = {
  id: string
  at: string
  label: string
  route: string
}

export type DemoSceneRuntimeState = {
  shareState: DemoShareState
  fixture: Fixture | null
  mutations: DemoMutationRecord[]
}

export type DemoSessionSnapshot = {
  shareState: DemoShareState
  fixtureState: 'seeded' | 'modified'
  fixtureBytes: number
  mutations: DemoMutationRecord[]
}
