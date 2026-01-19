import { useEffect, useState } from 'react'
import type { Route } from './routes'
import { installPopStateListener, parseRoute, subscribeNavigation } from './routes'

export function useRoute(): Route {
  const [route, setRoute] = useState<Route>(() => parseRoute(window.location.pathname))

  useEffect(() => {
    installPopStateListener()
    return subscribeNavigation(() => {
      setRoute(parseRoute(window.location.pathname))
    })
  }, [])

  return route
}

