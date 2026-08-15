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
let activeThemeTransition = false
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

function copyScrollPositions(source: Element, clone: Element) {
  const sourceElements = [source, ...source.querySelectorAll('*')]
  const cloneElements = [clone, ...clone.querySelectorAll('*')]
  sourceElements.forEach((element, index) => {
    const cloneElement = cloneElements[index]
    if (!cloneElement) return
    cloneElement.scrollTop = element.scrollTop
    cloneElement.scrollLeft = element.scrollLeft
  })
}

function buildThemeTransitionLayer(nextTheme: DockrevTheme) {
  if (typeof document.getElementById !== 'function') return null
  const source = document.getElementById('root')
  if (!source) return null
  const layer = document.createElement('div')
  layer.className = 'themeTransitionLayer'
  layer.dataset.theme = nextTheme
  layer.style.colorScheme = nextTheme
  const clone = source.cloneNode(true) as HTMLElement
  clone.removeAttribute('id')
  clone.classList.add('themeTransitionSurface')
  clone.dataset.theme = nextTheme
  clone.querySelectorAll('[id]').forEach((element) => element.removeAttribute('id'))
  clone.setAttribute('aria-hidden', 'true')
  clone.setAttribute('inert', '')
  layer.append(clone)
  document.body.append(layer)
  copyScrollPositions(source, clone)
  return layer
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
  const layer = buildThemeTransitionLayer(nextTheme)
  if (!layer || typeof layer.animate !== 'function') {
    layer?.remove()
    applyThemePreference(preference)
    return
  }
  const { x, y, radius } = getThemeTransitionGeometry(transitionOrigin)
  activeThemeTransition = true
  const animation = layer.animate([
    { clipPath: `circle(0px at ${x}px ${y}px)` },
    { clipPath: `circle(${radius}px at ${x}px ${y}px)` },
  ], {
    duration: 1200,
    easing: 'cubic-bezier(0.4, 0, 0.2, 1)',
    fill: 'forwards',
  })
  animation.finished.then(
    () => applyThemePreference(preference),
    () => undefined,
  ).finally(() => {
    layer.remove()
    activeThemeTransition = false
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
