import { describe, expect, test } from 'bun:test'
import { normalizeNotificationBadgeCount, setNotificationBadge } from '../src/notificationBadge'

describe('notification badge count', () => {
  test('normalizes only the server absolute value', () => {
    expect(normalizeNotificationBadgeCount(3.8)).toBe(3)
    expect(normalizeNotificationBadgeCount(0)).toBe(0)
    expect(normalizeNotificationBadgeCount(-2)).toBe(0)
    expect(normalizeNotificationBadgeCount(Number.NaN)).toBe(0)
  })

  test('clears the platform badge when the authoritative count is zero', async () => {
    const originalNavigator = globalThis.navigator
    const calls: string[] = []
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      value: {
        clearAppBadge: () => {
          calls.push('clear')
        },
        setAppBadge: () => {
          calls.push('set')
        },
      },
    })
    try {
      setNotificationBadge(0)
      await Promise.resolve()
      expect(calls).toEqual(['clear'])
    } finally {
      Object.defineProperty(globalThis, 'navigator', {
        configurable: true,
        value: originalNavigator,
      })
    }
  })

  test('sets the platform badge from the authoritative count', async () => {
    const originalNavigator = globalThis.navigator
    const counts: number[] = []
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      value: {
        setAppBadge: (count?: number) => {
          if (count != null) counts.push(count)
        },
      },
    })
    try {
      setNotificationBadge(3.8)
      await Promise.resolve()
      expect(counts).toEqual([3])
    } finally {
      Object.defineProperty(globalThis, 'navigator', {
        configurable: true,
        value: originalNavigator,
      })
    }
  })
})
