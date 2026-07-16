import { describe, expect, test } from 'bun:test'

import type { ServiceReleaseNoteItem } from '../src/api'
import {
  formatVersionDirectoryTimeLabel,
  mergeReleaseNoteItems,
} from '../src/components/serviceVersionsSectionUtils'

const NOW = Date.UTC(2026, 6, 16, 12, 0, 0)

function isoOffset(offsetMs: number): string {
  return new Date(NOW - offsetMs).toISOString()
}

function note(id: string, tagName: string): ServiceReleaseNoteItem {
  return {
    id,
    tagName,
    htmlUrl: `https://github.com/acme/app/releases/tag/${tagName}`,
    draft: false,
    prerelease: false,
    publishedAt: '2026-07-16T00:00:00.000Z',
  }
}

describe('formatVersionDirectoryTimeLabel', () => {
  test('formats just now, minutes, hours, and days within the seven-day window', () => {
    expect(formatVersionDirectoryTimeLabel(isoOffset(30_000), NOW)).toBe('刚刚')
    expect(formatVersionDirectoryTimeLabel(isoOffset(5 * 60_000), NOW)).toBe('5 分钟前')
    expect(formatVersionDirectoryTimeLabel(isoOffset(3 * 60 * 60_000), NOW)).toBe('3 小时前')
    expect(formatVersionDirectoryTimeLabel(isoOffset(6 * 24 * 60 * 60_000), NOW)).toBe('6 天前')
  })

  test('keeps the seven-day boundary relative and older values absolute', () => {
    expect(formatVersionDirectoryTimeLabel(isoOffset(7 * 24 * 60 * 60_000), NOW)).toBe('7 天前')
    expect(formatVersionDirectoryTimeLabel(isoOffset((7 * 24 * 60 * 60_000) + 1), NOW)).toBe('2026-07-09')
  })

  test('falls back for invalid or missing timestamps', () => {
    expect(formatVersionDirectoryTimeLabel('not-a-date', NOW)).toBe('not-a-date')
    expect(formatVersionDirectoryTimeLabel('', NOW)).toBe('时间未知')
  })
})

describe('mergeReleaseNoteItems', () => {
  test('deduplicates incoming items by stable id while preserving order', () => {
    const merged = mergeReleaseNoteItems(
      [note('github:1', 'v1.0.0'), note('github:2', 'v1.1.0')],
      [note('github:2', 'v1.1.0'), note('github:3', 'v1.2.0')],
    )

    expect(merged.map((item) => item.id)).toEqual(['github:1', 'github:2', 'github:3'])
  })
})
