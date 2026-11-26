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

/** Simulated session, mapped onto the product's own auth fields. */
export type DemoPersonaId = 'guest' | 'user' | 'admin'

export type DemoConnection = 'online' | 'offline'

export type DemoDelayMode = 'normal' | 'slow' | 'custom'

export type DemoMotionPreference = 'system' | 'reduce'

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
 * Everything a reviewer can reproduce from a URL. Scene and environment state
 * live in the query string; panel geometry and open/closed state stay local.
 *
 * `theme` is not duplicated here: the product owns the preference and the
 * inspector reads/writes it, so a single source stays authoritative.
 */
export type DemoShareState = {
  sceneId: DemoSceneId
  persona: DemoPersonaId
  connection: DemoConnection
  delay: DemoDelayMode
  delayMs: number
  motion: DemoMotionPreference
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
  detail: string
  route: string
}

export type DemoSessionSnapshot = {
  shareState: DemoShareState
  fixtureState: 'seeded' | 'modified'
  fixtureBytes: number
  mutations: DemoMutationRecord[]
}
