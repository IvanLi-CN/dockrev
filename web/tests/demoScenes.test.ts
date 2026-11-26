import { describe, expect, test } from 'bun:test'

import {
  DEMO_MAX_CUSTOM_DELAY_MS,
  DEMO_SLOW_DELAY_MS,
  applyDemoShareStateToSearchParams,
  buildDemoHref,
  clampDemoDelayMs,
  defaultShareState,
  DEMO_PERSONA_OPTIONS,
  DEMO_SCENES,
  demoNetworkMode,
  demoNetworkModePatch,
  demoRequestDelayMs,
  demoSceneForPath,
  readDemoShareState,
  withShareStatePatch,
} from '../src/demo/scenes'

describe('demo scene registry', () => {
  test('every scene path is distinct and reachable through the app router contract', () => {
    const paths = DEMO_SCENES.map((scene) => scene.path)
    expect(new Set(paths).size).toBe(paths.length)
    expect(demoSceneForPath('/services')).toBe('services')
    expect(demoSceneForPath('/queue')).toBe('queue')
    expect(demoSceneForPath('/queue/job-live-long')).toBe('queue-logs')
    expect(demoSceneForPath('/settings/ghcr-webhooks')).toBe('ghcr-registry')
  })

  test('the page wins over a stale demo parameter', () => {
    const state = readDemoShareState(new URL('https://demo.local/cleanup?demo=queue'))
    expect(state.sceneId).toBe('cleanup')
  })

  test('share state round-trips through the URL without losing non-default fields', () => {
    const state = withShareStatePatch(defaultShareState('queue-logs'), {
      persona: 'allowed',
      connection: 'offline',
      delay: 'custom',
      delayMs: 2300,
      motion: 'reduce',
      asyncCase: 'error',
    })
    const href = buildDemoHref(state, '/')
    const restored = readDemoShareState(new URL(`https://demo.local${href}`))
    expect(restored).toEqual(state)
  })

  test('the URL keeps only non-default state and the hidden-controls flag', () => {
    const state = { ...defaultShareState('services'), persona: 'blocked' as const, controlsHidden: true }
    const params = new URLSearchParams('demo=services&d_persona=blocked&d_net=slow&stale=1')
    applyDemoShareStateToSearchParams(params, state)
    expect(params.get('d_persona')).toBe('blocked')
    expect(params.get('d_net')).toBeNull()
    expect(params.get('stale')).toBe('1')
    expect(params.get('d_controls')).toBe('hidden')
    expect(params.get('demo')).toBe('services')
  })

  test('an environment choice survives a scene switch', () => {
    const environment = withShareStatePatch(defaultShareState('overview'), {
      persona: 'blocked',
      connection: 'offline',
      delay: 'custom',
      delayMs: 900,
      motion: 'reduce',
    })
    const next = withShareStatePatch(environment, { sceneId: 'cleanup' })
    expect(next.persona).toBe('blocked')
    expect(next.connection).toBe('offline')
    expect(next.delay).toBe('custom')
    expect(next.delayMs).toBe(900)
    expect(next.motion).toBe('reduce')
    // Scene-owned state still re-seeds for the target scene.
    expect(next.sceneId).toBe('cleanup')
  })

  test('valid URL fields override the session while missing ones keep it', () => {
    const session = withShareStatePatch(defaultShareState('queue'), {
      persona: 'allowed',
      connection: 'offline',
      delay: 'custom',
      delayMs: 1200,
    })
    const restored = readDemoShareState(new URL('https://demo.local/queue?demo=queue&d_persona=blocked'), session)
    expect(restored.persona).toBe('blocked')
    expect(restored.connection).toBe('offline')
    expect(restored.delay).toBe('custom')
    expect(restored.delayMs).toBe(1200)
  })

  test('delay policy maps presets, custom values and offline deterministically', () => {
    const base = defaultShareState('queue')
    expect(demoRequestDelayMs(base)).toBe(0)
    expect(demoRequestDelayMs({ ...base, delay: 'slow' })).toBe(DEMO_SLOW_DELAY_MS)
    expect(demoRequestDelayMs({ ...base, delay: 'custom', delayMs: 240 })).toBe(240)
    expect(demoRequestDelayMs({ ...base, connection: 'offline', delay: 'slow' })).toBe(0)
  })

  test('custom delay is clamped to the documented range', () => {
    expect(clampDemoDelayMs(-5)).toBe(0)
    expect(clampDemoDelayMs(1e9)).toBe(DEMO_MAX_CUSTOM_DELAY_MS)
    expect(clampDemoDelayMs(1234.6)).toBe(1235)
    expect(clampDemoDelayMs(Number.NaN)).toBe(0)
  })

  test('demo href carries a trailing-scoped base path', () => {
    const href = buildDemoHref(defaultShareState('cleanup'), '/')
    expect(href).toBe('/cleanup?demo=cleanup')
  })
})

describe('demo identity follows the product authorization model', () => {
  test('only the two real authorization outcomes are accepted', () => {
    expect(DEMO_PERSONA_OPTIONS.map((option) => option.value)).toEqual(['allowed', 'blocked'])
  })

  test('legacy role-like share links map onto real outcomes', () => {
    const read = (value: string) =>
      readDemoShareState(new URL(`https://demo.local/services?demo=services&d_persona=${value}`)).persona
    expect(read('admin')).toBe('allowed')
    expect(read('user')).toBe('allowed')
    expect(read('guest')).toBe('blocked')
    expect(read('allowed')).toBe('allowed')
    expect(read('blocked')).toBe('blocked')
    expect(read('nonsense')).toBe(defaultShareState('services').persona)
  })
})

describe('combined network and latency control', () => {
  test('the three visible modes map onto the two persisted fields', () => {
    const base = defaultShareState('services')
    expect(demoNetworkMode(base)).toBe('normal')
    expect(demoNetworkMode({ ...base, delay: 'slow' })).toBe('slow')
    expect(demoNetworkMode({ ...base, connection: 'offline' })).toBe('offline')
    expect(demoNetworkMode({ ...base, delay: 'custom', delayMs: 900 })).toBe('custom')
  })

  test('choosing a mode only writes the fields that mode owns', () => {
    const slow = defaultShareState('services')
    expect(demoNetworkModePatch(slow, 'slow')).toEqual({ connection: 'online', delay: 'slow' })
    expect(demoNetworkModePatch(slow, 'normal')).toEqual({ connection: 'online', delay: 'normal' })
    expect(demoNetworkModePatch(slow, 'offline')).toEqual({ connection: 'offline', delay: 'normal' })
  })

  test('leaving offline restores the delay that was in effect', () => {
    const offlineSlow = { ...defaultShareState('services'), connection: 'offline' as const, delay: 'slow' as const }
    expect(demoNetworkModePatch(offlineSlow, 'slow')).toEqual({ connection: 'online', delay: 'slow' })
  })

  test('a custom delay is reachable but never claimed by the three-state control', () => {
    const custom = { ...defaultShareState('services'), delay: 'custom' as const, delayMs: 1200 }
    expect(demoNetworkMode(custom)).toBe('custom')
    expect(demoRequestDelayMs(custom)).toBe(1200)
    // Choosing a visible mode exits custom mode explicitly.
    expect(demoNetworkModePatch(custom, 'normal')).toEqual({ connection: 'online', delay: 'normal' })
  })
})
