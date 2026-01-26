export type DockrevRuntimeConfig = {
  selfUpgradeUrl?: string
  dockrevImageRepo?: string
}

declare global {
  interface Window {
    __DOCKREV_CONFIG__?: DockrevRuntimeConfig
  }
}

function normalizeBaseUrl(input: string): string {
  const trimmed = input.trim()
  if (!trimmed) return '/'
  if (trimmed.endsWith('/')) return trimmed
  return `${trimmed}/`
}

export function dockrevImageRepo(): string {
  return window.__DOCKREV_CONFIG__?.dockrevImageRepo ?? 'ghcr.io/ivanli-cn/dockrev'
}

export function isDockrevImageRef(imageRef: string): boolean {
  const repo = dockrevImageRepo().trim()
  if (!repo) return false
  return imageRef === repo || imageRef.startsWith(`${repo}:`) || imageRef.startsWith(`${repo}@`)
}

export function selfUpgradeBaseUrl(): string {
  const v = window.__DOCKREV_CONFIG__?.selfUpgradeUrl ?? '/supervisor/'
  return normalizeBaseUrl(v)
}
