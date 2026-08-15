import { describe, expect, test } from 'bun:test'

import type { JobListItem } from '../src/api'
import type { ManagementEvent } from '../src/managementEvents'
import {
  doesManagementEventInvalidateUpdateSnapshot,
  pickLatestActiveUpdateJobs,
  isUpdateJobSnapshotCurrent,
  reconcileTrackedUpdateJobs,
  resolveActiveUpdateJobStatus,
  resolveTrackedUpdateJobTransition,
} from '../src/updateActionTracking'

function makeJob(overrides: Partial<JobListItem> & Pick<JobListItem, 'id'>): JobListItem {
  return {
    id: overrides.id,
    type: 'update',
    scope: 'service',
    stackId: 'stack-1',
    serviceId: 'svc-1',
    status: 'running',
    createdBy: 'ivan',
    reason: 'ui',
    createdAt: '2026-03-30T10:00:00.000Z',
    startedAt: '2026-03-30T10:01:00.000Z',
    finishedAt: null,
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary: {},
    progress: null,
    ...overrides,
  }
}

function sortHydratedJobs(jobs: ReturnType<typeof pickLatestActiveUpdateJobs>) {
  return [...jobs].sort((a, b) => a.target.localeCompare(b.target) || a.jobId.localeCompare(b.jobId))
}

describe('pickLatestActiveUpdateJobs', () => {
  test('keeps only active update jobs with resolvable targets', () => {
    const jobs = [
      makeJob({ id: 'job-service-running' }),
      makeJob({ id: 'job-stack-queued', scope: 'stack', stackId: 'stack-2', serviceId: null, status: 'queued' }),
      makeJob({ id: 'job-all-running', scope: 'all', stackId: null, serviceId: null }),
      makeJob({ id: 'job-terminal', status: 'success' }),
      makeJob({ id: 'job-check', type: 'check' }),
      makeJob({ id: 'job-invalid-scope', scope: 'service', serviceId: null }),
    ]

    expect(sortHydratedJobs(pickLatestActiveUpdateJobs(jobs))).toEqual([
      { target: 'all', jobId: 'job-all-running', status: 'running' },
      { target: 'service:svc-1', jobId: 'job-service-running', status: 'running' },
      { target: 'stack:stack-2', jobId: 'job-stack-queued', status: 'queued' },
    ])
  })

  test('keeps the newest active job for the same target', () => {
    const jobs = [
      makeJob({
        id: 'job-old',
        status: 'running',
        createdAt: '2026-03-30T10:00:00.000Z',
        startedAt: '2026-03-30T10:01:00.000Z',
      }),
      makeJob({
        id: 'job-new',
        status: 'queued',
        createdAt: '2026-03-30T10:05:00.000Z',
        startedAt: null,
      }),
    ]

    expect(pickLatestActiveUpdateJobs(jobs)).toEqual([
      { target: 'service:svc-1', jobId: 'job-new', status: 'queued' },
    ])
  })

  test('falls back to progress updatedAt when other timestamps are unusable', () => {
    const jobs = [
      makeJob({
        id: 'job-invalid-created-at',
        createdAt: 'invalid-date',
        startedAt: null,
        progress: {
          phase: 'apply',
          message: 'running',
          current: 1,
          total: 3,
          percent: 33,
          updatedAt: '2026-03-30T10:03:00.000Z',
        },
      }),
      makeJob({
        id: 'job-fallback-newer',
        createdAt: 'also-invalid',
        startedAt: null,
        progress: {
          phase: 'apply',
          message: 'still running',
          current: 2,
          total: 3,
          percent: 66,
          updatedAt: '2026-03-30T10:04:00.000Z',
        },
      }),
    ]

    expect(pickLatestActiveUpdateJobs(jobs)).toEqual([
      { target: 'service:svc-1', jobId: 'job-fallback-newer', status: 'running' },
    ])
  })
})

