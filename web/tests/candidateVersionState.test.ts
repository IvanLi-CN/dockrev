import { describe, expect, test } from 'bun:test'

import type { Service } from '../src/api'
import { resolveCandidateVersionState } from '../src/candidateVersionState'

function makeService(overrides?: Partial<Service>): Service {
  return {
    id: 'svc-1',
    name: 'svc-1',
    image: {
      ref: 'ghcr.io/acme/demo:latest',
      tag: 'latest',
      digest: 'sha256:current',
      resolvedTag: 'v1.0.0',
      resolvedTags: ['v1.0.0'],
    },
    candidate: {
      tag: 'latest',
      resolvedTag: 'v1.1.0',
      digest: 'sha256:candidate',
      archMatch: 'match',
      arch: ['linux/amd64'],
    },
    ignore: null,
    versionInference: { status: 'ready', reason: null, checkedAt: null },
    settings: {
      autoRollback: true,
      backupTargets: {
        bindPaths: {},
        volumeNames: {},
      },
    },
    archived: false,
    ...overrides,
  }
}

describe('candidateVersionState', () => {
  test('keeps candidate visible when resolved tags differ', () => {
    const state = resolveCandidateVersionState(makeService())

    expect(state.showCandidate).toBe(true)
    expect(state.sameDisplayUpdate).toBe(false)
    expect(state.candidateDisplayTag).toBe('v1.1.0')
  })

  test('flags same-tag digest-only updates explicitly', () => {
    const state = resolveCandidateVersionState(
      makeService({
        candidate: {
          tag: 'latest',
          resolvedTag: 'v1.0.0',
          digest: 'sha256:next',
          archMatch: 'match',
          arch: ['linux/amd64'],
        },
      }),
    )

    expect(state.showCandidate).toBe(true)
    expect(state.sameDisplayUpdate).toBe(true)
    expect(state.currentDisplayTag).toBe('v1.0.0')
    expect(state.candidateDisplayTag).toBe('v1.0.0')
  })
})
