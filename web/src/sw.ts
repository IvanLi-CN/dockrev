/// <reference lib="webworker" />

import { clientsClaim } from 'workbox-core'
import { cleanupOutdatedCaches, createHandlerBoundToURL, precacheAndRoute } from 'workbox-precaching'
import { NavigationRoute, registerRoute } from 'workbox-routing'

declare let self: ServiceWorkerGlobalScope & {
  __WB_MANIFEST: Array<{ url: string; revision: string | null }>
}

precacheAndRoute(self.__WB_MANIFEST)
cleanupOutdatedCaches()
clientsClaim()

registerRoute(
  new NavigationRoute(createHandlerBoundToURL('/index.html'), {
    allowlist: [
      /^\/$/,
      /^\/overview$/,
      /^\/services$/,
      /^\/services\/[^/]+$/,
      /^\/services\/[^/]+\/[^/]+(?:\/(?:overview|monitoring|backup|logs|settings))?$/,
      /^\/queue$/,
      /^\/queue\/version-inference$/,
    ],
    denylist: [
      /^\/api(?:\/|$)/,
      /^\/assets(?:\/|$)/,
      /^\/cleanup(?:\/|$)/,
      /^\/settings(?:\/|$)/,
      /^\/deploy-check(?:\/|$)/,
      /^\/queue\/ghcr-webhooks(?:\/|$)/,
      /^\/queue\/ghcr-webhook-inbox(?:\/|$)/,
      /^\/queue\/[^/]+$/,
    ],
  }),
)

self.addEventListener('message', (event) => {
  if (event.data && typeof event.data === 'object' && event.data.type === 'SKIP_WAITING') {
    void self.skipWaiting()
  }
})

self.addEventListener('push', (event) => {
  let data: { title?: string; body?: string; url?: string } = {}
  try {
    data = event.data ? (event.data.json() as typeof data) : {}
  } catch {
    data = { title: 'Dockrev', body: event.data ? event.data.text() : '' }
  }

  const title = data.title || 'Dockrev'
  event.waitUntil(
    self.registration.showNotification(title, {
      body: data.body || '',
      data,
    }),
  )
})

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then(async (clients) => {
      const data =
        event.notification && event.notification.data && typeof event.notification.data === 'object'
          ? (event.notification.data as { url?: string })
          : {}
      const url = typeof data.url === 'string' && data.url.trim().length > 0 ? data.url : null

      if (url) {
        for (const client of clients) {
          if (client && typeof client.focus === 'function' && typeof client.navigate === 'function') {
            try {
              const navigatedClient = await client.navigate(url)
              if (navigatedClient && typeof navigatedClient.focus === 'function') {
                return navigatedClient.focus()
              }
            } catch {
              // Fall through to openWindow.
            }
          }
        }
        if (self.clients.openWindow) return self.clients.openWindow(url)
      }

      for (const client of clients) {
        if (client.url && 'focus' in client) return client.focus()
      }
      if (self.clients.openWindow) return self.clients.openWindow('/')
      return undefined
    }),
  )
})
