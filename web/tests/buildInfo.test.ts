import { describe, expect, test } from 'bun:test'

import { dockrevBuildInfo, isDemoBundle, releaseTagForVersion } from '../src/buildInfo'

describe('build info contract', () => {
  test('a non-injected runtime reports unknown instead of inventing a version', () => {
    const info = dockrevBuildInfo()
    expect(info.variant).toBe('live')
    expect(info.version).toBe('unknown')
    expect(isDemoBundle()).toBe(false)
  })

  test('only release builds link a release tag, and they use the v prefix', () => {
    expect(
      releaseTagForVersion({
        variant: 'demo',
        version: '0.81.11',
        channel: 'release',
        sourceSha: null,
        builtAt: '',
      }),
    ).toBe('v0.81.11')
    expect(
      releaseTagForVersion({
        variant: 'demo',
        version: 'main+abc1234',
        channel: 'main',
        sourceSha: 'abc1234',
        builtAt: '',
      }),
    ).toBeNull()
    expect(
      releaseTagForVersion({
        variant: 'demo',
        version: '0.0.0-pr.abc1234',
        channel: 'pr',
        sourceSha: 'abc1234',
        builtAt: '',
      }),
    ).toBeNull()
  })
})
