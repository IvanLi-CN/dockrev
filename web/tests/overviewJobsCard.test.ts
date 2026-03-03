import { describe, expect, test } from 'bun:test'

import type { JobListItem, JobProgress } from '../src/api'
import { selectOverviewJobsForCard, toOverviewJobCardItem } from '../src/pages/overviewJobsCard'

function makeJob(input: {
  id: string
  status: string
  createdAt: string
  type?: string
  scope?: string
  progress?: JobProgress | null
}): JobListItem {
  return {
    id: input.id,
    type: input.type ?? 'update',
    scope: input.scope ?? 'service',
    stackId: 'stack-a',
    serviceId: 'svc-a',
    status: input.status,
    createdBy: 'ivan',
    reason: 'ui',
    createdAt: input.createdAt,
    startedAt: null,
    finishedAt: null,
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary: {},
    progress: input.progress ?? null,
  }
}

describe('selectOverviewJobsForCard', () => {
  test('shows only latest 10 in-flight jobs when queued/running >= 10', () => {
    const inFlight = Array.from({ length: 12 }, (_, idx) =>
      makeJob({
        id: `inflight-${idx}`,
        status: idx % 2 === 0 ? 'queued' : 'running',
        createdAt: `2026-03-03T10:${String(59 - idx).padStart(2, '0')}:00.000Z`,
      }),
    )
    const fallback = [
      makeJob({ id: 'fallback-new', status: 'success', createdAt: '2026-03-03T11:59:00.000Z' }),
      makeJob({ id: 'fallback-old', status: 'failed', createdAt: '2026-03-03T09:59:00.000Z' }),
    ]

    const selected = selectOverviewJobsForCard([...inFlight, ...fallback], { maxItems: 10 })

    expect(selected).toHaveLength(10)
    expect(selected.every((job) => job.status === 'queued' || job.status === 'running')).toBe(true)
    expect(selected.map((job) => job.id)).toEqual(inFlight.slice(0, 10).map((job) => job.id))
  })

  test('fills with fallback jobs when in-flight jobs are not enough', () => {
    const jobs = [
      makeJob({ id: 'run-2', status: 'running', createdAt: '2026-03-03T10:12:00.000Z' }),
      makeJob({ id: 'success-3', status: 'success', createdAt: '2026-03-03T10:10:00.000Z' }),
      makeJob({ id: 'queue-1', status: 'queued', createdAt: '2026-03-03T10:11:00.000Z' }),
      makeJob({ id: 'failed-4', status: 'failed', createdAt: '2026-03-03T10:09:00.000Z' }),
      makeJob({ id: 'rolled-5', status: 'rolled_back', createdAt: '2026-03-03T10:08:00.000Z' }),
    ]

    const selected = selectOverviewJobsForCard(jobs, { maxItems: 4 })

    expect(selected.map((job) => `${job.id}:${job.status}`)).toEqual([
      'run-2:running',
      'queue-1:queued',
      'success-3:success',
      'failed-4:failed',
    ])
  })

  test('returns all available jobs when total count is smaller than max limit', () => {
    const jobs = [
      makeJob({ id: 'queued-a', status: 'queued', createdAt: '2026-03-03T10:03:00.000Z' }),
      makeJob({ id: 'success-b', status: 'success', createdAt: '2026-03-03T10:02:00.000Z' }),
      makeJob({ id: 'running-c', status: 'running', createdAt: '2026-03-03T10:01:00.000Z' }),
    ]

    const selected = selectOverviewJobsForCard(jobs, { maxItems: 10 })

    expect(selected.map((job) => job.id)).toEqual(['queued-a', 'running-c', 'success-b'])
  })

  test('uses id descending as tie-breaker when createdAt is equal', () => {
    const createdAt = '2026-03-03T10:00:00.000Z'
    const jobs = [
      makeJob({ id: 'queued-a', status: 'queued', createdAt }),
      makeJob({ id: 'queued-c', status: 'queued', createdAt }),
      makeJob({ id: 'queued-b', status: 'queued', createdAt }),
      makeJob({ id: 'success-a', status: 'success', createdAt }),
      makeJob({ id: 'success-c', status: 'success', createdAt }),
      makeJob({ id: 'success-b', status: 'success', createdAt }),
    ]

    const selected = selectOverviewJobsForCard(jobs, { maxItems: 6 })

    expect(selected.map((job) => job.id)).toEqual([
      'queued-c',
      'queued-b',
      'queued-a',
      'success-c',
      'success-b',
      'success-a',
    ])
  })

  test('returns empty list when maxItems is zero', () => {
    const jobs = [makeJob({ id: 'queued-a', status: 'queued', createdAt: '2026-03-03T10:00:00.000Z' })]

    const selected = selectOverviewJobsForCard(jobs, { maxItems: 0 })

    expect(selected).toEqual([])
  })
})

