function normalizePathname(pathname: string): string {
  let normalized = pathname.trim()
  if (!normalized) normalized = '/'
  if (!normalized.startsWith('/')) normalized = `/${normalized}`
  return normalized.replace(/\/{2,}/g, '/')
}

export function normalizeAppBasePath(basePath: string | null | undefined): string {
  const rawBasePath = (basePath ?? '/').trim()
  let normalized = '/'
  try {
    normalized = normalizePathname(new URL(rawBasePath || '/', 'https://dockrev.local/').pathname)
  } catch {
    normalized = normalizePathname(rawBasePath || '/')
  }
  return normalized === '/' ? '/' : `${normalized.replace(/\/+$/, '')}/`
}

function trimTrailingSlash(pathname: string): string {
  return pathname.replace(/\/+$/, '') || '/'
}

export function stripAppBaseFromPath(basePath: string, pathname: string): string {
  const normalizedBasePath = normalizeAppBasePath(basePath)
  const normalizedPathname = normalizePathname(pathname)

  if (normalizedBasePath === '/') return normalizedPathname

  const baseWithoutTrailingSlash = trimTrailingSlash(normalizedBasePath)
  if (normalizedPathname === baseWithoutTrailingSlash) return '/'
  if (normalizedPathname === normalizedBasePath) return '/'
  if (normalizedPathname.startsWith(normalizedBasePath)) {
    return `/${normalizedPathname.slice(normalizedBasePath.length)}`.replace(/\/{2,}/g, '/')
  }
  return normalizedPathname
}

export function withAppBasePath(basePath: string, routePath: string): string {
  const normalizedBasePath = normalizeAppBasePath(basePath)
  const normalizedRoutePath = normalizePathname(routePath)
  if (normalizedBasePath === '/') return normalizedRoutePath

  const baseWithoutTrailingSlash = trimTrailingSlash(normalizedBasePath)
  if (normalizedRoutePath === '/') return normalizedBasePath
  return `${baseWithoutTrailingSlash}${normalizedRoutePath}`
}

export function isAppBaseEntryPath(basePath: string, pathname: string): boolean {
  const normalizedBasePath = normalizeAppBasePath(basePath)
  const normalizedPathname = normalizePathname(pathname)
  const baseWithoutTrailingSlash = trimTrailingSlash(normalizedBasePath)
  return (
    normalizedPathname === normalizedBasePath ||
    normalizedPathname === baseWithoutTrailingSlash ||
    normalizedPathname === `${baseWithoutTrailingSlash}/index.html`
  )
}

function runtimeBaseUrl(): string {
  return import.meta.env.BASE_URL ?? '/'
}

export function appBasePath(): string {
  return normalizeAppBasePath(runtimeBaseUrl())
}

export function stripAppBase(pathname: string): string {
  return stripAppBaseFromPath(appBasePath(), pathname)
}

export function withAppBase(routePath: string): string {
  return withAppBasePath(appBasePath(), routePath)
}

export function isCurrentAppBaseEntry(pathname: string): boolean {
  return isAppBaseEntryPath(appBasePath(), pathname)
}
