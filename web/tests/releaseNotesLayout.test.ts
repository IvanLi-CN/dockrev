import { describe, expect, test } from 'bun:test'

import { releaseNoteCardShouldReserveAside } from '../src/releaseNotes'

describe('release note card aside layout', () => {
  test('keeps the current deployed release on the desktop three-rail layout even when it is read-only', () => {
    expect(
      releaseNoteCardShouldReserveAside({
        currentMatch: true,
        showUpdate: false,
        showRollback: false,
        showCandidateStatus: false,
        showRollbackDigestStatus: false,
        showHistoricalStatus: false,
      }),
    ).toBe(true)
  })

  test('allows unrelated release cards to collapse when they have no status or actions to show', () => {
    expect(
      releaseNoteCardShouldReserveAside({
        currentMatch: false,
        showUpdate: false,
        showRollback: false,
        showCandidateStatus: false,
        showRollbackDigestStatus: false,
        showHistoricalStatus: false,
      }),
    ).toBe(false)
  })
})
