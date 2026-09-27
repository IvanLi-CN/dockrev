import { describe, expect, test } from 'bun:test'
import { refreshNotificationAfterServerChange } from '../src/notificationSync'

describe('notification sync recovery', () => {
  test('invalidates stale work and retries one authoritative sync', async () => {
    const calls: Array<[boolean, boolean]> = []
    let invalidations = 0
    let waits = 0

    const result = await refreshNotificationAfterServerChange(
      async (withItems, shouldBroadcast) => {
        calls.push([withItems, shouldBroadcast])
        return calls.length === 2
      },
      () => {
        invalidations += 1
      },
      true,
      false,
      async () => {
        waits += 1
      },
    )

    expect(result).toBe(true)
    expect(invalidations).toBe(1)
    expect(waits).toBe(1)
    expect(calls).toEqual([
      [true, false],
      [true, false],
    ])
  })

  test('bounds repeated failures and reports that the page is not synchronized', async () => {
    let attempts = 0
    let waits = 0

    const result = await refreshNotificationAfterServerChange(
      async () => {
        attempts += 1
        return false
      },
      () => {},
      false,
      true,
      async () => {
        waits += 1
      },
    )

    expect(result).toBe(false)
    expect(attempts).toBe(2)
    expect(waits).toBe(2)
  })
})
