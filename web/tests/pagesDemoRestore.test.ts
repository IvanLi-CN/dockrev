import { describe, expect, test } from 'bun:test'

import {
  canonicalPagesDemoEntryPath,
  parsePagesDemoRestoreEntry,
  shouldRestorePagesDemoPath,
} from '../src/demo/pagesDemoRestore'

describe('pages demo restore helpers', () => {
  test('parses valid restore entries', () => {
    expect(
      parsePagesDemoRestoreEntry(
        JSON.stringify({
          path: '/demo/services/stack-prod/svc-prod-api',
          savedAt: 1_725_000_000_000,
        }),
      ),
    ).toEqual({
      path: '/demo/services/stack-prod/svc-prod-api',
      savedAt: 1_725_000_000_000,
    })
  })

  test('rejects malformed restore entries', () => {
    expect(parsePagesDemoRestoreEntry(null)).toBeNull()
    expect(parsePagesDemoRestoreEntry('{}')).toBeNull()
    expect(
      parsePagesDemoRestoreEntry(
        JSON.stringify({
          path: 'demo/services',
          savedAt: 1,
        }),
      ),
    ).toBeNull()
  })

  test('restores only fresh pending paths when the current location is the base entry', () => {
    const savedAt = 1_725_000_000_000
    expect(
      shouldRestorePagesDemoPath({
        currentBasePath: '/demo/',
        currentPathname: '/demo/',
        pendingEntry: {
          path: '/demo/services/stack-prod/svc-prod-api',
          savedAt,
        },
        now: savedAt + 60_000,
      }),
    ).toBe(true)

    expect(
      shouldRestorePagesDemoPath({
        currentBasePath: '/demo/',
        currentPathname: '/demo/index.html',
        pendingEntry: {
          path: '/demo/services/stack-prod/svc-prod-api',
          savedAt,
        },
        now: savedAt + 60_000,
      }),
    ).toBe(true)

    expect(
      shouldRestorePagesDemoPath({
        currentBasePath: '/demo/',
        currentPathname: '/demo/services',
        pendingEntry: {
          path: '/demo/services/stack-prod/svc-prod-api',
          savedAt,
        },
        now: savedAt + 60_000,
      }),
    ).toBe(false)

    expect(
      shouldRestorePagesDemoPath({
        currentBasePath: '/demo/',
        currentPathname: '/demo/',
        pendingEntry: {
          path: '/demo/services/stack-prod/svc-prod-api',
          savedAt,
        },
        now: savedAt + 10 * 60_000,
      }),
    ).toBe(false)
  })

  test('canonicalizes index-style demo entry paths back to the base route', () => {
    expect(canonicalPagesDemoEntryPath('/demo/', '/demo/index.html')).toBe('/demo/')
    expect(canonicalPagesDemoEntryPath('/dockrev/demo/', '/dockrev/demo')).toBe('/dockrev/demo/')
    expect(canonicalPagesDemoEntryPath('/demo/', '/demo/')).toBeNull()
    expect(canonicalPagesDemoEntryPath('/demo/', '/demo/services')).toBeNull()
  })
})
