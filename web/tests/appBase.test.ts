import { describe, expect, test } from 'bun:test'

import {
  isAppBaseEntryPath,
  normalizeAppBasePath,
  stripAppBaseFromPath,
  withAppBasePath,
} from '../src/appBase'

describe('app base helpers', () => {
  test('normalizes root and nested base paths', () => {
    expect(normalizeAppBasePath(undefined)).toBe('/')
    expect(normalizeAppBasePath('./')).toBe('/')
    expect(normalizeAppBasePath('/demo')).toBe('/demo/')
    expect(normalizeAppBasePath('repo/demo')).toBe('/repo/demo/')
    expect(normalizeAppBasePath('https://docs.example.test/dockrev/demo/')).toBe('/dockrev/demo/')
  })

  test('strips the configured base path from runtime pathnames', () => {
    expect(stripAppBaseFromPath('/demo/', '/demo/')).toBe('/')
    expect(stripAppBaseFromPath('/demo/', '/demo/services')).toBe('/services')
    expect(stripAppBaseFromPath('/repo/demo/', '/repo/demo/services/stack-prod')).toBe('/services/stack-prod')
  })

  test('prefixes route hrefs with the configured base path', () => {
    expect(withAppBasePath('/demo/', '/')).toBe('/demo/')
    expect(withAppBasePath('/demo/', '/services')).toBe('/demo/services')
    expect(withAppBasePath('/repo/demo/', '/settings')).toBe('/repo/demo/settings')
  })

  test('recognizes base entry paths and base index documents', () => {
    expect(isAppBaseEntryPath('/demo/', '/demo/')).toBe(true)
    expect(isAppBaseEntryPath('/demo/', '/demo/index.html')).toBe(true)
    expect(isAppBaseEntryPath('/demo/', '/demo/services')).toBe(false)
  })
})
