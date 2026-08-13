import { describe, expect, test } from 'bun:test'

import type { Service } from '../src/api'
import type { ManagementEvent } from '../src/managementEvents'
import { managementEventAffectsServiceDetail } from '../src/pages/useServiceDetailPageState'

const service = {
  id: 'svc-1',
  name: 'web',
  image: {
    ref: 'ghcr.io/acme/web:latest',
    tag: 'latest',
    digest: 'sha256:current',
  },
  candidate: {
    tag: '2.0.0',
    digest: 'sha256:candidate',
    archMatch: 'match',
    arch: [],
  },
} as Service

function versionInferenceEvent(digest: string): ManagementEvent {
  return {
    type: 'entities_changed',
    domain: 'version_inference',
    entities: [{ entityType: 'task', id: 'digest-task' }],
    version: 1,
    summary: {
      phase: 'finished',
      imageRepo: 'ghcr.io/acme/web',
      digest,
    },
  }
}

describe('service detail management events', () => {
  test('refreshes when a finished version inference matches the service image', () => {
    expect(managementEventAffectsServiceDetail(
      versionInferenceEvent('sha256:current'),
      'stack-1',
      'svc-1',
      service,
    )).toBe(true)
  })

  test('does not refresh for a different inferred digest', () => {
    expect(managementEventAffectsServiceDetail(
      versionInferenceEvent('sha256:other'),
      'stack-1',
      'svc-1',
      service,
    )).toBe(false)
  })
})
