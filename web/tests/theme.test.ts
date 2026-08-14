import { afterEach, describe, expect, test } from 'bun:test'

import {
  cycleThemePreference,
  getThemePreference,
  initTheme,
  setThemePreference,
  subscribeTheme,
  THEME_STORAGE_KEY,
  type DockrevTheme,
} from '../src/theme'

type FakeStorage = Storage & { values: Map<string, string> }

function createStorage(): FakeStorage {
  const values = new Map<string, string>()
  return {
    values,
    get length() {
      return values.size
    },
    clear() {
      values.clear()
    },
    getItem(key) {
      return values.get(key) ?? null
    },
    key(index) {
      return [...values.keys()][index] ?? null
    },
    removeItem(key) {
      values.delete(key)
    },
    setItem(key, value) {
      values.set(key, String(value))
    },
  }
}

function installFakeThemeEnvironment(systemTheme: DockrevTheme = 'light') {
  const eventTarget = new EventTarget()
  const storage = createStorage()
  const meta = { content: '' }
  const mediaQuery = {
    matches: systemTheme === 'dark',
    addEventListener(type: string, listener: EventListener) {
      eventTarget.addEventListener(`media:${type}`, listener)
    },
    addListener(listener: EventListener) {
      eventTarget.addEventListener('media:change', listener)
    },
    removeEventListener(type: string, listener: EventListener) {
      eventTarget.removeEventListener(`media:${type}`, listener)
    },
    removeListener(listener: EventListener) {
      eventTarget.removeEventListener('media:change', listener)
    },
  }
  const root = {
    dataset: {} as DOMStringMap,
    style: { colorScheme: '' },
    classList: {
      toggle() {
        return false
      },
    },
  }
  const fakeWindow = {
    localStorage: storage,
    matchMedia: () => mediaQuery,
    addEventListener(type: string, listener: EventListener) {
      eventTarget.addEventListener(type, listener)
    },
    removeEventListener(type: string, listener: EventListener) {
      eventTarget.removeEventListener(type, listener)
    },
    dispatchEvent(event: Event) {
      return eventTarget.dispatchEvent(event)
    },
  }
  const fakeDocument = {
    documentElement: root,
    querySelector: () => meta,
  }

  ;(globalThis as unknown as { window: typeof fakeWindow }).window = fakeWindow
  ;(globalThis as unknown as { document: typeof fakeDocument }).document = fakeDocument

  return {
    storage,
    mediaQuery,
    root,
    meta,
    emitMediaChange() {
      eventTarget.dispatchEvent(new Event('media:change'))
    },
    emitStorageChange() {
      const event = Object.assign(new Event('storage'), { key: THEME_STORAGE_KEY })
      eventTarget.dispatchEvent(event)
    },
  }
}

afterEach(() => {
  delete (globalThis as unknown as { window?: unknown }).window
  delete (globalThis as unknown as { document?: unknown }).document
})

describe('theme preference contract', () => {
  test('cycles in the opposite-first order for the current system theme', () => {
    expect(cycleThemePreference('system', 'dark')).toBe('light')
    expect(cycleThemePreference('light', 'dark')).toBe('dark')
    expect(cycleThemePreference('dark', 'dark')).toBe('system')
    expect(cycleThemePreference('system', 'light')).toBe('dark')
    expect(cycleThemePreference('dark', 'light')).toBe('light')
    expect(cycleThemePreference('light', 'light')).toBe('system')
  })

  test('treats missing and invalid storage as system and applies browser metadata', () => {
    const env = installFakeThemeEnvironment('dark')
    env.storage.setItem(THEME_STORAGE_KEY, 'sepia')

    initTheme()

    expect(getThemePreference()).toBe('system')
    expect(env.root.dataset.theme).toBe('dark')
    expect(env.root.style.colorScheme).toBe('dark')
    expect(env.meta.content).toBe('#061227')
  })

  test('keeps explicit preferences independent from OS changes', () => {
    const env = installFakeThemeEnvironment('light')
    initTheme()
    setThemePreference('dark')
    env.mediaQuery.matches = false
    env.emitMediaChange()

    expect(getThemePreference()).toBe('dark')
    expect(env.root.dataset.theme).toBe('dark')
  })

  test('removes the storage key when returning to system and follows OS changes', () => {
    const env = installFakeThemeEnvironment('light')
    initTheme()
    setThemePreference('dark')
    setThemePreference('system')

    expect(env.storage.getItem(THEME_STORAGE_KEY)).toBeNull()
    expect(env.root.dataset.theme).toBe('light')

    env.mediaQuery.matches = true
    env.emitMediaChange()
    expect(env.root.dataset.theme).toBe('dark')
  })

  test('syncs same-origin storage changes to the current tab', () => {
    const env = installFakeThemeEnvironment('light')
    initTheme()
    env.storage.setItem(THEME_STORAGE_KEY, 'dark')
    env.emitStorageChange()
    expect(getThemePreference()).toBe('dark')
    expect(env.root.dataset.theme).toBe('dark')
  })

  test('notifies same-page subscribers without changing business state', () => {
    installFakeThemeEnvironment('light')
    initTheme()
    let updates = 0
    const unsubscribe = subscribeTheme(() => {
      updates += 1
    })
    setThemePreference('dark')
    unsubscribe()

    expect(updates).toBe(1)
  })
})
