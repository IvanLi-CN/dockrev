export type DockrevTheme = 'dark' | 'light'

const STORAGE_KEY = 'dockrev:theme'

function preferredTheme(): DockrevTheme {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

function normalizeTheme(value: string | null): DockrevTheme | null {
  if (value === 'dark') return 'dark'
  if (value === 'light') return 'light'
  return null
}

function applyTheme(theme: DockrevTheme) {
  const root = document.documentElement
  root.dataset.theme = theme
  root.style.colorScheme = theme
  root.classList.toggle('dark', theme === 'dark')
}

export function initTheme() {
  const saved = normalizeTheme(window.localStorage.getItem(STORAGE_KEY))
  const theme = saved ?? preferredTheme()
  applyTheme(theme)
}

export function getTheme(): DockrevTheme {
  const t = document.documentElement.dataset.theme
  if (t === 'light') return 'light'
  return 'dark'
}

export function setTheme(theme: DockrevTheme) {
  applyTheme(theme)
  window.localStorage.setItem(STORAGE_KEY, theme)
}
