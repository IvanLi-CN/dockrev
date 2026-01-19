export type DockrevTheme = 'dockrev-dark' | 'dockrev-light'

const STORAGE_KEY = 'dockrev.theme'

function preferredTheme(): DockrevTheme {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dockrev-dark' : 'dockrev-light'
}

export function initTheme() {
  const saved = window.localStorage.getItem(STORAGE_KEY) as DockrevTheme | null
  const theme = saved ?? preferredTheme()
  document.documentElement.dataset.theme = theme
}

export function getTheme(): DockrevTheme {
  const t = document.documentElement.dataset.theme
  if (t === 'dockrev-light') return 'dockrev-light'
  return 'dockrev-dark'
}

export function setTheme(theme: DockrevTheme) {
  document.documentElement.dataset.theme = theme
  window.localStorage.setItem(STORAGE_KEY, theme)
}

