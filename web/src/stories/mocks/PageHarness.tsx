import { useEffect, useState, type ReactNode } from 'react'
import { AppShell } from '../../Shell'
import { GitHubReleaseDrawer } from '../../components/GitHubReleaseDrawer'
import {
  CLOSED_GITHUB_RELEASE_DRAWER_STATE,
  RELEASE_DRAWER_LOCATION_EVENT,
  RELEASE_DRAWER_QUERY_KEYS,
  closeGitHubReleaseDrawer,
  readGitHubReleaseDrawerState,
} from '../../releaseDrawer'
import type { Route } from '../../routes'
import type { TopbarAuthIdentity } from '../../topbarAuthIdentity'

export function PageHarness(props: {
  route: Route
  title: string
  pageSubtitle?: string
  topbarHint?: string
  authIdentity?: TopbarAuthIdentity | null
  children: (ctx: {
    onTopActions: (node: ReactNode) => void
    onLastScanHint: (lastScan?: string) => void
  }) => ReactNode
}) {
  const [topActions, setTopActions] = useState<ReactNode>(null)
  const [lastScanHint, setLastScanHint] = useState<string | undefined>(undefined)
  const [releaseDrawerState, setReleaseDrawerState] = useState(CLOSED_GITHUB_RELEASE_DRAWER_STATE)

  useEffect(() => {
    if (typeof window === 'undefined') return
    const url = new URL(window.location.href)
    let changed = false
    for (const key of RELEASE_DRAWER_QUERY_KEYS) {
      if (!url.searchParams.has(key)) continue
      url.searchParams.delete(key)
      changed = true
    }
    if (!changed) return
    window.history.replaceState({}, '', `${url.pathname}${url.search}${url.hash}`)
  }, [])

  useEffect(() => {
    if (typeof window === 'undefined') return
    const sync = () => setReleaseDrawerState(readGitHubReleaseDrawerState())
    const handleLocation = () => sync()
    sync()
    window.addEventListener('popstate', handleLocation)
    window.addEventListener('hashchange', handleLocation)
    window.addEventListener(RELEASE_DRAWER_LOCATION_EVENT, handleLocation as EventListener)
    return () => {
      window.removeEventListener('popstate', handleLocation)
      window.removeEventListener('hashchange', handleLocation)
      window.removeEventListener(RELEASE_DRAWER_LOCATION_EVENT, handleLocation as EventListener)
    }
  }, [])

  return (
    <>
      <AppShell
        route={props.route}
        title={props.title}
        pageSubtitle={props.pageSubtitle}
        topbarHint={props.topbarHint}
        topActions={topActions}
        authIdentity={props.authIdentity}
        lastScanHint={lastScanHint}
      >
        {props.children({
          onTopActions: setTopActions,
          onLastScanHint: setLastScanHint,
        })}
      </AppShell>
      <GitHubReleaseDrawer
        open={releaseDrawerState.open}
        serviceId={releaseDrawerState.serviceId}
        version={releaseDrawerState.version}
        onOpenChange={(open) => {
          if (open) return
          closeGitHubReleaseDrawer('replace')
        }}
      />
    </>
  )
}
