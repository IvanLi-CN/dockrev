import type { StackDetail, StackListItem } from '../api'
import { partitionAggregateUpdateServices } from '../aggregateUpdateGuard'
import type { AggregateUpdatePreviewListItem } from '../components/AggregateUpdatePreviewList'
import type { UpdateCandidateFilter } from '../components/UpdateCandidateFilters'
import { serviceRowStatus, type RowStatus } from '../updateStatus'
import { matchesCandidateSearch } from './operationsDashboardShared'

type ServiceRow = {
  svc: StackDetail['services'][number]
  stt: RowStatus
}

export type AggregateScopeSummary = {
  totalServiceCount: number
  visibleServiceCount: number
  visibleActionableCount: number
  actualActionableCount: number
  hiddenActionableCount: number
  isFilteredSubset: boolean
  rows: ServiceRow[]
  visibleServices: StackDetail['services']
  actualPreviewItems: AggregateUpdatePreviewListItem[]
  actualActionableServices: StackDetail['services']
  actualCounts: ReturnType<typeof partitionAggregateUpdateServices>['counts']
  guardedPreviewCount: number
}

function withDisplayName(
  items: Array<Pick<AggregateUpdatePreviewListItem, 'svc' | 'status' | 'guardedDockrev'>>,
  stackName?: string,
  stackId?: string,
): AggregateUpdatePreviewListItem[] {
  return items.map((item) => ({
    ...item,
    displayName: stackName ? `${stackName}/${item.svc.name}` : item.svc.name,
    stackId,
  }))
}

function buildVisibleRows(
  detail: StackDetail,
  filter: UpdateCandidateFilter,
  candidateSearch: string,
): ServiceRow[] {
  return detail.services
    .filter((svc) => !svc.archived)
    .map((svc) => ({ svc, stt: serviceRowStatus(svc) }))
    .filter((row) => filter === 'all' || row.stt === filter)
    .filter((row) => matchesCandidateSearch(detail.name, row.svc, candidateSearch))
}

export function buildStackAggregateScope(
  detail: StackDetail,
  filter: UpdateCandidateFilter,
  candidateSearch: string,
): AggregateScopeSummary {
  const rows = buildVisibleRows(detail, filter, candidateSearch)
  const visibleServices = rows.map((row) => row.svc)
  const totalServiceCount = detail.services.filter((svc) => !svc.archived).length
  const actualPartition = partitionAggregateUpdateServices(detail.services)
  const visiblePartition = partitionAggregateUpdateServices(visibleServices)
  const actualPreviewItems = [
    ...withDisplayName(actualPartition.actionable, detail.name, detail.id),
    ...withDisplayName(actualPartition.guardedDockrevPreview, detail.name, detail.id),
  ]

  return {
    totalServiceCount,
    visibleServiceCount: visibleServices.length,
    visibleActionableCount: visiblePartition.actionable.length,
    actualActionableCount: actualPartition.actionable.length,
    hiddenActionableCount: Math.max(
      0,
      actualPartition.actionable.length - visiblePartition.actionable.length,
    ),
    isFilteredSubset: visibleServices.length !== totalServiceCount,
    rows,
    visibleServices,
    actualPreviewItems,
    actualActionableServices: actualPartition.actionable.map((item) => item.svc),
    actualCounts: actualPartition.counts,
    guardedPreviewCount: actualPartition.guardedDockrevPreview.length,
  }
}

export function buildAllAggregateScope(input: {
  stacks: StackListItem[]
  details: Record<string, StackDetail | undefined>
  filter: UpdateCandidateFilter
  candidateSearch: string
}): Omit<AggregateScopeSummary, 'rows' | 'visibleServices'> {
  let totalServiceCount = 0
  let visibleServiceCount = 0
  let visibleActionableCount = 0
  let actualActionableCount = 0
  let guardedPreviewCount = 0
  const actualPreviewItems: AggregateUpdatePreviewListItem[] = []
  const actualActionableServices: StackDetail['services'] = []
  const actualCounts = {
    updatable: 0,
    hint: 0,
    archMismatch: 0,
    blocked: 0,
  } satisfies ReturnType<typeof partitionAggregateUpdateServices>['counts']

  for (const stack of input.stacks) {
    const detail = input.details[stack.id]
    if (!detail) continue
    const scope = buildStackAggregateScope(
      detail,
      input.filter,
      input.candidateSearch,
    )
    totalServiceCount += scope.totalServiceCount
    visibleServiceCount += scope.visibleServiceCount
    visibleActionableCount += scope.visibleActionableCount
    actualActionableCount += scope.actualActionableCount
    guardedPreviewCount += scope.guardedPreviewCount
    actualCounts.updatable += scope.actualCounts.updatable
    actualCounts.hint += scope.actualCounts.hint
    actualCounts.archMismatch += scope.actualCounts.archMismatch
    actualCounts.blocked += scope.actualCounts.blocked
    actualPreviewItems.push(...scope.actualPreviewItems)
    actualActionableServices.push(...scope.actualActionableServices)
  }

  return {
    totalServiceCount,
    visibleServiceCount,
    visibleActionableCount,
    actualActionableCount,
    hiddenActionableCount: Math.max(0, actualActionableCount - visibleActionableCount),
    isFilteredSubset: visibleServiceCount !== totalServiceCount,
    actualPreviewItems,
    actualActionableServices,
    actualCounts,
    guardedPreviewCount,
  }
}
