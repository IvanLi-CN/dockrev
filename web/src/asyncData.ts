export type AsyncDataPhase =
  | 'initial-loading'
  | 'ready-empty'
  | 'ready-data'
  | 'refreshing'
  | 'error'
  | 'offline'

export type AsyncDataSource = 'none' | 'live' | 'memory' | 'fresh-snapshot'

export type AsyncDataTrigger = 'user-action' | 'background'

export const USER_ACTION_OVERLAY_DELAY_MS = 200
export const BACKGROUND_OVERLAY_DELAY_MS = 800

export function asyncOverlayDelay(trigger: AsyncDataTrigger): number {
  return trigger === 'background' ? BACKGROUND_OVERLAY_DELAY_MS : USER_ACTION_OVERLAY_DELAY_MS
}

export function isAsyncDataBusy(phase: AsyncDataPhase): boolean {
  return phase === 'initial-loading' || phase === 'refreshing'
}

export function canShowAsyncEmpty(phase: AsyncDataPhase): boolean {
  return phase === 'ready-empty'
}

export function isAsyncDataOffline(phase: AsyncDataPhase, isOnline: boolean): boolean {
  return phase === 'offline' && !isOnline
}

export function hasCompleteAsyncReadiness(readiness: Record<string, unknown>, domains: readonly string[]): boolean {
  return domains.every((domain) => readiness[domain] === true)
}
