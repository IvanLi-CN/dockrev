import { useEffect, useState } from 'react'
import type { Route } from './routes'
import { currentRoutePathname, installPopStateListener, parseRoute, subscribeNavigation } from './routes'

export function useRoute(): Route {
  const [route, setRoute] = useState<Route>(() => parseRoute(currentRoutePathname()))

  useEffect(() => {
    installPopStateListener()
    return subscribeNavigation(() => {
      setRoute(parseRoute(currentRoutePathname()))
    })
  }, [])

  return route
}
