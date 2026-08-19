export type AsyncDataPhase =
  | 'initial-loading'
  | 'ready-empty'
  | 'ready-data'
  | 'refreshing'
  | 'error'
  | 'offline'

export type AsyncDataSource = 'none' | 'live' | 'memory' | 'fresh-snapshot'

export const USER_ACTION_OVERLAY_DELAY_MS = 200
export const BACKGROUND_OVERLAY_DELAY_MS = 800

export function asyncOverlayDelay(source: AsyncDataSource): number {
  return source === 'fresh-snapshot' ? BACKGROUND_OVERLAY_DELAY_MS : USER_ACTION_OVERLAY_DELAY_MS
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
