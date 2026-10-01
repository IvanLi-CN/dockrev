import { describe, expect, test } from 'bun:test'

import { compareStackNamesNaturally } from '../src/stackNameSorting'

describe('stack name natural sorting', () => {
  test('compares numeric name segments by value', () => {
    expect(['stack-10', 'stack-2', 'stack-1'].sort(compareStackNamesNaturally)).toEqual([
      'stack-1',
      'stack-2',
      'stack-10',
    ])
  })

  test('compares letter case without splitting equivalent names', () => {
    expect(compareStackNamesNaturally('Alpha', 'alpha')).toBe(0)
  })
})
