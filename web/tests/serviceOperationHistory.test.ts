import { describe, expect, test } from 'bun:test'

import type { JobListItem } from '../src/api'
import { selectRecentServiceUpdateJobs, selectServiceOperationJobs } from '../src/components/RecentUpdateRecords'

function makeJob(input: Partial<JobListItem> & Pick<JobListItem, 'id' | 'type' | 'status'>): JobListItem {
  return {
    id: input.id,
    type: input.type,
    scope: input.scope ?? 'service',
    stackId: input.stackId ?? 'stack-prod',
    serviceId: input.serviceId ?? null,
    status: input.status,
    createdBy: input.createdBy ?? 'ivan',
    reason: input.reason ?? 'ui',
    createdAt: input.createdAt ?? '2026-07-12T09:00:00.000Z',
    startedAt: input.startedAt ?? null,
    finishedAt: input.finishedAt ?? null,
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary: input.summary ?? {},
    progress: null,
  }
}

describe('service operation history', () => {
  test('includes matching updates and rollbacks across service, stack, and all scopes in time order', () => {
    const jobs = [
      makeJob({ id: 'service-update', type: 'update', status: 'success', serviceId: 'svc-api', finishedAt: '2026-07-12T09:01:00.000Z' }),
      makeJob({ id: 'stack-update', type: 'update', status: 'failed', serviceId: null, scope: 'stack', finishedAt: '2026-07-12T09:03:00.000Z', summary: {} }),
      makeJob({ id: 'all-update', type: 'update', status: 'running', serviceId: null, scope: 'all', startedAt: '2026-07-12T09:04:00.000Z', summary: { targets: [{ serviceId: 'svc-api' }] } }),
      makeJob({ id: 'all-other', type: 'update', status: 'success', serviceId: null, scope: 'all', finishedAt: '2026-07-12T09:05:30.000Z', summary: {} }),
      makeJob({ id: 'rollback', type: 'rollback', status: 'rolled_back', serviceId: 'svc-api', finishedAt: '2026-07-12T09:02:00.000Z' }),
      makeJob({ id: 'other-service', type: 'update', status: 'success', serviceId: 'svc-web', finishedAt: '2026-07-12T09:05:00.000Z' }),
      makeJob({ id: 'check', type: 'check', status: 'success', serviceId: 'svc-api', finishedAt: '2026-07-12T09:06:00.000Z' }),
    ]

    expect(selectServiceOperationJobs(jobs, 'svc-api', 'stack-prod').map((job) => job.id)).toEqual([
      'all-update',
      'stack-update',
      'rollback',
      'service-update',
    ])
  })

  test('keeps the overview summary limited to the newest three update jobs', () => {
    const jobs = [
      makeJob({ id: 'update-1', type: 'update', status: 'success', serviceId: 'svc-api', finishedAt: '2026-07-12T09:01:00.000Z' }),
      makeJob({ id: 'update-2', type: 'update', status: 'success', serviceId: 'svc-api', finishedAt: '2026-07-12T09:02:00.000Z' }),
      makeJob({ id: 'update-3', type: 'update', status: 'success', serviceId: 'svc-api', finishedAt: '2026-07-12T09:03:00.000Z' }),
      makeJob({ id: 'update-4', type: 'update', status: 'success', serviceId: 'svc-api', finishedAt: '2026-07-12T09:04:00.000Z' }),
      makeJob({ id: 'rollback', type: 'rollback', status: 'rolled_back', serviceId: 'svc-api', finishedAt: '2026-07-12T09:05:00.000Z' }),
    ]

    expect(selectRecentServiceUpdateJobs(jobs, 'svc-api').map((job) => job.id)).toEqual(['update-4', 'update-3', 'update-2'])
  })
})
