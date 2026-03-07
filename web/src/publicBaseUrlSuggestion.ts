function normalizePathname(pathname: string): string {
  let normalizedPath = pathname.trim()
  if (!normalizedPath) normalizedPath = '/'
  if (!normalizedPath.startsWith('/')) normalizedPath = `/${normalizedPath}`
  return normalizedPath.replace(/\/+$/, '') || '/'
}

function deriveBasePathFromPagePathname(pagePathname: string): { basePath: string; matchesSettingsRoute: boolean } | null {
  const normalizedPagePath = normalizePathname(pagePathname)
  const matchesSettingsRoute = normalizedPagePath === '/settings' || normalizedPagePath.endsWith('/settings')

  if (normalizedPagePath === '/') {
    return { basePath: '/', matchesSettingsRoute }
  }

  const lastSlash = normalizedPagePath.lastIndexOf('/')
  const lastSegment = normalizedPagePath.slice(lastSlash + 1)
  const basePath =
    lastSegment === 'index.html' || lastSegment === 'iframe.html'
      ? normalizedPagePath.slice(0, lastSlash) || '/'
      : normalizedPagePath

  return { basePath, matchesSettingsRoute }
}

export function derivePublicBaseUrlSuggestion(routePathname: string, origin: string, pagePathname = ''): string | null {
  const trimmedOrigin = origin.trim()
  if (!trimmedOrigin || trimmedOrigin === 'null') return null

  const normalizedRoutePath = normalizePathname(routePathname)

  const settingsSuffix = '/settings'
  let basePath = '/'
  if (normalizedRoutePath === settingsSuffix) {
    basePath = '/'
  } else if (normalizedRoutePath.endsWith(settingsSuffix)) {
    basePath = normalizedRoutePath.slice(0, -settingsSuffix.length) || '/'
  } else if (normalizedRoutePath !== '/') {
    basePath = normalizedRoutePath
  }

  const pageBasePath = deriveBasePathFromPagePathname(pagePathname)
  if (normalizedRoutePath === settingsSuffix && pageBasePath && pageBasePath.basePath !== '/' && !pageBasePath.matchesSettingsRoute) {
    basePath = pageBasePath.basePath
  }

  try {
    return new URL(basePath.endsWith('/') ? basePath : `${basePath}/`, trimmedOrigin).toString()
  } catch {
    return null
  }
}
