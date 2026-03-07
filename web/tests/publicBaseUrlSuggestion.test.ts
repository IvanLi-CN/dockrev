import { describe, expect, test } from 'bun:test'

import { derivePublicBaseUrlSuggestion } from '../src/publicBaseUrlSuggestion'

describe('derivePublicBaseUrlSuggestion', () => {
  test('returns the site root for the default settings route', () => {
    expect(derivePublicBaseUrlSuggestion('/settings', 'https://dockrev.ivanli.cc')).toBe('https://dockrev.ivanli.cc/')
    expect(derivePublicBaseUrlSuggestion('/settings/', 'https://dockrev.ivanli.cc')).toBe('https://dockrev.ivanli.cc/')
  })

  test('preserves an app base path before the settings segment', () => {
    expect(derivePublicBaseUrlSuggestion('/dockrev/settings', 'https://example.com')).toBe('https://example.com/dockrev/')
    expect(derivePublicBaseUrlSuggestion('/dockrev/settings/', 'https://example.com')).toBe('https://example.com/dockrev/')
  })

  test('keeps the existing path when called with an unexpected route', () => {
    expect(derivePublicBaseUrlSuggestion('/dockrev', 'https://example.com')).toBe('https://example.com/dockrev/')
  })
})
