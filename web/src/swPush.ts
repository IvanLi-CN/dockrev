export type PushNotificationData = {
  title?: string
  body?: string
  url?: string
  notificationId?: string
  unreadCount?: number
}

export const PUSH_MESSAGE = 'DOCKREV_NOTIFICATION_PUSH'
export const PUSH_ACK = 'DOCKREV_NOTIFICATION_PUSH_ACK'
export const PUSH_BADGE_TIMEOUT_MS = 1500
export const PUSH_BADGE_FETCH_TIMEOUT_MS = 400

export function validPushUnreadCount(data: PushNotificationData): number | null {
  if (
    typeof data.unreadCount !== 'number' ||
    !Number.isFinite(data.unreadCount) ||
    data.unreadCount < 0
  ) {
    return null
  }
  return Math.max(0, Math.floor(data.unreadCount))
}

export function isPushBadgeAcknowledged(data: unknown): boolean {
  return (
    typeof data === 'object' &&
    data !== null &&
    (data as { type?: unknown }).type === PUSH_ACK &&
    (data as { ok?: unknown }).ok === true
  )
}
