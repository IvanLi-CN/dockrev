import { describe, expect, test } from 'bun:test'
import { normalizeNotificationBadgeCount } from '../src/notificationBadge'

describe('notification badge count', () => {
  test('normalizes only the server absolute value', () => {
    expect(normalizeNotificationBadgeCount(3.8)).toBe(3)
    expect(normalizeNotificationBadgeCount(0)).toBe(0)
    expect(normalizeNotificationBadgeCount(-2)).toBe(0)
    expect(normalizeNotificationBadgeCount(Number.NaN)).toBe(0)
  })
})
