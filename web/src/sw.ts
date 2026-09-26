/// <reference lib="webworker" />

import { clientsClaim } from 'workbox-core'
import { addPlugins, cleanupOutdatedCaches, createHandlerBoundToURL, precacheAndRoute } from 'workbox-precaching'
import { NavigationRoute, registerRoute } from 'workbox-routing'
import { DYNAMIC_PAGE_TEMPLATES, DYNAMIC_SEGMENT_PATTERN, RESERVED_PREFIXES, STATIC_PAGE_PATHS } from './routeContract'

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

self.addEventListener('push', (event) => {
  let data: { title?: string; body?: string; url?: string; notificationId?: string; unreadCount?: number } = {}
  try {
    data = event.data ? (event.data.json() as typeof data) : {}
  } catch {
    data = { title: 'Dockrev', body: event.data ? event.data.text() : '' }
  }

  const title = data.title || 'Dockrev'
  const badgeRegistration = self.registration as ServiceWorkerRegistration & {
    setAppBadge?: (count?: number) => Promise<void>
    clearAppBadge?: () => Promise<void>
  }
  event.waitUntil(
    (async () => {
      if (
        typeof data.unreadCount === 'number' &&
        Number.isFinite(data.unreadCount) &&
        data.unreadCount >= 0
      ) {
        const unreadCount = Math.max(0, Math.floor(data.unreadCount))
        try {
          if (unreadCount === 0) {
            await badgeRegistration.clearAppBadge?.()
          } else {
            await badgeRegistration.setAppBadge?.(unreadCount)
          }
        } catch {
          // Badging is optional and must not prevent the notification from showing.
        }
      }
      await self.registration.showNotification(title, {
        body: data.body || '',
        data,
      })
    })(),
  )
})

function notificationLaunchUrl(url: string | null, notificationId: string | null): string {
  const launch = new URL(appBasePath || '/', self.registration.scope)
  if (notificationId) launch.searchParams.set('dockrevNotificationId', notificationId)
  if (url) launch.searchParams.set('dockrevNotificationTarget', url)
  return launch.href
}

async function waitForNotificationClickAck(client: WindowClient, notificationId: string, url: string): Promise<boolean> {
  if (typeof MessageChannel === 'undefined') return false
  const channel = new MessageChannel()
  const acknowledgement = new Promise<boolean>((resolve) => {
    const timeout = setTimeout(() => resolve(false), 2500)
    channel.port1.onmessage = (message) => {
      clearTimeout(timeout)
      resolve(message.data?.type === 'DOCKREV_NOTIFICATION_CLICK_ACK' && message.data?.ok === true)
    }
  })
  try {
    client.postMessage(
      { type: 'DOCKREV_NOTIFICATION_CLICK', notificationId, url },
      [channel.port2],
    )
  } catch {
    channel.port1.close()
    return false
  }
  const confirmed = await acknowledgement
  channel.port1.close()
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
      const notificationId =
        event.notification && event.notification.data && typeof event.notification.data === 'object'
          ? (event.notification.data as { notificationId?: string }).notificationId
          : undefined
      const targetUrl = url ? new URL(url, self.registration.scope).href : null

      if (targetUrl && notificationId) {
        for (const client of clients) {
          if (client && typeof client.focus === 'function') {
            try {
              if (await waitForNotificationClickAck(client, notificationId, targetUrl)) {
                return client.focus()
              }
            } catch {
              // Fall through to the cold-start handshake.
            }
          }
        }
        if (self.clients.openWindow) {
          return self.clients.openWindow(notificationLaunchUrl(targetUrl, notificationId))
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
