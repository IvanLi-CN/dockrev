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
  event.waitUntil(self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((clients) => {
    for (const client of clients) {
      if (client.url && 'focus' in client) return client.focus()
    }
    if (self.clients.openWindow) return self.clients.openWindow('/')
    return undefined
  }))
})

