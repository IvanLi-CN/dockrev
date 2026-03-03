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

const TERMINAL_STATUSES = new Set(['success', 'failed', 'rolled_back'])
const BASE_VISIBLE_ITEMS = 5
const MAX_NON_TERMINAL_ITEMS = 10

function compareJobsByCreatedAtDesc(lhs: JobListItem, rhs: JobListItem): number {
  const tsCmp = String(rhs.createdAt ?? '').localeCompare(String(lhs.createdAt ?? ''))
  if (tsCmp !== 0) return tsCmp
  return rhs.id.localeCompare(lhs.id)
}

export function selectOverviewJobsForCard(jobs: JobListItem[], options?: OverviewJobsCardOptions): JobListItem[] {
  const maxItemsRaw = options?.maxItems ?? MAX_NON_TERMINAL_ITEMS
  const maxItems = Math.max(0, Math.floor(maxItemsRaw))
  if (maxItems === 0 || jobs.length === 0) return []

  const sorted = [...jobs].sort(compareJobsByCreatedAtDesc)
  const nonTerminalJobs = sorted.filter((job) => !TERMINAL_STATUSES.has(job.status))
  const terminalJobs = sorted.filter((job) => TERMINAL_STATUSES.has(job.status))

  const baseVisibleItems = Math.min(BASE_VISIBLE_ITEMS, maxItems)
  const maxNonTerminalItems = Math.min(MAX_NON_TERMINAL_ITEMS, maxItems)

  if (nonTerminalJobs.length === 0) return terminalJobs.slice(0, baseVisibleItems)
  if (nonTerminalJobs.length > BASE_VISIBLE_ITEMS) return nonTerminalJobs.slice(0, maxNonTerminalItems)

  const nonTerminalSelected = nonTerminalJobs.slice(0, baseVisibleItems)
  const terminalFillCount = Math.max(0, baseVisibleItems - nonTerminalSelected.length)
  return [...nonTerminalSelected, ...terminalJobs.slice(0, terminalFillCount)]
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
