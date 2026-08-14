export type ThemePreference = 'system' | 'dark' | 'light'
export type DockrevTheme = Exclude<ThemePreference, 'system'>

export const THEME_STORAGE_KEY = 'dockrev:theme'
const SYSTEM_THEME_MEDIA_QUERY = '(prefers-color-scheme: dark)'
const THEME_COLORS: Record<DockrevTheme, string> = {
  dark: '#061227',
  light: '#f6faff',
}

type ThemeListener = () => void

let mediaQuery: MediaQueryList | null = null
let initialized = false
let boundWindow: Window | null = null
const listeners = new Set<ThemeListener>()

function normalizeThemePreference(value: string | null): ThemePreference {
  if (value === 'dark' || value === 'light') return value
  return 'system'
}

function readStoredPreference(): ThemePreference {
  if (typeof window === 'undefined') return 'system'
  try {
    return normalizeThemePreference(window.localStorage.getItem(THEME_STORAGE_KEY))
  } catch {
    return 'system'
  }
}

export function getSystemTheme(): DockrevTheme {
  if (typeof window === 'undefined') return 'light'
  return window.matchMedia(SYSTEM_THEME_MEDIA_QUERY).matches ? 'dark' : 'light'
}

export function getThemePreference(): ThemePreference {
  return readStoredPreference()
}

export function resolveTheme(preference: ThemePreference = getThemePreference()): DockrevTheme {
  return preference === 'system' ? getSystemTheme() : preference
}

function notify() {
  listeners.forEach((listener) => listener())
}

function setThemeColor(theme: DockrevTheme) {
  if (typeof document === 'undefined') return
  const meta = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]')
  if (meta) meta.content = THEME_COLORS[theme]
}

function applyTheme(theme: DockrevTheme) {
  if (typeof document === 'undefined') return
  const root = document.documentElement
  root.dataset.theme = theme
  root.style.colorScheme = theme
  root.classList.toggle('dark', theme === 'dark')
  setThemeColor(theme)
}

function syncThemeFromEnvironment() {
  applyTheme(resolveTheme())
  notify()
}

function installThemeListeners() {
  if (typeof window === 'undefined' || (initialized && boundWindow === window)) return
  initialized = true
  boundWindow = window
  mediaQuery = window.matchMedia(SYSTEM_THEME_MEDIA_QUERY)
  const onSystemThemeChange = () => {
    if (getThemePreference() === 'system') syncThemeFromEnvironment()
  }
  if (typeof mediaQuery.addEventListener === 'function') {
    mediaQuery.addEventListener('change', onSystemThemeChange)
  } else {
    mediaQuery.addListener(onSystemThemeChange)
  }
  window.addEventListener('storage', (event) => {
    if (event.key !== THEME_STORAGE_KEY) return
    syncThemeFromEnvironment()
  })
}

export function initTheme() {
  installThemeListeners()
  applyTheme(resolveTheme())
}

export function getTheme(): DockrevTheme {
  if (typeof document === 'undefined') return resolveTheme()
  const theme = document.documentElement.dataset.theme
  return theme === 'light' ? 'light' : 'dark'
}

export function setThemePreference(preference: ThemePreference) {
  if (typeof window !== 'undefined') {
    try {
      if (preference === 'system') {
        window.localStorage.removeItem(THEME_STORAGE_KEY)
      } else {
        window.localStorage.setItem(THEME_STORAGE_KEY, preference)
      }
    } catch {
      // A storage failure should not prevent the current tab from changing theme.
    }
  }
  applyTheme(resolveTheme(preference))
  notify()
}

export function setTheme(theme: DockrevTheme) {
  setThemePreference(theme)
}

export function subscribeTheme(listener: ThemeListener) {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function cycleThemePreference(
  preference: ThemePreference,
  systemTheme: DockrevTheme,
): ThemePreference {
  const matchingTheme = systemTheme
  const oppositeTheme: DockrevTheme = systemTheme === 'dark' ? 'light' : 'dark'
  const order: ThemePreference[] = ['system', oppositeTheme, matchingTheme]
  return order[(order.indexOf(preference) + 1) % order.length]
}
