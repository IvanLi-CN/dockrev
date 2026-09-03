import { appBasePath, isAppBaseEntryPath, normalizeAppBasePath } from '../appBase'

export const PAGES_DEMO_RESTORE_STORAGE_KEY = 'dockrev:pages-demo:restore'
const PAGES_DEMO_RESTORE_TTL_MS = 5 * 60 * 1000

export type PagesDemoRestoreEntry = {
  path: string
  savedAt: number
}

export function parsePagesDemoRestoreEntry(input: string | null): PagesDemoRestoreEntry | null {
  if (!input) return null
  try {
    const parsed = JSON.parse(input) as Partial<PagesDemoRestoreEntry>
    if (typeof parsed.path !== 'string' || !parsed.path.startsWith('/')) return null
    if (typeof parsed.savedAt !== 'number' || !Number.isFinite(parsed.savedAt)) return null
    return {
      path: parsed.path,
      savedAt: parsed.savedAt,
    }
  } catch {
    return null
  }
}

export function shouldRestorePagesDemoPath(input: {
  currentBasePath: string
  currentPathname: string
  pendingEntry: PagesDemoRestoreEntry | null
  now: number
}): boolean {
  if (!input.pendingEntry) return false
  const normalizedBasePath = normalizeAppBasePath(input.currentBasePath)
  if (!isAppBaseEntryPath(normalizedBasePath, input.currentPathname)) return false
  if (input.now - input.pendingEntry.savedAt > PAGES_DEMO_RESTORE_TTL_MS) return false
  return input.pendingEntry.path.startsWith(normalizedBasePath)
}

export function canonicalPagesDemoEntryPath(basePath: string, pathname: string): string | null {
  const normalizedBasePath = normalizeAppBasePath(basePath)
  if (!isAppBaseEntryPath(normalizedBasePath, pathname)) return null
  return pathname === normalizedBasePath ? null : normalizedBasePath
}

export function restorePendingPagesDemoPath() {
  if (typeof window === 'undefined') return false

  const currentBasePath = appBasePath()
  const currentPathname = window.location.pathname
  const rawEntry = window.sessionStorage.getItem(PAGES_DEMO_RESTORE_STORAGE_KEY)
  const pendingEntry = parsePagesDemoRestoreEntry(rawEntry)
  if (
    !shouldRestorePagesDemoPath({
      currentBasePath,
      currentPathname,
      pendingEntry,
      now: Date.now(),
    })
  ) {
    if (rawEntry && !pendingEntry) {
      window.sessionStorage.removeItem(PAGES_DEMO_RESTORE_STORAGE_KEY)
    }
    const canonicalPath = canonicalPagesDemoEntryPath(currentBasePath, currentPathname)
    if (!canonicalPath) return false
    window.history.replaceState({}, '', `${canonicalPath}${window.location.search}`)
    return true
  }
  if (!pendingEntry) return false

  window.sessionStorage.removeItem(PAGES_DEMO_RESTORE_STORAGE_KEY)
  const nextUrl = new URL(pendingEntry.path, window.location.origin)
  const next = `${nextUrl.pathname}${nextUrl.search}`
  const current = `${window.location.pathname}${window.location.search}`
  if (next === current) return false
  window.history.replaceState({}, '', next)
  return true
}
