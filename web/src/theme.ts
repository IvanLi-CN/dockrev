export type ThemePreference = 'system' | 'dark' | 'light'
export type DockrevTheme = Exclude<ThemePreference, 'system'>

export const THEME_STORAGE_KEY = 'dockrev:theme'
const SYSTEM_THEME_MEDIA_QUERY = '(prefers-color-scheme: dark)'
const REDUCED_MOTION_MEDIA_QUERY = '(prefers-reduced-motion: reduce)'
const THEME_COLORS: Record<DockrevTheme, string> = {
  dark: '#061227',
  light: '#f6faff',
}

type ThemeListener = () => void

let mediaQuery: MediaQueryList | null = null
let initialized = false
let boundWindow: Window | null = null
let activeThemeTransition: ViewTransition | null = null
const listeners = new Set<ThemeListener>()

export type ThemeTransitionOrigin = {
  x: number
  y: number
}

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

function persistThemePreference(preference: ThemePreference) {
  if (typeof window === 'undefined') return
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

function applyThemePreference(preference: ThemePreference) {
  persistThemePreference(preference)
  applyTheme(resolveTheme(preference))
  notify()
}

function prefersReducedMotion() {
  return typeof window !== 'undefined'
    && window.matchMedia(REDUCED_MOTION_MEDIA_QUERY).matches
}

export function getThemeTransitionGeometry(origin: ThemeTransitionOrigin) {
  const width = document.documentElement.clientWidth
  const height = document.documentElement.clientHeight
  const x = Math.min(width, Math.max(0, origin.x))
  const y = Math.min(height, Math.max(0, origin.y))
  return {
    x,
    y,
    radius: Math.hypot(Math.max(x, width - x), Math.max(y, height - y)),
  }
}

export function getThemeTransitionClipPaths(origin: ThemeTransitionOrigin) {
  const { x, y, radius } = getThemeTransitionGeometry(origin)
  const width = document.documentElement.clientWidth
  const height = document.documentElement.clientHeight
  const radiusBasis = Math.hypot(width, height) / Math.SQRT2
  const xPercent = `${(x / width) * 100}%`
  const yPercent = `${(y / height) * 100}%`
  return {
    start: `circle(0% at ${xPercent} ${yPercent})`,
    end: `circle(${(radius / radiusBasis) * 100}% at ${xPercent} ${yPercent})`,
  }
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
    if (event.key !== null && event.key !== THEME_STORAGE_KEY) return
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

export function setThemePreference(
  preference: ThemePreference,
  origin?: ThemeTransitionOrigin,
) {
  if (activeThemeTransition) return
  const nextTheme = resolveTheme(preference)
  const currentTheme = getTheme()
  if (
    typeof document === 'undefined'
    || typeof document.startViewTransition !== 'function'
    || prefersReducedMotion()
    || nextTheme === currentTheme
  ) {
    applyThemePreference(preference)
    return
  }

  const transitionOrigin = origin ?? {
    x: window.innerWidth / 2,
    y: window.innerHeight / 2,
  }
  const clipPaths = getThemeTransitionClipPaths(transitionOrigin)
  const transition = document.startViewTransition(() => applyThemePreference(preference))
  activeThemeTransition = transition
  transition.ready.then(() => document.documentElement.animate([
    { clipPath: clipPaths.start },
    { clipPath: clipPaths.end },
  ], {
    duration: 1800,
    easing: 'cubic-bezier(0.45, 0, 0.55, 1)',
    fill: 'both',
    pseudoElement: '::view-transition-new(root)',
  }).finished).catch(() => undefined)
  transition.finished.finally(() => {
    if (activeThemeTransition === transition) activeThemeTransition = null
  })
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
