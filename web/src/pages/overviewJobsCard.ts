import type { JobListItem } from '../api'
import { formatJobReadableDisplay, type JobTypeTone } from '../jobDisplay'

export type OverviewJobsCardOptions = {
  maxItems?: number
}

export type OverviewJobCardItem = {
  jobId: string
  status: string
  createdAt: string
  createdBy: string
  reason: string
  primaryLabel: string
  scopeTag: string | null
  typeTone: JobTypeTone
}

const IN_FLIGHT_STATUSES = new Set(['queued', 'running'])

function compareJobsByCreatedAtDesc(lhs: JobListItem, rhs: JobListItem): number {
  const tsCmp = String(rhs.createdAt ?? '').localeCompare(String(lhs.createdAt ?? ''))
  if (tsCmp !== 0) return tsCmp
  return rhs.id.localeCompare(lhs.id)
}

export function selectOverviewJobsForCard(jobs: JobListItem[], options?: OverviewJobsCardOptions): JobListItem[] {
  const maxItemsRaw = options?.maxItems ?? 10
  const maxItems = Math.max(0, Math.floor(maxItemsRaw))
  if (maxItems === 0 || jobs.length === 0) return []

  const sorted = [...jobs].sort(compareJobsByCreatedAtDesc)

  const selected: JobListItem[] = []
  const selectedIds = new Set<string>()

  for (const job of sorted) {
    if (!IN_FLIGHT_STATUSES.has(job.status)) continue
    selected.push(job)
    selectedIds.add(job.id)
    if (selected.length >= maxItems) return selected
  }

  for (const job of sorted) {
    if (selectedIds.has(job.id)) continue
    selected.push(job)
    selectedIds.add(job.id)
    if (selected.length >= maxItems) break
  }

  return selected
}

export function toOverviewJobCardItem(job: JobListItem): OverviewJobCardItem {
  const readable = formatJobReadableDisplay(job.type, job.scope)
  return {
    jobId: job.id,
    status: job.status,
    createdAt: job.createdAt,
    createdBy: job.createdBy,
    reason: job.reason,
    primaryLabel: readable.primaryLabel,
    scopeTag: readable.scopeTag,
    typeTone: readable.typeTone,
  }
}
