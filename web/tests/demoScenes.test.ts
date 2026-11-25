import { describe, expect, test } from 'bun:test'

import {
  applyDemoShareStateToSearchParams,
  buildDemoHref,
  defaultShareState,
  DEMO_SCENES,
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
      persona: 'admin',
      network: 'faulty',
      asyncCase: 'error',
    })
    const href = buildDemoHref(state, '/')
    const restored = readDemoShareState(new URL(`https://demo.local${href}`))
    expect(restored).toEqual(state)
  })

  test('the URL keeps only non-default state and the hidden-controls flag', () => {
    const state = { ...defaultShareState('services'), persona: 'admin' as const, controlsHidden: true }
    const params = new URLSearchParams('demo=services&d_persona=admin&d_net=slow&stale=1')
    applyDemoShareStateToSearchParams(params, state)
    expect(params.get('d_persona')).toBe('admin')
    expect(params.get('d_net')).toBeNull()
    expect(params.get('stale')).toBe('1')
    expect(params.get('d_controls')).toBe('hidden')
    expect(params.get('demo')).toBe('services')
  })

  test('scene switch resets state that belongs to the previous scene', () => {
    const previous = withShareStatePatch(defaultShareState('cleanup'), { cleanup: 'cleanup-console-confirm-failed' })
    const next = withShareStatePatch(previous, { sceneId: 'overview' })
    expect(next.cleanup).toBe('cleanup-console-storage-normal')
    expect(next.sceneId).toBe('overview')
  })

  test('demo href carries a trailing-scoped base path', () => {
    const href = buildDemoHref(defaultShareState('cleanup'), '/')
    expect(href).toBe('/cleanup?demo=cleanup')
  })
})
