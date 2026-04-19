import { describe, expect, test } from 'bun:test'

import type { Service } from '../src/api'
import {
  partitionAggregateUpdateServices,
  resolveAggregateUpdateActionState,
} from '../src/aggregateUpdateGuard'

;(globalThis as typeof globalThis & {
  window?: { __DOCKREV_CONFIG__?: { dockrevImageRepo?: string } }
}).window = {
  __DOCKREV_CONFIG__: {
    dockrevImageRepo: 'ghcr.io/ivanli-cn/dockrev',
  },
}

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

describe('aggregateUpdateGuard', () => {
  test('keeps dockrev aggregate guard semantics', () => {
    const guarded = makeService({
      id: 'svc-dockrev',
      name: 'dockrev',
      image: {
        ref: 'ghcr.io/ivanli-cn/dockrev:latest',
        tag: 'latest',
        digest: 'sha256:current',
        resolvedTag: 'v1.0.0',
        resolvedTags: ['v1.0.0'],
      },
    })
    const partition = partitionAggregateUpdateServices([guarded])

    expect(partition.guardedDockrevPreview).toHaveLength(1)
    expect(partition.actionable).toHaveLength(0)

    const action = resolveAggregateUpdateActionState(partition)
    expect(action.enabled).toBe(false)
  })
})
