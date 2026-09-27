/// <reference lib="webworker" />

import { clientsClaim } from 'workbox-core'
import { addPlugins, cleanupOutdatedCaches, createHandlerBoundToURL, precacheAndRoute } from 'workbox-precaching'
import { NavigationRoute, registerRoute } from 'workbox-routing'
import { DYNAMIC_PAGE_TEMPLATES, DYNAMIC_SEGMENT_PATTERN, RESERVED_PREFIXES, STATIC_PAGE_PATHS } from './routeContract'
import {
  PUSH_BADGE_TIMEOUT_MS,
  PUSH_BADGE_FETCH_TIMEOUT_MS,
  CLICK_CANCEL,
  PUSH_MESSAGE,
  isNotificationClickAcknowledged,
  isPushBadgeAcknowledged,
  notificationLaunchUrl,
  resolveNotificationTargetUrl,
  type PushNotificationData,
  validPushUnreadCount,
  isNotificationClickCancelAcknowledged,
  waitForServiceWorkerAck,
} from './swPush'

declare let self: ServiceWorkerGlobalScope & {
  __WB_MANIFEST: Array<{ url: string; revision: string | null }>
}

addPlugins([
  {
    async requestWillFetch({ request }) {
      // Browsers without Fetch Priority ignore this progressive enhancement.
      return new Request(request, { priority: 'low' })
    },
  },
])

precacheAndRoute(self.__WB_MANIFEST)
cleanupOutdatedCaches()
clientsClaim()

const appBasePath = new URL('./', self.registration.scope).pathname.replace(/\/$/, '')
const appShellUrl = `${appBasePath || ''}/index.html`
const escapedBase = appBasePath.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
const escapedPath = (path: string) => path.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
const appRoute = (pattern: string) => new RegExp(`^${escapedBase}${pattern}`)
const contractTemplateRoute = (template: string) => {
  const path = template
    .split('/')
    .filter(Boolean)
    .map((part) => (part.startsWith(':') ? DYNAMIC_SEGMENT_PATTERN : escapedPath(part)))
    .join('/')
  return appRoute(`/${path}\\/?$`)
}
const navigationAllowlist = [
  ...STATIC_PAGE_PATHS.map((path) => appRoute(path === '/' ? '/?$' : `${escapedPath(path)}\\/?$`)),
  ...DYNAMIC_PAGE_TEMPLATES.map(contractTemplateRoute),
]

registerRoute(
  new NavigationRoute(createHandlerBoundToURL(appShellUrl), {
    allowlist: [
      ...navigationAllowlist,
    ],
    denylist: RESERVED_PREFIXES.map((prefix) => appRoute(`${escapedPath(prefix)}(?:/|$)`)),
  }),
)

self.addEventListener('message', (event) => {
  if (event.data && typeof event.data === 'object' && event.data.type === 'SKIP_WAITING') {
    void self.skipWaiting()
  }
})

async function authoritativePushUnreadCount(data: PushNotificationData): Promise<number | null> {
  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), PUSH_BADGE_FETCH_TIMEOUT_MS)
  try {
    const response = await fetch(
      new URL(`${appBasePath || ''}/api/notifications/unread-count`, self.registration.scope),
      {
        credentials: 'include',
        headers: { Accept: 'application/json' },
        signal: controller.signal,
      },
    )
    if (response.ok) {
      const payload = (await response.json()) as { unreadCount?: unknown }
      const unreadCount = validPushUnreadCount(payload as PushNotificationData)
      if (unreadCount != null) return unreadCount
    }
  } catch {
    // Fall back to the signed absolute payload when the service is unreachable.
  } finally {
    clearTimeout(timeout)
  }
  return validPushUnreadCount(data)
}

async function waitForPushBadgeClaim(client: WindowClient, notificationId: string): Promise<boolean> {
  if (typeof MessageChannel === 'undefined') return false
  const channel = new MessageChannel()
  const claimed = await waitForServiceWorkerAck(
    (receive) => {
      channel.port1.onmessage = (message) => receive(message.data)
      client.postMessage({ type: PUSH_MESSAGE, notificationId }, [channel.port2])
    },
    isPushBadgeAcknowledged,
    1000,
  )
  channel.port1.close()
  return claimed
}

