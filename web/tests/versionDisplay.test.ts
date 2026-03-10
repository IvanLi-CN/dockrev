import { describe, expect, test } from 'bun:test'

import { formatCandidateTagDisplay, formatCurrentTagDisplay, pickSnapshotDisplayTag } from '../src/versionDisplay'

describe('versionDisplay', () => {
  test('prefers resolved current tag when it is strict semver', () => {
    expect(formatCurrentTagDisplay('latest', 'v0.2.51')).toBe('v0.2.51')
  })

  test('falls back to raw current semver and hides non-semver current tag', () => {
    expect(formatCurrentTagDisplay('0.2.51', null)).toBe('0.2.51')
    expect(formatCurrentTagDisplay('latest', null)).toBe('-')
  })

  test('shows current display as loading text during pending state', () => {
    expect(formatCurrentTagDisplay('latest', 'v0.2.51', 'pending')).toBe('加载中…')
  })

  test('prefers resolved candidate tag when it is strict semver', () => {
    expect(formatCandidateTagDisplay('latest', 'v0.2.51')).toBe('v0.2.51')
  })

  test('falls back to raw candidate tag when resolved tag is missing or non-semver', () => {
    expect(formatCandidateTagDisplay('latest', null)).toBe('latest')
    expect(formatCandidateTagDisplay('latest', 'stable')).toBe('latest')
  })

  test('keeps candidate visible during pending state', () => {
    expect(formatCandidateTagDisplay('latest', 'v0.2.51', 'pending')).toBe('v0.2.51')
    expect(formatCandidateTagDisplay('latest', null, 'pending')).toBe('latest')
  })

  test('picks the best semver-ish tag from a snapshot tag list', () => {
    expect(pickSnapshotDisplayTag(['latest', 'v0.2.51', '0.2.50'], 'latest')).toBe('v0.2.51')
    expect(pickSnapshotDisplayTag(['stable', 'latest'], 'latest')).toBeNull()

    // Prefer canonical release tags over prereleases when deriving from a floating raw tag.
    expect(
      pickSnapshotDisplayTag(
        ['v0.8.8-arm64', 'v0.8.8', '0.8.8', 'stable', 'latest'],
        'latest',
      ),
    ).toBe('v0.8.8')

    // Avoid rewriting already-stable semver tags.
    expect(pickSnapshotDisplayTag(['v1.2.3', '1.2.3'], 'v1.2.3')).toBeNull()
  })
})
