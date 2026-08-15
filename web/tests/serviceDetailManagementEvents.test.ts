import { describe, expect, test } from 'bun:test'

import type { Service } from '../src/api'
import type { ManagementEvent } from '../src/managementEvents'
import { managementEventAffectsServiceDetail } from '../src/pages/useServiceDetailPageState'
import { formatMockManagementCursor, parseMockManagementCursor } from '../src/stories/mocks/dockrevMockApi/managementEventCursor'

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
  test('keeps management cursors generation-qualified and versionable', () => {
    const cursor = formatMockManagementCursor('storybook', 42)
    expect(cursor).toBe('storybook:42')
    expect(parseMockManagementCursor(cursor, 'storybook')).toEqual({ kind: 'valid', id: 42 })
  })

  test('requests resync for invalid or cross-generation management cursors', () => {
    expect(parseMockManagementCursor('storybook:not-a-number', 'storybook')).toEqual({ kind: 'resync', reason: 'invalid_cursor' })
    expect(parseMockManagementCursor('other:42', 'storybook')).toEqual({ kind: 'resync', reason: 'generation_changed' })
    expect(parseMockManagementCursor('storybook:0x10', 'storybook')).toEqual({ kind: 'resync', reason: 'invalid_cursor' })
    expect(parseMockManagementCursor('storybook:', 'storybook')).toEqual({ kind: 'resync', reason: 'invalid_cursor' })
    expect(parseMockManagementCursor(`storybook:${'9'.repeat(400)}`, 'storybook')).toEqual({ kind: 'resync', reason: 'invalid_cursor' })
  })

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

  test('refreshes when a terminal jobs event names the service', () => {
    expect(managementEventAffectsServiceDetail(
      {
        type: 'entities_changed',
        domain: 'jobs',
        entities: [{ entityType: 'service', id: 'svc-1' }],
        version: 0,
        summary: { jobId: 'job-update-1', terminal: true, status: 'success' },
      },
      'stack-1',
      'svc-1',
      service,
    )).toBe(true)
  })
})
