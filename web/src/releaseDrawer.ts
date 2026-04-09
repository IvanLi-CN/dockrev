export type GitHubReleaseDrawerState = {
  open: boolean
  source: 'github' | null
  serviceId: string | null
  version: string | null
}

export const CLOSED_GITHUB_RELEASE_DRAWER_STATE: GitHubReleaseDrawerState = {
  open: false,
  source: null,
  serviceId: null,
  version: null,
}

export const RELEASE_DRAWER_LOCATION_EVENT = 'dockrev:release-drawer-location'

const RELEASE_DRAWER_KEY = 'releaseDrawer'
const RELEASE_DRAWER_SERVICE_ID_KEY = 'releaseServiceId'
const RELEASE_DRAWER_VERSION_KEY = 'releaseVersion'

export const RELEASE_DRAWER_QUERY_KEYS = [
  RELEASE_DRAWER_KEY,
  RELEASE_DRAWER_SERVICE_ID_KEY,
  RELEASE_DRAWER_VERSION_KEY,
] as const

function clean(value: string | null | undefined): string | null {
  const trimmed = (value ?? '').trim()
  return trimmed ? trimmed : null
}

function dispatchReleaseDrawerLocationChange() {
  if (typeof window === 'undefined') return
  window.dispatchEvent(new CustomEvent(RELEASE_DRAWER_LOCATION_EVENT))
}

function nextUrlWithReleaseDrawerState(nextState: GitHubReleaseDrawerState): URL | null {
  if (typeof window === 'undefined') return null
  try {
    const url = new URL(window.location.href)
    if (nextState.open && nextState.source === 'github' && nextState.serviceId) {
      url.searchParams.set(RELEASE_DRAWER_KEY, 'github')
      url.searchParams.set(RELEASE_DRAWER_SERVICE_ID_KEY, nextState.serviceId)
      if (nextState.version) url.searchParams.set(RELEASE_DRAWER_VERSION_KEY, nextState.version)
      else url.searchParams.delete(RELEASE_DRAWER_VERSION_KEY)
    } else {
      url.searchParams.delete(RELEASE_DRAWER_KEY)
      url.searchParams.delete(RELEASE_DRAWER_SERVICE_ID_KEY)
      url.searchParams.delete(RELEASE_DRAWER_VERSION_KEY)
    }
    return url
  } catch {
    return null
  }
}

export function readGitHubReleaseDrawerState(): GitHubReleaseDrawerState {
  if (typeof window === 'undefined') {
    return CLOSED_GITHUB_RELEASE_DRAWER_STATE
  }
  try {
    const url = new URL(window.location.href)
    const source = clean(url.searchParams.get(RELEASE_DRAWER_KEY))
    const serviceId = clean(url.searchParams.get(RELEASE_DRAWER_SERVICE_ID_KEY))
    const version = clean(url.searchParams.get(RELEASE_DRAWER_VERSION_KEY))
    if (source !== 'github' || !serviceId) {
      return CLOSED_GITHUB_RELEASE_DRAWER_STATE
    }
    return { open: true, source: 'github', serviceId, version }
  } catch {
    return CLOSED_GITHUB_RELEASE_DRAWER_STATE
  }
}

export function shouldResetReleaseDrawerOnRouteChange(input: {
  drawerOpen: boolean
  hashRouting: boolean
  previousPathname: string | null
  nextPathname: string
}): boolean {
  return (
    input.drawerOpen &&
    input.hashRouting &&
    input.previousPathname != null &&
    input.previousPathname !== input.nextPathname
  )
}

export function openGitHubReleaseDrawer(
  input: { serviceId: string; version?: string | null },
  mode: 'push' | 'replace' = 'push',
) {
  const nextUrl = nextUrlWithReleaseDrawerState({
    open: true,
    source: 'github',
    serviceId: input.serviceId.trim(),
    version: clean(input.version),
  })
  if (!nextUrl) return
  const next = `${nextUrl.pathname}${nextUrl.search}${nextUrl.hash}`
  if (mode === 'replace') window.history.replaceState({}, '', next)
  else window.history.pushState({}, '', next)
  dispatchReleaseDrawerLocationChange()
}

export function closeGitHubReleaseDrawer(mode: 'push' | 'replace' = 'push') {
  const nextUrl = nextUrlWithReleaseDrawerState({
    open: false,
    source: null,
    serviceId: null,
    version: null,
  })
  if (!nextUrl) return
  const next = `${nextUrl.pathname}${nextUrl.search}${nextUrl.hash}`
  if (mode === 'replace') window.history.replaceState({}, '', next)
  else window.history.pushState({}, '', next)
  dispatchReleaseDrawerLocationChange()
}
