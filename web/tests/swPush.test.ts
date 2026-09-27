import { describe, expect, test } from 'bun:test'
import {
  PUSH_ACK,
  PUSH_BADGE_FETCH_TIMEOUT_MS,
  PUSH_BADGE_TIMEOUT_MS,
  PUSH_MESSAGE,
  isNotificationClickAcknowledged,
  isPushBadgeAcknowledged,
  notificationLaunchUrl,
  resolveNotificationTargetUrl,
  validPushUnreadCount,
  waitForServiceWorkerAck,
} from '../src/swPush'

describe('service worker push contract', () => {
  test('keeps push badge counts absolute and fails closed for invalid payloads', () => {
    expect(validPushUnreadCount({ unreadCount: 3 })).toBe(3)
    expect(validPushUnreadCount({ unreadCount: 3.8 })).toBeNull()
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
    expect(PUSH_BADGE_FETCH_TIMEOUT_MS).toBeGreaterThan(0)
    expect(PUSH_BADGE_FETCH_TIMEOUT_MS).toBeLessThan(PUSH_BADGE_TIMEOUT_MS)
  })

  test('waits for an explicit service worker acknowledgement and fails on timeout', async () => {
    await expect(
      waitForServiceWorkerAck(
        (receive) => receive({ type: PUSH_ACK, ok: true }),
        isPushBadgeAcknowledged,
        10,
      ),
    ).resolves.toBe(true)
    await expect(
      waitForServiceWorkerAck(() => {}, isPushBadgeAcknowledged, 1),
    ).resolves.toBe(false)
    expect(isNotificationClickAcknowledged({ type: 'DOCKREV_NOTIFICATION_CLICK_ACK', ok: true })).toBe(true)
    expect(isNotificationClickAcknowledged({ type: PUSH_ACK, ok: true })).toBe(false)
  })

  test('encodes and resolves cold-start notification navigation targets', () => {
    const launch = notificationLaunchUrl(
      'https://dockrev.example/app/',
      '/app',
      'https://dockrev.example/app/queue/job-1',
      'notification-1',
    )
    expect(launch).toContain('/app?dockrevNotificationId=notification-1')
    expect(launch).toContain(
      'dockrevNotificationTarget=https%3A%2F%2Fdockrev.example%2Fapp%2Fqueue%2Fjob-1',
    )
    expect(
      resolveNotificationTargetUrl(
        'https://dockrev.example/app/',
        '/app',
        'https://dockrev.example/queue/job-1',
      ),
    ).toBe('https://dockrev.example/app/queue/job-1')
  })
})
