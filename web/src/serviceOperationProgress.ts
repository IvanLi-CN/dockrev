export type ServiceOperationProgress = {
  kind: 'update' | 'rollback'
  phase: 'submitting' | 'queued' | 'running'
  bannerLabel: string
  compactLabel: string
  actionLabel: string
}

export function describeServiceOperationProgress(input: {
  updateSubmitting: boolean
  updateStatus?: string | null
  rollbackStatus?: string | null
}): ServiceOperationProgress | null {
  if (input.updateSubmitting && !input.updateStatus) {
    return {
      kind: 'update',
      phase: 'submitting',
      bannerLabel: '更新任务提交中',
      compactLabel: '提交中',
      actionLabel: '提交中…',
    }
  }
  if (input.updateStatus) {
    const queued = input.updateStatus === 'queued'
    return {
      kind: 'update',
      phase: queued ? 'queued' : 'running',
      bannerLabel: queued ? '更新排队中' : '更新中',
      compactLabel: queued ? '排队中' : '更新中',
      actionLabel: queued ? '排队中…' : '更新中…',
    }
  }
  if (input.rollbackStatus) {
    const queued = input.rollbackStatus === 'queued'
    return {
      kind: 'rollback',
      phase: queued ? 'queued' : 'running',
      bannerLabel: queued ? '回滚排队中' : '回滚中',
      compactLabel: queued ? '排队中' : '回滚中',
      actionLabel: queued ? '回滚排队中…' : '回滚中…',
    }
  }
  return null
}
