export const SETTINGS_GHCR_WEBHOOK_ID = 'settings-ghcr-webhook'

type SettingsFocusTarget = 'ghcr-webhook'

const SETTINGS_FOCUS_STORAGE_KEY = 'dockrev:settings:focus-target'

function storage(): Storage | null {
  if (typeof window === 'undefined') return null
  try {
    return window.sessionStorage
  } catch {
    return null
  }
}

export function requestSettingsFocus(target: SettingsFocusTarget) {
  storage()?.setItem(SETTINGS_FOCUS_STORAGE_KEY, target)
}

export function peekRequestedSettingsFocus(): SettingsFocusTarget | null {
  const value = storage()?.getItem(SETTINGS_FOCUS_STORAGE_KEY)
  return value === 'ghcr-webhook' ? value : null
}

export function clearRequestedSettingsFocus() {
  storage()?.removeItem(SETTINGS_FOCUS_STORAGE_KEY)
}
