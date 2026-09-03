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
      if (self.clients.openWindow) return self.clients.openWindow(appBasePath || '/')
      return undefined
    }),
  )
})
