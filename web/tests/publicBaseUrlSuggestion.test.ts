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

  test('preserves the current page base path for hash-routed settings pages', () => {
    expect(derivePublicBaseUrlSuggestion('/settings', 'https://example.com', '/dockrev/')).toBe('https://example.com/dockrev/')
    expect(derivePublicBaseUrlSuggestion('/settings', 'https://example.com', '/dockrev/index.html')).toBe('https://example.com/dockrev/')
    expect(derivePublicBaseUrlSuggestion('/settings', 'https://example.com', '/v1.2.3/')).toBe('https://example.com/v1.2.3/')
  })

  test('ignores the storybook iframe pathname when inferring from hash routing', () => {
    expect(derivePublicBaseUrlSuggestion('/settings', 'https://example.com', '/iframe.html')).toBe('https://example.com/')
  })

  test('keeps the existing path when called with an unexpected route', () => {
    expect(derivePublicBaseUrlSuggestion('/dockrev', 'https://example.com')).toBe('https://example.com/dockrev/')
  })
})
