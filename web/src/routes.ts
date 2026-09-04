import { selfUpgradeBaseUrl } from './runtimeConfig'
import { stripAppBase, withAppBase } from './appBase'
import { isSafeDynamicSegment } from './routeContract'

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
  | { name: 'settings'; section?: SettingsSection }
  | { name: 'stack'; stackId: string }
  | {
      name: 'service'
      stackId: string
      serviceId: string
      section?: 'overview' | 'versions' | 'history' | 'monitoring' | 'backup' | 'logs' | 'settings'
    }
  | { name: 'supervisor-misroute'; basePath: string; pathname: string }
  | { name: 'not-found'; pathname: string }

export type SettingsSection =
  | 'account'
  | 'maintenance'
  | 'backup'
  | 'monitoring'
  | 'schedules'
  | 'release-notes'
  | 'notifications'
  | 'integrations'

function normalizeSettingsSection(value: string | null | undefined): SettingsSection | null {
  if (
    value === 'account' ||
    value === 'maintenance' ||
    value === 'backup' ||
    value === 'monitoring' ||
    value === 'schedules' ||
    value === 'release-notes' ||
    value === 'notifications' ||
    value === 'integrations'
  ) {
    return value
  }
  return null
}

function normalizeServiceSection(
  value: string | null | undefined,
): 'overview' | 'versions' | 'history' | 'monitoring' | 'backup' | 'logs' | 'settings' | null {
  const section = (value ?? '').trim()
  if (section === '' || section === 'overview') return 'overview'
  if (section === 'versions' || section === 'history' || section === 'monitoring' || section === 'backup' || section === 'logs' || section === 'settings') return section
  return null
}

export function parseRoute(pathname: string): Route {
  const sup = parseSupervisorMisroute(pathname)
  if (sup) return sup

  let parts: string[]
  try {
    parts = stripAppBase(pathname).split('/').filter(Boolean).map(decodeURIComponent)
  } catch {
    return { name: 'not-found', pathname }
  }
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
  if (parts.length === 2 && parts[0] === 'queue' && isSafeDynamicSegment(parts[1])) return { name: 'job', jobId: parts[1] }
  if (parts.length === 1 && parts[0] === 'services') return { name: 'services' }
  if (parts.length === 1 && parts[0] === 'cleanup') return { name: 'cleanup' }
  // Legacy compatibility: keep old path readable after route migration.
  if (parts.length === 1 && parts[0] === 'version-inference') return { name: 'version-inference' }
  if (parts.length === 1 && parts[0] === 'deploy-check') return { name: 'deploy-check' }
  if (parts.length === 1 && parts[0] === 'settings') return { name: 'settings' }
  if (parts.length === 2 && parts[0] === 'settings') {
    const section = normalizeSettingsSection(parts[1])
    if (section) return { name: 'settings', section }
  }
  if (parts.length === 2 && parts[0] === 'services' && isSafeDynamicSegment(parts[1])) {
    return { name: 'stack', stackId: parts[1] }
  }
  if (parts.length === 3 && parts[0] === 'services' && isSafeDynamicSegment(parts[1]) && isSafeDynamicSegment(parts[2])) {
    return { name: 'service', stackId: parts[1], serviceId: parts[2], section: 'overview' }
  }
  if (parts.length === 4 && parts[0] === 'services') {
    const section = normalizeServiceSection(parts[3])
    if (section && isSafeDynamicSegment(parts[1]) && isSafeDynamicSegment(parts[2])) {
      return { name: 'service', stackId: parts[1], serviceId: parts[2], section }
    }
  }
  return { name: 'not-found', pathname }
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
        return route.section ? `/settings/${route.section}` : '/settings'
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
      case 'not-found':
        return route.pathname
    }
  })()

  if (route.name === 'supervisor-misroute' || route.name === 'not-found') return routePath
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

function currentPathname(): string { return stripAppBase(window.location.pathname) }

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

export function currentRoutePathname(): string {
  return currentPathname()
}

export function currentHref(route: Route): string {
  return href(route)
}
