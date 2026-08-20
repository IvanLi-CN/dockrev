export type AsyncDataPhase =
  | 'initial-loading'
  | 'ready-empty'
  | 'ready-data'
  | 'refreshing'
  | 'error'
  | 'offline'

export type AsyncDataSource = 'none' | 'live' | 'memory' | 'fresh-snapshot'

export type AsyncDataTrigger = 'user-action' | 'background'
export type AsyncDataRequestIntent = 'initial' | AsyncDataTrigger
export type AsyncFreshnessProfile = 'volatile' | 'operational' | 'configuration'

export const USER_ACTION_OVERLAY_DELAY_MS = 200
export const BACKGROUND_NOTICE_DELAY_MS = 5_000
export const BACKGROUND_NOTICE_SUCCESS_VISIBLE_MS = 1_500
export const ASYNC_GET_DEADLINE_MS = 15_000

export const ASYNC_FRESHNESS_MS: Record<AsyncFreshnessProfile, number> = {
  volatile: 15_000,
  operational: 60_000,
  configuration: 300_000,
}

export function asyncOverlayDelay(trigger: AsyncDataTrigger): number {
  return trigger === 'user-action' ? USER_ACTION_OVERLAY_DELAY_MS : 0
}

export function asyncBackgroundNoticeDelay(): number {
  return BACKGROUND_NOTICE_DELAY_MS
}

export function asyncFreshnessWindow(profile: AsyncFreshnessProfile): number {
  return ASYNC_FRESHNESS_MS[profile]
}

export function isAsyncDataBusy(phase: AsyncDataPhase, trigger: AsyncDataTrigger = 'user-action'): boolean {
  return phase === 'initial-loading' || (phase === 'refreshing' && trigger === 'user-action')
}

export function isAsyncBackgroundRefresh(phase: AsyncDataPhase, trigger: AsyncDataTrigger): boolean {
  return phase === 'refreshing' && trigger === 'background'
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

const DEFAULT_ASYNC_DATA_ERROR = '加载失败，请重试。'
const MAX_ASYNC_DATA_ERROR_LENGTH = 180

export function formatAsyncDataError(error: string | null | undefined): string {
  const raw = error?.trim()
  if (!raw) return DEFAULT_ASYNC_DATA_ERROR

  try {
    const decoded = JSON.parse(raw)
    if (typeof decoded === 'object' && decoded !== null) {
      const record = decoded as Record<string, unknown>
      for (const key of ['error', 'message', 'detail']) {
        const candidate = record[key]
        if (typeof candidate === 'string' && candidate.trim()) {
          return candidate.trim().slice(0, MAX_ASYNC_DATA_ERROR_LENGTH)
        }
      }
    }
  } catch {
    // Plain Error messages are already safe to display.
  }

  return raw.slice(0, MAX_ASYNC_DATA_ERROR_LENGTH)
}