describe('reconcileTrackedUpdateJobs', () => {
  test('settles tracked jobs that are terminal in the REST snapshot', () => {
    const result = reconcileTrackedUpdateJobs(
      [['service:svc-1', { jobId: 'job-finished', status: 'running' }]],
      [makeJob({ id: 'job-finished', status: 'success', summary: { targetTag: 'v2' } })],
    )

    expect(result.active).toEqual([])
    expect(result.settled).toEqual([
      {
        target: 'service:svc-1',
        job: expect.objectContaining({ id: 'job-finished', status: 'success', summary: { targetTag: 'v2' } }),
      },
    ])
    expect(result.unresolved).toEqual([])
  })

  test('keeps an omitted tracked job for detail confirmation while hydrating a newer active job', () => {
    const result = reconcileTrackedUpdateJobs(
      [['service:svc-1', { jobId: 'job-old', status: 'running' }]],
      [makeJob({ id: 'job-new', createdAt: '2026-03-30T10:05:00.000Z' })],
    )

    expect(result.active).toEqual([{ target: 'service:svc-1', jobId: 'job-new', status: 'running' }])
    expect(result.settled).toEqual([])
    expect(result.unresolved).toEqual([{ target: 'service:svc-1', jobId: 'job-old' }])
  })
})

describe('resolveTrackedUpdateJobTransition', () => {
  test('advances a tracked update from queued to running', () => {
    const event: ManagementEvent = {
      type: 'entities_changed',
      domain: 'jobs',
      entities: [{ entityType: 'job', id: 'job-update' }],
      version: 2,
      summary: {
        jobId: 'job-update',
        status: 'running',
        jobType: 'update',
        scope: 'service',
        stackId: 'stack-1',
        serviceId: 'svc-1',
      },
    }

    expect(resolveTrackedUpdateJobTransition(
      event,
      [['service:svc-1', { jobId: 'job-update', status: 'queued', targetVersion: 'v2' }]],
    )).toEqual({ target: 'service:svc-1', jobId: 'job-update', status: 'running' })
  })

  test('ignores progress-only events and unrelated jobs', () => {
    const progressEvent: ManagementEvent = {
      type: 'entities_changed',
      domain: 'jobs',
      entities: [{ entityType: 'job', id: 'job-update' }],
      version: 3,
      summary: { jobId: 'job-update', jobType: 'update', status: 'running', operation: 'progress_updated' },
    }

    expect(resolveTrackedUpdateJobTransition(
      progressEvent,
      [['service:svc-1', { jobId: 'job-update', status: 'queued' }]],
    )).toBeNull()
    expect(resolveTrackedUpdateJobTransition(
      { ...progressEvent, summary: { jobId: 'job-other', status: 'running' } },
      [['service:svc-1', { jobId: 'job-update', status: 'queued' }]],
    )).toBeNull()
  })
})

describe('resolveActiveUpdateJobStatus', () => {
  test('does not let a delayed queued snapshot regress an observed running event', () => {
    expect(resolveActiveUpdateJobStatus('running', 'queued')).toBe('running')
  })

  test('allows queued jobs to advance to running', () => {
    expect(resolveActiveUpdateJobStatus('queued', 'running')).toBe('running')
  })
})

describe('isUpdateJobSnapshotCurrent', () => {
  test('rejects a snapshot when a terminal event mutates tracking while the request is in flight', () => {
    const requestRevision = 4
    const revisionAfterTerminalEvent = 5

    expect(isUpdateJobSnapshotCurrent(requestRevision, revisionAfterTerminalEvent)).toBeFalse()
  })

  test('accepts a snapshot when tracking has not changed', () => {
    expect(isUpdateJobSnapshotCurrent(4, 4)).toBeTrue()
  })
})

describe('doesManagementEventInvalidateUpdateSnapshot', () => {
  test('invalidates initial hydration for a terminal event before the job is tracked', () => {
    const terminalEvent: ManagementEvent = {
      type: 'entities_changed',
      domain: 'jobs',
      entities: [{ entityType: 'job', id: 'job-update' }],
      version: 3,
      summary: { jobId: 'job-update', jobType: 'update', status: 'success', terminal: true },
    }

    expect(doesManagementEventInvalidateUpdateSnapshot(terminalEvent)).toBeTrue()
  })

  test('does not invalidate hydration for progress-only or unrelated events', () => {
    const progressEvent: ManagementEvent = {
      type: 'entities_changed',
      domain: 'jobs',
      entities: [{ entityType: 'job', id: 'job-update' }],
      version: 2,
      summary: { jobId: 'job-update', operation: 'progress_updated' },
    }

    expect(doesManagementEventInvalidateUpdateSnapshot(progressEvent)).toBeFalse()
    expect(doesManagementEventInvalidateUpdateSnapshot({ ...progressEvent, domain: 'services' })).toBeFalse()
    expect(doesManagementEventInvalidateUpdateSnapshot({
      ...progressEvent,
      summary: { jobId: 'job-check', jobType: 'check', status: 'running' },
    })).toBeFalse()
  })
})
