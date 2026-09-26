export type NotificationBadgeNavigator = {
  setAppBadge?: (count?: number) => Promise<void> | void
  clearAppBadge?: () => Promise<void> | void
}

export function normalizeNotificationBadgeCount(count: number): number {
  return Number.isFinite(count) ? Math.max(0, Math.floor(count)) : 0
}

export function setNotificationBadge(count: number): void {
  if (typeof navigator === 'undefined') return
  const target = navigator as typeof navigator & NotificationBadgeNavigator
  const safeCount = normalizeNotificationBadgeCount(count)
  try {
    if (safeCount === 0) {
      void target.clearAppBadge?.()
    } else {
      void target.setAppBadge?.(safeCount)
    }
  } catch {
    // Badging is an optional platform enhancement; the inbox remains authoritative.
  }
}
