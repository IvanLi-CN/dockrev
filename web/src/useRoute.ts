import { useMemo, useSyncExternalStore } from 'react'
import type { Route } from './routes'
import { currentRoutePathname, installPopStateListener, parseRoute, subscribeNavigation } from './routes'

let popStateInstalled = false

function ensureNavigationListenerInstalled() {
  if (popStateInstalled) return
  installPopStateListener()
  popStateInstalled = true
}

function subscribe(listener: () => void) {
  ensureNavigationListenerInstalled()
  return subscribeNavigation(listener)
}

function getPathSnapshot(): string {
  return currentRoutePathname()
}

export function useRoute(): Route {
  const pathname = useSyncExternalStore(subscribe, getPathSnapshot, getPathSnapshot)
  return useMemo(() => parseRoute(pathname), [pathname])
}
