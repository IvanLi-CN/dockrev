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
  progressMode: OverviewJobProgressMode
  progressPercent: number | null
}

const IN_FLIGHT_STATUSES = new Set(['queued', 'running'])

export type OverviewJobProgressMode = 'none' | 'determinate' | 'indeterminate'

type OverviewJobProgressVisual = {
  progressMode: OverviewJobProgressMode
  progressPercent: number | null
}

function compareJobsByCreatedAtDesc(lhs: JobListItem, rhs: JobListItem): number {
  const tsCmp = String(rhs.createdAt ?? '').localeCompare(String(lhs.createdAt ?? ''))
  if (tsCmp !== 0) return tsCmp
  return rhs.id.localeCompare(lhs.id)
}

function clampPercent(input: number): number {
  return Math.max(0, Math.min(100, Math.round(input)))
}

export function getOverviewJobProgressVisual(job: JobListItem): OverviewJobProgressVisual {
  if (job.status !== 'running') return { progressMode: 'none', progressPercent: null }
  const p = job.progress
  if (!p) return { progressMode: 'indeterminate', progressPercent: null }
  const total = Number.isFinite(p.total) ? Math.max(0, p.total) : 0
  if (total <= 0 || !Number.isFinite(p.percent)) return { progressMode: 'indeterminate', progressPercent: null }

  const percent = clampPercent(p.percent)
  const currentRaw = Number.isFinite(p.current) ? Math.max(0, p.current) : 0
  const current = Math.min(currentRaw, total || currentRaw)
  if (percent === 0 && current < total) return { progressMode: 'indeterminate', progressPercent: null }
  return { progressMode: 'determinate', progressPercent: percent }
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
  const visual = getOverviewJobProgressVisual(job)
  return {
    jobId: job.id,
    status: job.status,
    createdAt: job.createdAt,
    createdBy: job.createdBy,
    reason: job.reason,
    primaryLabel: readable.primaryLabel,
    scopeTag: readable.scopeTag,
    typeTone: readable.typeTone,
    progressMode: visual.progressMode,
    progressPercent: visual.progressPercent,
  }
}
