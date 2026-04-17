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
  actionableCount: number
  isFilteredSubset: boolean
  rows: ServiceRow[]
  visibleServices: StackDetail['services']
  previewItems: AggregateUpdatePreviewListItem[]
  actionableServices: StackDetail['services']
  counts: ReturnType<typeof partitionAggregateUpdateServices>['counts']
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
  const visiblePartition = partitionAggregateUpdateServices(visibleServices)
  const previewItems = [
    ...withDisplayName(visiblePartition.actionable, detail.name, detail.id),
    ...withDisplayName(visiblePartition.guardedDockrevPreview, detail.name, detail.id),
  ]

  return {
    totalServiceCount,
    visibleServiceCount: visibleServices.length,
    actionableCount: visiblePartition.actionable.length,
    isFilteredSubset: visibleServices.length !== totalServiceCount,
    rows,
    visibleServices,
    previewItems,
    actionableServices: visiblePartition.actionable.map((item) => item.svc),
    counts: visiblePartition.counts,
    guardedPreviewCount: visiblePartition.guardedDockrevPreview.length,
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
  let actionableCount = 0
  let guardedPreviewCount = 0
  const previewItems: AggregateUpdatePreviewListItem[] = []
  const actionableServices: StackDetail['services'] = []
  const counts = {
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
    actionableCount += scope.actionableCount
    guardedPreviewCount += scope.guardedPreviewCount
    counts.updatable += scope.counts.updatable
    counts.hint += scope.counts.hint
    counts.archMismatch += scope.counts.archMismatch
    counts.blocked += scope.counts.blocked
    previewItems.push(...scope.previewItems)
    actionableServices.push(...scope.actionableServices)
  }

  return {
    totalServiceCount,
    visibleServiceCount,
    actionableCount,
    isFilteredSubset: visibleServiceCount !== totalServiceCount,
    previewItems,
    actionableServices,
    counts,
    guardedPreviewCount,
  }
}
