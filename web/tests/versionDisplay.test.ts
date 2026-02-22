import { describe, expect, test } from 'bun:test'

import { formatCandidateTagDisplay, formatCurrentTagDisplay } from '../src/versionDisplay'

describe('versionDisplay', () => {
  test('prefers resolved current tag when it is strict semver', () => {
    expect(formatCurrentTagDisplay('latest', 'v0.2.51')).toBe('v0.2.51')
  })

  test('falls back to raw current semver and hides non-semver current tag', () => {
    expect(formatCurrentTagDisplay('0.2.51', null)).toBe('0.2.51')
    expect(formatCurrentTagDisplay('latest', null)).toBe('-')
  })

  test('prefers resolved candidate tag when it is strict semver', () => {
    expect(formatCandidateTagDisplay('latest', 'v0.2.51')).toBe('v0.2.51')
  })

  test('falls back to raw candidate tag when resolved tag is missing or non-semver', () => {
    expect(formatCandidateTagDisplay('latest', null)).toBe('latest')
    expect(formatCandidateTagDisplay('latest', 'stable')).toBe('latest')
  })
})
