self.addEventListener('push', (event) => {
  let data = {}
  try {
    data = event.data ? event.data.json() : {}
  } catch (_) {
    data = { title: 'Dockrev', body: event.data ? event.data.text() : '' }
  }

  const title = data.title || 'Dockrev'
  const options = {
    body: data.body || '',
    data,
  }

  event.waitUntil(self.registration.showNotification(title, options))
})

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then(async (clients) => {
      const data = event.notification && event.notification.data ? event.notification.data : {}
      const url = typeof data.url === 'string' && data.url.trim().length ? data.url : null

      if (url) {
        // Prefer reusing an existing tab by navigating it when supported.
        for (const client of clients) {
          if (client && typeof client.focus === 'function') {
            if (typeof client.navigate === 'function') {
              try {
                const navigatedClient = await client.navigate(url)
                if (navigatedClient && typeof navigatedClient.focus === 'function') {
                  return navigatedClient.focus()
                }
              } catch (_) {
                // Fall through to openWindow.
              }
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
