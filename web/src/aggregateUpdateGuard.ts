import { ApiError, type Service } from './api'
import { isDockrevImageRef } from './runtimeConfig'
import { serviceRowStatus, type RowStatus } from './updateStatus'

export const DOCKREV_AGGREGATE_GUARD_HINT = 'Dockrev 需通过 supervisor 自升级；聚合更新会自动跳过它'
const HINT_ONLY_CANDIDATE_TEXT = '存在需确认的候选；将由服务端计算是否实际变更'
const NO_UPDATEABLE_SERVICE_TEXT = '无可更新服务'

export type AggregateUpdatePreviewItem = {
  svc: Service
  status: Extract<RowStatus, 'updatable' | 'hint'>
  guardedDockrev?: boolean
}

export type AggregateUpdateCounts = Record<Exclude<RowStatus, 'ok'>, number>

export type AggregateUpdatePartition = {
  actionable: AggregateUpdatePreviewItem[]
  guardedDockrevPreview: AggregateUpdatePreviewItem[]
  counts: AggregateUpdateCounts
}

export type AggregateUpdateActionState = {
  enabled: boolean
  title: string | null
  hint: string | null
}

export function emptyAggregateUpdateCounts(): AggregateUpdateCounts {
  return {
    updatable: 0,
    hint: 0,
    archMismatch: 0,
    blocked: 0,
  }
}

export function resolveAggregateUpdateActionState(
  partition: Pick<AggregateUpdatePartition, 'counts' | 'guardedDockrevPreview'>,
): AggregateUpdateActionState {
  if (partition.counts.updatable > 0) {
    return { enabled: true, title: null, hint: null }
  }
  if (partition.counts.hint > 0) {
    return { enabled: true, title: HINT_ONLY_CANDIDATE_TEXT, hint: null }
  }
  if (partition.guardedDockrevPreview.length > 0) {
    return {
      enabled: false,
      title: DOCKREV_AGGREGATE_GUARD_HINT,
      hint: DOCKREV_AGGREGATE_GUARD_HINT,
    }
  }
  return { enabled: false, title: NO_UPDATEABLE_SERVICE_TEXT, hint: null }
}

export function partitionAggregateUpdateServices(services: Service[]): AggregateUpdatePartition {
  const actionable: AggregateUpdatePreviewItem[] = []
  const guardedDockrevPreview: AggregateUpdatePreviewItem[] = []
  const counts = emptyAggregateUpdateCounts()

  for (const svc of services) {
    if (svc.archived) continue
    const status = serviceRowStatus(svc)
    if (status === 'ok') continue

    if (status === 'updatable' || status === 'hint') {
      const item: AggregateUpdatePreviewItem = { svc, status }
      if (isDockrevImageRef(svc.image.ref)) {
        guardedDockrevPreview.push({ ...item, guardedDockrev: true })
        continue
      }
      actionable.push(item)
    }

    counts[status] += 1
  }

  return {
    actionable,
    guardedDockrevPreview,
    counts,
  }
}

export function readUpdateGuardBlockedReason(error: unknown): string | null {
  if (!(error instanceof ApiError)) {
    return null
  }
  return null
}
