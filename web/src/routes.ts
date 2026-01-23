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

function currentPathname(): string {
  const hash = window.location.hash
  if (hash.startsWith('#/')) return hash.slice(1)
  return window.location.pathname
}

function shouldUseHashRouting(): boolean {
  if (window.location.hash.startsWith('#/')) return true
  // Storybook renders stories inside `iframe.html?...`; pushing pathname would break the preview.
  if (window.location.pathname.endsWith('/iframe.html')) return true
  return false
}

type NavListener = () => void
const listeners = new Set<NavListener>()

function notify() {
  for (const l of listeners) l()
}

export function navigate(route: Route) {
  const url = href(route)
  if (shouldUseHashRouting()) {
    const next = `#${url}`
    if (window.location.hash !== next) {
      window.location.hash = next
    } else {
      notify()
    }
    return
  }

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
  window.addEventListener('hashchange', notify)
}

export function currentRoutePathname(): string {
  return currentPathname()
}

export function currentHref(route: Route): string {
  const url = href(route)
  return shouldUseHashRouting() ? `#${url}` : url
}
