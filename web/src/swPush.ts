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
export const CLICK_ACK = 'DOCKREV_NOTIFICATION_CLICK_ACK'

export function validPushUnreadCount(data: PushNotificationData): number | null {
  if (
    typeof data.unreadCount !== 'number' ||
    !Number.isFinite(data.unreadCount) ||
    !Number.isInteger(data.unreadCount) ||
    data.unreadCount < 0
  ) {
    return null
  }
  return data.unreadCount
}

export function isPushBadgeAcknowledged(data: unknown): boolean {
  return (
    typeof data === 'object' &&
    data !== null &&
    (data as { type?: unknown }).type === PUSH_ACK &&
    (data as { ok?: unknown }).ok === true
  )
}

export function isNotificationClickAcknowledged(data: unknown): boolean {
  return (
    typeof data === 'object' &&
    data !== null &&
    (data as { type?: unknown }).type === CLICK_ACK &&
    (data as { ok?: unknown }).ok === true
  )
}

export async function waitForServiceWorkerAck(
  send: (receive: (data: unknown) => void) => void,
  isAcknowledged: (data: unknown) => boolean,
  timeoutMs: number,
): Promise<boolean> {
  return new Promise((resolve) => {
    let settled = false
    const timeout = setTimeout(() => finish(false), timeoutMs)
    const finish = (value: boolean) => {
      if (settled) return
      settled = true
      clearTimeout(timeout)
      resolve(value)
    }
    try {
      send((data) => finish(isAcknowledged(data)))
    } catch {
      finish(false)
    }
  })
}

export function notificationLaunchUrl(
  scope: string,
  appBasePath: string,
  url: string | null,
  notificationId: string | null,
): string {
  const launch = new URL(appBasePath || '/', scope)
  if (notificationId) launch.searchParams.set('dockrevNotificationId', notificationId)
  if (url) launch.searchParams.set('dockrevNotificationTarget', url)
  return launch.href
}

export function resolveNotificationTargetUrl(
  scopeUrl: string,
  appBasePath: string,
  url: string,
): string | null {
  try {
    const target = new URL(url, scopeUrl)
    const scope = new URL(scopeUrl)
    if (
      target.origin === scope.origin &&
      appBasePath &&
      !target.pathname.startsWith(`${appBasePath}/`) &&
      target.pathname !== appBasePath
    ) {
      target.pathname = `${appBasePath}${target.pathname}`.replace(/\/+/g, '/')
    }
    return target.href
  } catch {
    return null
  }
}
