import { describe, expect, test } from 'bun:test'

import { splitImageNameForDisplay } from '../src/imageLinks'

describe('splitImageNameForDisplay', () => {
  test('keeps repo and tag split for tag-plus-digest refs', () => {
    expect(splitImageNameForDisplay('acme/api:1.2.3@sha256:deadbeef', '1.2.3')).toEqual({
      base: 'acme/api',
      suffix: ':1.2.3@sha256:deadbeef',
    })
  })

  test('keeps digest-only refs visible in the suffix', () => {
    expect(splitImageNameForDisplay('acme/api@sha256:deadbeef', 'latest')).toEqual({
      base: 'acme/api',
      suffix: '@sha256:deadbeef',
    })
  })
})
