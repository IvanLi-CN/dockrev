export function derivePublicBaseUrlSuggestion(routePathname: string, origin: string): string | null {
  const trimmedOrigin = origin.trim()
  if (!trimmedOrigin || trimmedOrigin === 'null') return null

  let normalizedPath = routePathname.trim()
  if (!normalizedPath) normalizedPath = '/'
  if (!normalizedPath.startsWith('/')) normalizedPath = `/${normalizedPath}`
  normalizedPath = normalizedPath.replace(/\/+$/, '') || '/'

  const settingsSuffix = '/settings'
  let basePath = '/'
  if (normalizedPath === settingsSuffix) {
    basePath = '/'
  } else if (normalizedPath.endsWith(settingsSuffix)) {
    basePath = normalizedPath.slice(0, -settingsSuffix.length) || '/'
  } else if (normalizedPath !== '/') {
    basePath = normalizedPath
  }

  try {
    return new URL(basePath.endsWith('/') ? basePath : `${basePath}/`, trimmedOrigin).toString()
  } catch {
    return null
  }
}
