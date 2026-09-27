import { describe, expect, test } from 'bun:test'
import {
  PUSH_ACK,
  PUSH_BADGE_TIMEOUT_MS,
  PUSH_MESSAGE,
  isPushBadgeAcknowledged,
  validPushUnreadCount,
} from '../src/swPush'

describe('service worker push contract', () => {
  test('keeps push badge counts absolute and fails closed for invalid payloads', () => {
    expect(validPushUnreadCount({ unreadCount: 3.8 })).toBe(3)
    expect(validPushUnreadCount({ unreadCount: 0 })).toBe(0)
    expect(validPushUnreadCount({ unreadCount: -1 })).toBeNull()
    expect(validPushUnreadCount({ unreadCount: Number.NaN })).toBeNull()
    expect(validPushUnreadCount({})).toBeNull()
  })

  test('uses an explicit page claim before the worker falls back to REST', () => {
    expect(PUSH_MESSAGE).toBe('DOCKREV_NOTIFICATION_PUSH')
    expect(PUSH_ACK).toBe('DOCKREV_NOTIFICATION_PUSH_ACK')
    expect(isPushBadgeAcknowledged({ type: PUSH_ACK, ok: true })).toBe(true)
    expect(isPushBadgeAcknowledged({ type: PUSH_ACK, ok: false })).toBe(false)
    expect(isPushBadgeAcknowledged({ type: 'other', ok: true })).toBe(false)
    expect(PUSH_BADGE_TIMEOUT_MS).toBeGreaterThan(0)
  })
})
