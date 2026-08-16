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
  test('shows only latest 5 terminal jobs when no non-terminal jobs', () => {
    const terminal = [
      makeJob({ id: 'success-1', status: 'success', createdAt: '2026-03-03T10:15:00.000Z' }),
      makeJob({ id: 'failed-2', status: 'failed', createdAt: '2026-03-03T10:14:00.000Z' }),
      makeJob({ id: 'rolled-3', status: 'rolled_back', createdAt: '2026-03-03T10:13:00.000Z' }),
      makeJob({ id: 'success-4', status: 'success', createdAt: '2026-03-03T10:12:00.000Z' }),
      makeJob({ id: 'failed-5', status: 'failed', createdAt: '2026-03-03T10:11:00.000Z' }),
      makeJob({ id: 'success-6', status: 'success', createdAt: '2026-03-03T10:10:00.000Z' }),
    ]

    const selected = selectOverviewJobsForCard(terminal, { maxItems: 10 })

    expect(selected).toHaveLength(5)
    expect(selected.every((job) => ['success', 'failed', 'rolled_back'].includes(job.status))).toBe(true)
    expect(selected.map((job) => job.id)).toEqual(['success-1', 'failed-2', 'rolled-3', 'success-4', 'failed-5'])
  })

  test('fills to 5 with terminal jobs when non-terminal count is 1..4', () => {
    const jobs = [
      makeJob({ id: 'paused-1', status: 'paused', createdAt: '2026-03-03T10:12:00.000Z' }),
      makeJob({ id: 'running-2', status: 'running', createdAt: '2026-03-03T10:11:00.000Z' }),
      makeJob({ id: 'queued-3', status: 'queued', createdAt: '2026-03-03T10:10:00.000Z' }),
      makeJob({ id: 'success-4', status: 'success', createdAt: '2026-03-03T10:09:00.000Z' }),
      makeJob({ id: 'failed-5', status: 'failed', createdAt: '2026-03-03T10:08:00.000Z' }),
      makeJob({ id: 'rolled-6', status: 'rolled_back', createdAt: '2026-03-03T10:07:00.000Z' }),
    ]

    const selected = selectOverviewJobsForCard(jobs, { maxItems: 10 })

    expect(selected.map((job) => `${job.id}:${job.status}`)).toEqual([
      'paused-1:paused',
      'running-2:running',
      'queued-3:queued',
      'success-4:success',
      'failed-5:failed',
    ])
  })

  test('shows exactly 5 non-terminal jobs when non-terminal count is 5', () => {
    const jobs = [
      makeJob({ id: 'queued-1', status: 'queued', createdAt: '2026-03-03T10:15:00.000Z' }),
      makeJob({ id: 'running-2', status: 'running', createdAt: '2026-03-03T10:14:00.000Z' }),
      makeJob({ id: 'retrying-3', status: 'retrying', createdAt: '2026-03-03T10:13:00.000Z' }),
      makeJob({ id: 'pending-4', status: 'pending', createdAt: '2026-03-03T10:12:00.000Z' }),
      makeJob({ id: 'starting-5', status: 'starting', createdAt: '2026-03-03T10:11:00.000Z' }),
      makeJob({ id: 'success-6', status: 'success', createdAt: '2026-03-03T10:16:00.000Z' }),
      makeJob({ id: 'failed-7', status: 'failed', createdAt: '2026-03-03T10:10:00.000Z' }),
    ]

    const selected = selectOverviewJobsForCard(jobs, { maxItems: 10 })

    expect(selected).toHaveLength(5)
    expect(selected.every((job) => !['success', 'failed', 'rolled_back'].includes(job.status))).toBe(true)
    expect(selected.map((job) => job.id)).toEqual(['queued-1', 'running-2', 'retrying-3', 'pending-4', 'starting-5'])
  })

  test('shows all non-terminal jobs when non-terminal count is in 6..10', () => {
    const nonTerminal = Array.from({ length: 7 }, (_, idx) =>
      makeJob({
        id: `active-${idx + 1}`,
        status: idx % 2 === 0 ? 'running' : 'queued',
        createdAt: `2026-03-03T10:${String(20 - idx).padStart(2, '0')}:00.000Z`,
      }),
    )
    const terminals = [
      makeJob({ id: 'success-1', status: 'success', createdAt: '2026-03-03T10:30:00.000Z' }),
      makeJob({ id: 'failed-2', status: 'failed', createdAt: '2026-03-03T10:29:00.000Z' }),
    ]

    const selected = selectOverviewJobsForCard([...nonTerminal, ...terminals], { maxItems: 10 })

    expect(selected).toHaveLength(7)
    expect(selected.every((job) => !['success', 'failed', 'rolled_back'].includes(job.status))).toBe(true)
    expect(selected.map((job) => job.id)).toEqual(nonTerminal.map((job) => job.id))
  })

  test('shows latest 10 non-terminal jobs when non-terminal count is greater than 10', () => {
    const nonTerminal = Array.from({ length: 12 }, (_, idx) =>
      makeJob({
        id: `active-${idx + 1}`,
        status: idx % 3 === 0 ? 'running' : idx % 3 === 1 ? 'queued' : 'pending',
        createdAt: `2026-03-03T10:${String(59 - idx).padStart(2, '0')}:00.000Z`,
      }),
    )
    const terminals = [
      makeJob({ id: 'success-1', status: 'success', createdAt: '2026-03-03T11:59:00.000Z' }),
      makeJob({ id: 'failed-2', status: 'failed', createdAt: '2026-03-03T11:58:00.000Z' }),
    ]

    const selected = selectOverviewJobsForCard([...nonTerminal, ...terminals], { maxItems: 10 })

    expect(selected).toHaveLength(10)
    expect(selected.every((job) => !['success', 'failed', 'rolled_back'].includes(job.status))).toBe(true)
    expect(selected.map((job) => job.id)).toEqual(nonTerminal.slice(0, 10).map((job) => job.id))
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

    const selected = selectOverviewJobsForCard(jobs, { maxItems: 10 })

    expect(selected.map((job) => job.id)).toEqual(['queued-c', 'queued-b', 'queued-a', 'success-c', 'success-b'])
  })

  test('returns empty list when maxItems is zero', () => {
    const jobs = [makeJob({ id: 'queued-a', status: 'queued', createdAt: '2026-03-03T10:00:00.000Z' })]

    const selected = selectOverviewJobsForCard(jobs, { maxItems: 0 })

    expect(selected).toEqual([])
  })
})

describe('toOverviewJobCardItem progress visual mapping', () => {
  test('keeps the localized label when compact fallback is the raw job type', () => {
    const job = {
      ...makeJob({ id: 'global-discovery', type: 'discovery', scope: 'all', status: 'success', createdAt: '2026-03-03T10:00:00.000Z' }),
      displayLabel: 'discovery',
      targetVersion: null,
    }

    expect(toOverviewJobCardItem(job).primaryLabel).toBe('发现扫描')
  })

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
