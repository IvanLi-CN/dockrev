export type NotificationBadgeNavigator = {
  setAppBadge?: (count?: number) => Promise<void> | void
  clearAppBadge?: () => Promise<void> | void
}

export function normalizeNotificationBadgeCount(count: number): number {
  return Number.isFinite(count) ? Math.max(0, Math.floor(count)) : 0
}

let badgeWriteChain = Promise.resolve()

export function setNotificationBadge(count: number): void {
  if (typeof navigator === 'undefined') return
  const target = navigator as typeof navigator & NotificationBadgeNavigator
  const safeCount = normalizeNotificationBadgeCount(count)
  badgeWriteChain = badgeWriteChain
    .then(async () => {
      if (safeCount === 0) {
        await target.clearAppBadge?.()
      } else {
        await target.setAppBadge?.(safeCount)
      }
    })
    .catch(() => undefined)
}
