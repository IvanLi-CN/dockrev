import { describe, expect, test } from 'bun:test'

import { shouldResetReleaseDrawerOnRouteChange } from '../src/releaseDrawer'

describe('release drawer route transitions', () => {
  test('closes an open drawer when hash-routed navigation switches pages', () => {
    expect(
      shouldResetReleaseDrawerOnRouteChange({
        drawerOpen: true,
        hashRouting: true,
        previousPathname: '/services',
        nextPathname: '/settings',
      }),
    ).toBe(true)
  })

  test('keeps the drawer state for normal routing page changes', () => {
    expect(
      shouldResetReleaseDrawerOnRouteChange({
        drawerOpen: true,
        hashRouting: false,
        previousPathname: '/services',
        nextPathname: '/settings',
      }),
    ).toBe(false)
  })

  test('keeps the drawer when the hash route pathname does not change', () => {
    expect(
      shouldResetReleaseDrawerOnRouteChange({
        drawerOpen: true,
        hashRouting: true,
        previousPathname: '/services',
        nextPathname: '/services',
      }),
    ).toBe(false)
  })
})
