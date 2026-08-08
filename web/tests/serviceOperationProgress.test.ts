import { describe, expect, test } from 'bun:test'

import { describeServiceOperationProgress } from '../src/serviceOperationProgress'

describe('describeServiceOperationProgress', () => {
  test('maps update submission, queue, and execution phases', () => {
    expect(describeServiceOperationProgress({ updateSubmitting: true })).toMatchObject({
      kind: 'update',
      phase: 'submitting',
      bannerLabel: '更新任务提交中',
      compactLabel: '提交中',
    })
    expect(describeServiceOperationProgress({ updateSubmitting: false, updateStatus: 'queued' })).toMatchObject({
      kind: 'update',
      phase: 'queued',
      bannerLabel: '更新排队中',
      compactLabel: '排队中',
    })
    expect(describeServiceOperationProgress({ updateSubmitting: false, updateStatus: 'running' })).toMatchObject({
      kind: 'update',
      phase: 'running',
      bannerLabel: '更新中',
      compactLabel: '更新中',
    })
  })

  test('maps rollback queue and execution phases after update precedence', () => {
    expect(describeServiceOperationProgress({ updateSubmitting: false, rollbackStatus: 'queued' })).toMatchObject({
      kind: 'rollback',
      phase: 'queued',
      bannerLabel: '回滚排队中',
    })
    expect(describeServiceOperationProgress({ updateSubmitting: false, rollbackStatus: 'running' })).toMatchObject({
      kind: 'rollback',
      phase: 'running',
      bannerLabel: '回滚中',
    })
    expect(describeServiceOperationProgress({
      updateSubmitting: true,
      updateStatus: 'queued',
      rollbackStatus: 'running',
    })).toMatchObject({ kind: 'update', phase: 'queued' })
  })

  test('returns no progress without an active operation', () => {
    expect(describeServiceOperationProgress({ updateSubmitting: false })).toBeNull()
  })
})
