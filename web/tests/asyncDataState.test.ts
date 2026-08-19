import { describe, expect, test } from 'bun:test'
import {
  asyncOverlayDelay,
  canShowAsyncEmpty,
  isAsyncDataBusy,
  isAsyncDataOffline,
} from '../src/asyncData'
import { trimSamplesToWindow } from '../src/components/ServiceResourcePanel'

describe('async data continuity contract', () => {
  test('keeps loading distinct from empty and offline', () => {
    expect(canShowAsyncEmpty('initial-loading')).toBe(false)
    expect(canShowAsyncEmpty('refreshing')).toBe(false)
    expect(canShowAsyncEmpty('ready-empty')).toBe(true)
    expect(isAsyncDataBusy('initial-loading')).toBe(true)
    expect(isAsyncDataBusy('refreshing')).toBe(true)
    expect(isAsyncDataOffline('error', false)).toBe(false)
    expect(isAsyncDataOffline('offline', true)).toBe(false)
    expect(isAsyncDataOffline('offline', false)).toBe(true)
  })

  test('uses the short user-action and long snapshot overlay thresholds', () => {
    expect(asyncOverlayDelay('memory')).toBe(200)
    expect(asyncOverlayDelay('live')).toBe(200)
    expect(asyncOverlayDelay('fresh-snapshot')).toBe(800)
  })

  test('trims resource snapshots against the current time window', () => {
    const now = Date.parse('2026-08-20T10:00:00.000Z')
    const samples = [
      { sampledAt: 'not-a-date', cpuPercent: 99, containerCount: 1 },
      { sampledAt: '2026-08-20T08:00:00.000Z', cpuPercent: 1, containerCount: 1 },
      { sampledAt: '2026-08-20T09:30:00.000Z', cpuPercent: 2, containerCount: 1 },
      { sampledAt: '2026-08-20T10:00:01.000Z', cpuPercent: 3, containerCount: 1 },
      { sampledAt: '2026-08-20T09:45:00.000Z', cpuPercent: 4, containerCount: 1 },
    ]

    const trimmed = trimSamplesToWindow(samples, 1_800, now)

    expect(trimmed.map((sample) => sample.cpuPercent)).toEqual([2, 4])
  })
})
