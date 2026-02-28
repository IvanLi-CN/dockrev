import { describe, expect, test } from 'bun:test'

import type { Service } from '../src/api'
import { isSemverDowngradeAnomaly, noteFor, serviceRowStatus } from '../src/updateStatus'

function makeService(overrides?: Partial<Service>): Service {
  return {
    id: 'svc-1',
    name: 'svc-1',
    image: {
      ref: 'ghcr.io/acme/demo:latest',
      tag: 'latest',
      digest: 'sha256:current',
      resolvedTag: 'v0.3.1',
      resolvedTags: ['v0.3.1'],
    },
    candidate: {
      tag: 'latest',
      resolvedTag: 'v0.2.53',
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

describe('updateStatus semver downgrade anomaly', () => {
  test('flags downgrade when current/candidate are strict semver and candidate is lower', () => {
    const svc = makeService()
    expect(isSemverDowngradeAnomaly(svc)).toBe(true)
    expect(serviceRowStatus(svc)).toBe('updatable')
    expect(noteFor(svc, 'updatable')).toContain('版本异常')
  })

  test('does not flag when candidate is non-semver', () => {
    const svc = makeService({
      candidate: {
        tag: 'latest',
        resolvedTag: null,
        digest: 'sha256:candidate',
        archMatch: 'match',
        arch: ['linux/amd64'],
      },
    })

    expect(isSemverDowngradeAnomaly(svc)).toBe(false)
  })

  test('does not flag when candidate looks like semver but is non-strict', () => {
    const svc = makeService({
      candidate: {
        tag: 'latest',
        resolvedTag: 'v0.02.53',
        digest: 'sha256:candidate',
        archMatch: 'match',
        arch: ['linux/amd64'],
      },
    })

    expect(isSemverDowngradeAnomaly(svc)).toBe(false)
  })

  test('does not flag when candidate is higher', () => {
    const svc = makeService({
      candidate: {
        tag: 'latest',
        resolvedTag: 'v0.3.2',
        digest: 'sha256:candidate',
        archMatch: 'match',
        arch: ['linux/amd64'],
      },
    })

    expect(isSemverDowngradeAnomaly(svc)).toBe(false)
  })

  test('flags prerelease candidate as downgrade when current is stable release', () => {
    const svc = makeService({
      image: {
        ref: 'ghcr.io/acme/demo:latest',
        tag: 'latest',
        digest: 'sha256:current',
        resolvedTag: 'v1.0.0',
        resolvedTags: ['v1.0.0'],
      },
      candidate: {
        tag: 'latest',
        resolvedTag: 'v1.0.0-rc.1+build.5',
        digest: 'sha256:candidate',
        archMatch: 'match',
        arch: ['linux/amd64'],
      },
    })

    expect(isSemverDowngradeAnomaly(svc)).toBe(true)
  })

  test('keeps backup hint when downgrade anomaly and force backup both apply', () => {
    const svc = makeService({
      settings: {
        autoRollback: true,
        backupTargets: {
          bindPaths: { '/data': 'force' },
          volumeNames: {},
        },
      },
    })

    expect(noteFor(svc, 'updatable')).toContain('版本异常')
    expect(noteFor(svc, 'updatable')).toContain('备份通过后执行')
  })

  test('keeps hint status semantics and shows anomaly message first', () => {
    const svc = makeService({
      candidate: {
        tag: 'latest',
        resolvedTag: 'v0.2.53',
        digest: 'sha256:candidate',
        archMatch: 'unknown',
        arch: ['linux/amd64'],
      },
    })

    expect(serviceRowStatus(svc)).toBe('hint')
    expect(noteFor(svc, 'hint')).toContain('版本异常')
  })
})
