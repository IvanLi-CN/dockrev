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

const TERMINAL_STATUSES = new Set(['success', 'failed', 'rolled_back'])
const BASE_VISIBLE_ITEMS = 5
const MAX_NON_TERMINAL_ITEMS = 10

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
