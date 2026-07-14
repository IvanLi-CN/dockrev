import { selfUpgradeBaseUrl } from './runtimeConfig'
import { stripAppBase, withAppBase } from './appBase'

export type Route =
  | { name: 'overview' }
  | { name: 'queue' }
  | { name: 'job'; jobId: string }
  | { name: 'services' }
  | { name: 'cleanup' }
  | { name: 'version-inference' }
  | { name: 'ghcr-webhooks' }
  | { name: 'ghcr-webhook-inbox' }
  | { name: 'ghcr-webhook-registry' }
  | { name: 'deploy-check' }
  | { name: 'settings' }
  | { name: 'stack'; stackId: string }
  | {
      name: 'service'
      stackId: string
      serviceId: string
      section?: 'overview' | 'history' | 'monitoring' | 'backup' | 'logs' | 'settings'
    }
  | { name: 'supervisor-misroute'; basePath: string; pathname: string }

function normalizeServiceSection(
  value: string | null | undefined,
): 'overview' | 'history' | 'monitoring' | 'backup' | 'logs' | 'settings' | null {
  const section = (value ?? '').trim()
  if (section === '' || section === 'overview') return 'overview'
  if (section === 'history' || section === 'monitoring' || section === 'backup' || section === 'logs' || section === 'settings') return section
  return null
}

export function parseRoute(pathname: string): Route {
  const sup = parseSupervisorMisroute(pathname)
  if (sup) return sup

  const parts = stripAppBase(pathname).split('/').filter(Boolean).map(decodeURIComponent)
  if (parts.length === 0) return { name: 'overview' }
  if (parts.length === 1 && parts[0] === 'queue') return { name: 'queue' }
  if (parts.length === 2 && parts[0] === 'queue' && parts[1] === 'version-inference') {
    return { name: 'version-inference' }
  }
  if (parts.length === 2 && parts[0] === 'queue' && parts[1] === 'ghcr-webhooks') {
    return { name: 'ghcr-webhooks' }
  }
  if (parts.length === 2 && parts[0] === 'queue' && parts[1] === 'ghcr-webhook-inbox') {
    return { name: 'ghcr-webhook-inbox' }
  }
  if (parts.length === 2 && parts[0] === 'settings' && parts[1] === 'ghcr-webhooks') {
    return { name: 'ghcr-webhook-registry' }
  }
  if (parts.length === 2 && parts[0] === 'queue') return { name: 'job', jobId: parts[1] }
  if (parts.length === 1 && parts[0] === 'services') return { name: 'services' }
  if (parts.length === 1 && parts[0] === 'cleanup') return { name: 'cleanup' }
  // Legacy compatibility: keep old path readable after route migration.
  if (parts.length === 1 && parts[0] === 'version-inference') return { name: 'version-inference' }
  if (parts.length === 1 && parts[0] === 'deploy-check') return { name: 'deploy-check' }
  if (parts.length === 1 && parts[0] === 'settings') return { name: 'settings' }
  if (parts.length === 2 && parts[0] === 'services') {
    return { name: 'stack', stackId: parts[1] }
  }
  if (parts.length === 3 && parts[0] === 'services') {
    return { name: 'service', stackId: parts[1], serviceId: parts[2], section: 'overview' }
  }
  if (parts.length === 4 && parts[0] === 'services') {
    const section = normalizeServiceSection(parts[3])
    if (section) {
      return { name: 'service', stackId: parts[1], serviceId: parts[2], section }
    }
  }
  return { name: 'overview' }
}

export function href(route: Route): string {
  const routePath = (() => {
    switch (route.name) {
      case 'overview':
        return '/'
      case 'queue':
        return '/queue'
      case 'job':
        return `/queue/${encodeURIComponent(route.jobId)}`
      case 'services':
        return '/services'
      case 'cleanup':
        return '/cleanup'
      case 'version-inference':
        return '/queue/version-inference'
      case 'ghcr-webhooks':
        return '/queue/ghcr-webhooks'
      case 'ghcr-webhook-inbox':
        return '/queue/ghcr-webhook-inbox'
      case 'ghcr-webhook-registry':
        return '/settings/ghcr-webhooks'
      case 'deploy-check':
        return '/deploy-check'
      case 'settings':
        return '/settings'
      case 'stack':
        return `/services/${encodeURIComponent(route.stackId)}`
      case 'service':
        if (!route.section || route.section === 'overview') {
          return `/services/${encodeURIComponent(route.stackId)}/${encodeURIComponent(route.serviceId)}`
        }
        return `/services/${encodeURIComponent(route.stackId)}/${encodeURIComponent(route.serviceId)}/${route.section}`
      case 'supervisor-misroute': {
        const p = route.basePath.endsWith('/') ? route.basePath : `${route.basePath}/`
        return p
      }
    }
  })()

  if (route.name === 'supervisor-misroute') return routePath
  return withAppBase(routePath)
}

function parseSupervisorMisroute(pathname: string): Route | null {
  try {
    const base = new URL(selfUpgradeBaseUrl(), window.location.href)
    if (base.origin !== window.location.origin) return null
    let basePath = base.pathname
    if (!basePath.startsWith('/')) basePath = `/${basePath}`
    basePath = basePath.replace(/\/+$/, '')
    if (!basePath || basePath === '/' || basePath === '/api') return null

    if (pathname === basePath || pathname.startsWith(`${basePath}/`)) {
      return { name: 'supervisor-misroute', basePath, pathname }
    }
    return null
  } catch {
    return null
  }
}

function currentPathname(): string {
  const hash = window.location.hash
  if (hash.startsWith('#/')) return hash.slice(1)
  return stripAppBase(window.location.pathname)
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
