import { describe, expect, test } from 'bun:test'
import { filterCleanupResponseForView } from '../src/pages/cleanupPageModel'
import { queueContextJobBuckets } from '../src/components/PageContextNavigation'
import type { CleanupScanResponse, CompactJobListItem } from '../src/api'

function job(id: string, status: string, finishedAt: string): CompactJobListItem {
  return { id, type: 'update', scope: 'service', status, createdBy: 'test', reason: 'test', createdAt: finishedAt, finishedAt, displayLabel: id }
}

describe('page context navigation models', () => {
  test('keeps active tasks and only the newest five terminal tasks', () => {
    const jobs = [job('running', 'running', '2026-01-01T00:00:00Z'), ...Array.from({ length: 7 }, (_, index) => job(`done-${index}`, 'success', `2026-01-0${index + 1}T00:00:00Z`))]
    const buckets = queueContextJobBuckets(jobs)
    expect(buckets.active.map((item) => item.id)).toEqual(['running'])
    expect(buckets.recent).toHaveLength(5)
    expect(buckets.recent[0]?.id).toBe('done-6')
  })

  test('filters cleanup display data without changing the source response', () => {
    const response: CleanupScanResponse = {
      status: 'ready', reason: 'page', preset: 'aggressive', scope: 'all', stackGroups: [{ stackId: 's', stackName: 'stack', estimatedReclaimableBytes: 3, stackOrphans: [{ resourceId: 'i', kind: 'image', label: 'image', reason: 'old', minPreset: 'conservative', estimatedReclaimableBytes: 2 }], services: [{ serviceId: 'svc', serviceName: 'service', estimatedReclaimableBytes: 1, resources: [{ resourceId: 'c', kind: 'container', label: 'container', reason: 'stopped', minPreset: 'conservative', estimatedReclaimableBytes: 1 }] }] }], unownedGroup: null,
    }
    const filtered = filterCleanupResponseForView(response, 'service', ['container'])
    expect(filtered.stackGroups[0]?.stackOrphans).toHaveLength(0)
    expect(filtered.stackGroups[0]?.services[0]?.resources[0]?.kind).toBe('container')
    expect(response.stackGroups[0]?.stackOrphans).toHaveLength(1)
    expect(response.scope).toBe('all')
  })
})