describe('toOverviewJobCardItem progress visual mapping', () => {
  test('maps running jobs with determinate progress', () => {
    const job = makeJob({
      id: 'running-determinate',
      status: 'running',
      createdAt: '2026-03-03T10:00:00.000Z',
      progress: {
        phase: 'apply',
        message: 'updating',
        current: 2,
        total: 5,
        percent: 40,
        plannedCurrent: 4,
        plannedTotal: 5,
        plannedPercent: 80,
        currentTarget: 'api',
        updatedAt: '2026-03-03T10:00:10.000Z',
      },
    })

    const mapped = toOverviewJobCardItem(job)

    expect(mapped.progressMode).toBe('determinate')
    expect(mapped.progressPercent).toBe(40)
  })

  test('maps running jobs without progress as indeterminate', () => {
    const job = makeJob({
      id: 'running-no-progress',
      status: 'running',
      createdAt: '2026-03-03T10:00:00.000Z',
      progress: null,
    })

    const mapped = toOverviewJobCardItem(job)

    expect(mapped.progressMode).toBe('indeterminate')
    expect(mapped.progressPercent).toBeNull()
  })

  test('treats running zero-percent while unfinished as indeterminate', () => {
    const job = makeJob({
      id: 'running-zero-unfinished',
      status: 'running',
      createdAt: '2026-03-03T10:00:00.000Z',
      progress: {
        phase: 'apply',
        message: 'starting',
        current: 0,
        total: 6,
        percent: 0,
        plannedCurrent: 0,
        plannedTotal: 6,
        plannedPercent: 0,
        currentTarget: 'worker',
        updatedAt: '2026-03-03T10:00:10.000Z',
      },
    })

    const mapped = toOverviewJobCardItem(job)

    expect(mapped.progressMode).toBe('indeterminate')
    expect(mapped.progressPercent).toBeNull()
  })

  test('clamps determinate percent into 0..100', () => {
    const job = makeJob({
      id: 'running-over-100',
      status: 'running',
      createdAt: '2026-03-03T10:00:00.000Z',
      progress: {
        phase: 'done',
        message: 'finished',
        current: 4,
        total: 4,
        percent: 143,
        plannedCurrent: 4,
        plannedTotal: 4,
        plannedPercent: 143,
        currentTarget: null,
        updatedAt: '2026-03-03T10:00:10.000Z',
      },
    })

    const mapped = toOverviewJobCardItem(job)

    expect(mapped.progressMode).toBe('determinate')
    expect(mapped.progressPercent).toBe(100)
  })

  test('maps non-running jobs to no progress background', () => {
    const job = makeJob({
      id: 'success-1',
      status: 'success',
      createdAt: '2026-03-03T10:00:00.000Z',
      progress: {
        phase: 'done',
        message: 'finished',
        current: 6,
        total: 6,
        percent: 100,
        plannedCurrent: 6,
        plannedTotal: 6,
        plannedPercent: 100,
        currentTarget: null,
        updatedAt: '2026-03-03T10:00:10.000Z',
      },
    })

    const mapped = toOverviewJobCardItem(job)

    expect(mapped.progressMode).toBe('none')
    expect(mapped.progressPercent).toBeNull()
  })
})
