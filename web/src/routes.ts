export type Route =
  | { name: 'overview' }
  | { name: 'queue' }
  | { name: 'services' }
  | { name: 'settings' }
  | { name: 'service'; stackId: string; serviceId: string }

export function parseRoute(pathname: string): Route {
  const parts = pathname.split('/').filter(Boolean).map(decodeURIComponent)
  if (parts.length === 0) return { name: 'overview' }
  if (parts.length === 1 && parts[0] === 'queue') return { name: 'queue' }
  if (parts.length === 1 && parts[0] === 'services') return { name: 'services' }
  if (parts.length === 1 && parts[0] === 'settings') return { name: 'settings' }
  if (parts.length === 3 && parts[0] === 'services') {
    return { name: 'service', stackId: parts[1], serviceId: parts[2] }
  }
  return { name: 'overview' }
}

export function href(route: Route): string {
  switch (route.name) {
    case 'overview':
      return '/'
    case 'queue':
      return '/queue'
    case 'services':
      return '/services'
    case 'settings':
      return '/settings'
    case 'service':
      return `/services/${encodeURIComponent(route.stackId)}/${encodeURIComponent(route.serviceId)}`
  }
}

type NavListener = () => void
const listeners = new Set<NavListener>()

function notify() {
  for (const l of listeners) l()
}

export function navigate(route: Route) {
  const url = href(route)
  window.history.pushState({}, '', url)
  notify()
}

export function subscribeNavigation(listener: NavListener) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function installPopStateListener() {
  window.addEventListener('popstate', notify)
}