async function applyPushBadge(data: PushNotificationData): Promise<void> {
  const clients = await self.clients.matchAll({ type: 'window', includeUncontrolled: true })
  if (clients.length > 0) {
    const claimed = await Promise.all(
      typeof data.notificationId === 'string'
        ? clients.map((client) => waitForPushBadgeClaim(client, data.notificationId!))
        : [],
    )
    if (claimed.some(Boolean)) return
  }
  const unreadCount = await authoritativePushUnreadCount(data)
  if (unreadCount == null) return
  const badgeNavigator = navigator as WorkerNavigator & {
    setAppBadge?: (count?: number) => Promise<void>
    clearAppBadge?: () => Promise<void>
  }
  try {
    if (unreadCount === 0) {
      await badgeNavigator.clearAppBadge?.()
    } else {
      await badgeNavigator.setAppBadge?.(unreadCount)
    }
  } catch {
    // Badging is optional and must not prevent the notification from showing.
  }
}

self.addEventListener('push', (event) => {
  let data: PushNotificationData = {}
  try {
    const parsed = event.data?.json() as unknown
    data = parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? (parsed as typeof data) : {}
  } catch {
    data = { title: 'Dockrev', body: event.data ? event.data.text() : '' }
  }

  const title = data.title || 'Dockrev'
  event.waitUntil(
    (async () => {
      if (
        typeof data.notificationId === 'string' &&
        data.notificationId.trim().length > 0 &&
        validPushUnreadCount(data) != null
      ) {
        await self.registration.showNotification(title, {
          body: data.body || '',
          tag:
            typeof data.notificationId === 'string' && data.notificationId.trim().length > 0
              ? `dockrev-${data.notificationId}`
              : undefined,
          data,
        })
        await Promise.race([
          applyPushBadge(data).catch(() => undefined),
          new Promise<void>((resolve) => setTimeout(resolve, PUSH_BADGE_TIMEOUT_MS)),
        ])
        return
      }
      await self.registration.showNotification(title, {
        body: data.body || '',
        tag:
          typeof data.notificationId === 'string' && data.notificationId.trim().length > 0
            ? `dockrev-${data.notificationId}`
            : undefined,
        data,
      })
    })(),
  )
})

async function waitForNotificationClickAck(client: WindowClient, notificationId: string, url: string): Promise<boolean> {
  if (typeof MessageChannel === 'undefined') return false
  const channel = new MessageChannel()
  const requestId = `notification-click-${Date.now()}-${Math.random()}`
  const confirmed = await waitForServiceWorkerAck(
    (receive) => {
      channel.port1.onmessage = (message) => receive(message.data)
      client.postMessage(
        { type: 'DOCKREV_NOTIFICATION_CLICK', notificationId, url, requestId },
        [channel.port2],
      )
    },
    isNotificationClickAcknowledged,
    2500,
  )
  channel.port1.close()
  if (!confirmed) {
    const cancelChannel = new MessageChannel()
    await waitForServiceWorkerAck(
      (receive) => {
        cancelChannel.port1.onmessage = (message) => receive(message.data)
        client.postMessage(
          { type: CLICK_CANCEL, requestId },
          [cancelChannel.port2],
        )
      },
      isNotificationClickCancelAcknowledged,
      250,
    )
    cancelChannel.port1.close()
  }
  return confirmed
}

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then(async (clients) => {
      const data =
        event.notification && event.notification.data && typeof event.notification.data === 'object'
          ? (event.notification.data as { url?: string })
          : {}
      const url = typeof data.url === 'string' && data.url.trim().length > 0 ? data.url : null
      const rawNotificationId =
        event.notification && event.notification.data && typeof event.notification.data === 'object'
          ? (event.notification.data as { notificationId?: string }).notificationId
          : null
      const notificationId =
        typeof rawNotificationId === 'string' && rawNotificationId.trim().length > 0
          ? rawNotificationId
          : null
      let targetUrl: string | null = null
      if (url) {
        targetUrl = resolveNotificationTargetUrl(self.registration.scope, appBasePath, url)
      }

      if (targetUrl && notificationId) {
        for (const client of clients) {
          if (client && typeof client.focus === 'function') {
            try {
              if (await waitForNotificationClickAck(client, notificationId, targetUrl)) {
                try {
                  await client.focus()
                  return client.navigate(targetUrl)
                } catch {
                  return client.focus()
                }
              }
            } catch {
              // Fall through to the cold-start handshake.
            }
          }
        }
        if (self.clients.openWindow) {
          return self.clients.openWindow(
            notificationLaunchUrl(self.registration.scope, appBasePath, targetUrl, notificationId),
          )
        }
      } else if (targetUrl && self.clients.openWindow) {
        return self.clients.openWindow(targetUrl)
      }

      for (const client of clients) {
        if (client.url && 'focus' in client) return client.focus()
      }
      if (self.clients.openWindow) return self.clients.openWindow(appBasePath || '/')
      return undefined
    }),
  )
})
