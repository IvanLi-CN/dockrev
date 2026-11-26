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

/**
 * Dockrev has a single authorization outcome: the forward-auth identity either
 * matches the allowlist or gets `401 auth_required`. There are no role tiers, so
 * the demo exposes exactly these two states.
 */
export type DemoPersonaId = 'allowed' | 'blocked'

/** Legacy share links used role-like personas; they map onto the real states. */
export const LEGACY_PERSONA_MAP: Record<string, DemoPersonaId> = {
  guest: 'blocked',
  user: 'allowed',
  admin: 'allowed',
}

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
