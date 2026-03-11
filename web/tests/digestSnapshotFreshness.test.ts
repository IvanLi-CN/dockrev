import { describe, expect, test } from 'bun:test'

import {
  isSnapshotFreshEnough,
  type SnapshotFreshnessBaseline,
} from '../src/digestSnapshotFreshness'

describe('digestSnapshotFreshness', () => {
  test('accepts snapshots newer than the previous checkedAt', () => {
    const baseline: SnapshotFreshnessBaseline = {
      checkedAt: '2026-03-11T14:00:00Z',
      startedAtMs: Date.parse('2026-03-11T14:05:00Z'),
    }

    expect(isSnapshotFreshEnough('2026-03-11T14:00:01Z', baseline)).toBe(true)
    expect(isSnapshotFreshEnough('2026-03-11T14:00:00Z', baseline)).toBe(false)
  })

  test('falls back to refresh start time when no previous checkedAt exists', () => {
    const baseline: SnapshotFreshnessBaseline = {
      checkedAt: null,
      startedAtMs: Date.parse('2026-03-11T14:05:00Z'),
    }

    expect(isSnapshotFreshEnough('2026-03-11T14:04:30Z', baseline)).toBe(true)
    expect(isSnapshotFreshEnough('2026-03-11T14:03:30Z', baseline)).toBe(false)
  })
})
