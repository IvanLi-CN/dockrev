import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent, type ReactNode } from 'react'
import {
  triggerDiscoveryScan,
  listDiscoveryProjects,
  listJobs,
  getJob,
  getStack,
  listStacks,
  triggerCheck,
  triggerRuntimeScan,
  triggerUpdate,
  newJobEventsSource,
  newJobsEventsSource,
  ApiError,
  type DiscoveredProject,
  type JobListItem,
  type Service,
  type TriggerUpdateInput,
  type ServiceDigestTagsScanSummary,
  type StackDetail,
  type StackListItem,
} from '../api'
import { navigate } from '../routes'
import { buildUpdateServiceTarget, buildUpdateServiceTargets } from '../updateTargets'
import { ArrowRightIcon, Button, Mono, Pill, StatusRemark } from '../ui'
import { isDockrevImageRef, selfUpgradeBaseUrl } from '../runtimeConfig'
import { useSupervisorHealth } from '../useSupervisorHealth'
import {
  DOCKREV_AGGREGATE_GUARD_HINT,
  emptyAggregateUpdateCounts,
  partitionAggregateUpdateServices,
  resolveAggregateUpdateActionState,
} from '../aggregateUpdateGuard'
import { isSemverDowngradeAnomaly, serviceRowStatus, type RowStatus } from '../updateStatus'
import { selectOverviewJobsForCard, toOverviewJobCardItem } from './overviewJobsCard'
import { UpdateCandidateFilters, type UpdateCandidateFilter } from '../components/UpdateCandidateFilters'
import { useConfirm } from '../confirm'
import { VersionTagsPopover } from '../components/VersionTagsPopover'
import { CurrentVersionPopover } from '../components/CurrentVersionPopover'
import { AggregateUpdatePreviewList, type AggregateUpdatePreviewListItem } from '../components/AggregateUpdatePreviewList'
import { ConfirmServiceVersionCell } from '../components/ConfirmServiceVersionCell'
import { ImageLinkIcons, splitImageNameForDisplay, splitImageRef } from '../imageLinks'
import {
  formatCandidateTagDisplay,
  formatCurrentTagDisplay as formatTagDisplay,
  inferResolvedTagsFromSnapshot,
  isStrictSemverTag,
} from '../versionDisplay'
import { normalizeDigest } from '../components/digest'
import {
  DIGEST_SNAPSHOT_UPDATED_EVENT,
  type DigestSnapshotUpdatedDetail,
} from '../digestInferenceTracker'
import { Tooltip, TooltipContent, TooltipTrigger } from '../components/ui/tooltip'
import { imageRepoFromImageRef } from '../imageRepo'
import {
  resolveUpdateActionTargetKey,
  UPDATE_JOB_SETTLED_EVENT,
  UPDATE_JOB_SETTLE_RETRY_MS,
  type UpdateJobSettledDetail,
  useUpdateActionTracker,
} from '../updateActionTracking'
import { usePageResumeRefresh } from '../usePageResumeRefresh'

function formatShort(ts?: string | null) {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function formatCompactDateTime(ts?: string | null) {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  const h = String(d.getHours()).padStart(2, '0')
  const min = String(d.getMinutes()).padStart(2, '0')
  return `${m}/${day} ${h}:${min}`
}

function scanHasFailures(scan: ServiceDigestTagsScanSummary | null | undefined): boolean {
  if (!scan) return false
  return scan.manifestsTimeout > 0 || scan.manifestsError > 0
}

function scanIsComplete(scan: ServiceDigestTagsScanSummary | null | undefined): boolean {
  if (!scan) return false
  return scan.repoTagsConsidered >= scan.repoTagsTotal
}

function getDiscoveryScanStartedAt(summary: unknown): string | null {
  if (typeof summary !== 'object' || summary === null) return null
  const scan = (summary as Record<string, unknown>).scan
  if (typeof scan !== 'object' || scan === null) return null
  const startedAt = (scan as Record<string, unknown>).startedAt
  return typeof startedAt === 'string' ? startedAt : null
}

function isDockrevService(svc: Service): boolean {
  return isDockrevImageRef(svc.image.ref)
}

function shouldPrefetchFloatingCandidate(
  candidateTag: string | null | undefined,
  candidateResolvedTag: string | null | undefined,
  candidateDigest: string | null | undefined,
): boolean {
  const raw = (candidateTag ?? '').trim()
  if (raw === '-') return false
  if (!raw || isStrictSemverTag(raw)) return false
  if (isStrictSemverTag(candidateResolvedTag)) return false
  return (candidateDigest ?? '').trim().length > 0
}

function StackIcon(props: { variant: 'collapsed' | 'expanded' }) {
  return (
    <svg className="stackIcon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {props.variant === 'expanded' ? (
        <path d="m5 19l2.757-7.351A1 1 0 0 1 8.693 11H21a1 1 0 0 1 .986 1.164l-.996 5.211A2 2 0 0 1 19.026 19za2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h4l3 3h7a2 2 0 0 1 2 2v2" />
      ) : (
        <path d="M5 4h4l3 3h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2" />
      )}
    </svg>
  )
}

function formatGroupSummary(services: number, counts: Record<Exclude<RowStatus, 'ok'>, number>) {
  const parts: string[] = [`${services} services`]
  if (counts.updatable > 0) parts.push(`${counts.updatable} 可更新`)
  if (counts.hint > 0) parts.push(`${counts.hint} 需确认`)
  if (counts.archMismatch > 0) parts.push(`${counts.archMismatch} 架构不匹配`)
  if (counts.blocked > 0) parts.push(`${counts.blocked} 被阻止`)
  return parts.join(' · ')
}

function withAggregateDisplayName(
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

function GroupGuide() {
  return <div className="groupGuide" aria-hidden="true" />
}

type DiscoveryIssueTone = 'warning' | 'missing' | 'invalid'

type DiscoveryIssueItem = {
  project: string
  tone: DiscoveryIssueTone
  label: string
  summary: string
  fullError: string | null
  lastSeenAt: string | null
  lastScanAt: string | null
  configSummary: string | null
  stackId: string | null
}

const DISCOVERY_ISSUE_ORDER: Record<DiscoveryIssueTone, number> = {
  invalid: 0,
  missing: 1,
  warning: 2,
}

function truncateText(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max - 1).trimEnd()}…` : text
}

function compactPathLabel(path: string): string {
  const trimmed = path.trim()
  if (!trimmed) return '-'
  const parts = trimmed.split(/[\\/]/).filter(Boolean)
  return parts[parts.length - 1] ?? trimmed
}

function formatDiscoveryConfigSummary(configFiles?: string[] | null): string | null {
  const items = (configFiles ?? []).map((item) => item.trim()).filter(Boolean)
  if (items.length === 0) return null
  const first = compactPathLabel(items[0])
  if (items.length === 1) return `配置 ${first}`
  return `配置 ${first} +${items.length - 1}`
}

function normalizeDiscoveryIssueError(message?: string | null): string | null {
  const raw = (message ?? '').trim()
  if (!raw) return null
  let normalized = raw.replace(/\s+/g, ' ').trim()
  while (/^(warning|invalid|missing)\s*:\s*/i.test(normalized)) {
    normalized = normalized.replace(/^(warning|invalid|missing)\s*:\s*/i, '')
  }
  normalized = normalized.replace(/^[a-z0-9_]+:\s*/i, '').trim()
  return normalized || raw
}

function summarizeDiscoveryIssueError(message?: string | null): { summary: string | null; fullError: string | null } {
  const full = normalizeDiscoveryIssueError(message)
  if (!full) return { summary: null, fullError: null }

  const hintIndex = full.search(/\bHint:/i)
  const withoutHint = hintIndex >= 0 ? full.slice(0, hintIndex).trim().replace(/[;:,.]+$/, '') : full
  const summary = truncateText(withoutHint || full, 120)
  return { summary, fullError: full === summary ? null : full }
}

function buildDiscoveryIssue(project: DiscoveredProject, tone: DiscoveryIssueTone): DiscoveryIssueItem {
  const { summary, fullError } = summarizeDiscoveryIssueError(project.lastError)
  return {
    project: project.project,
    tone,
    label: tone === 'warning' ? '告警' : tone === 'missing' ? '缺失' : '无效',
    summary:
      summary ??
      (tone === 'warning'
        ? '发现扫描已标记告警，请检查 compose 与挂载状态。'
        : tone === 'missing'
          ? '发现项目已缺失，请检查 compose 文件或挂载路径。'
          : '发现项目无效，请修复 compose / override 配置。'),
    fullError,
    lastSeenAt: project.lastSeenAt ?? null,
    lastScanAt: project.lastScanAt ?? null,
    configSummary: formatDiscoveryConfigSummary(project.configFiles),
    stackId: project.stackId ?? null,
  }
}

const UPDATE_CANDIDATE_FILTER_QUERY_KEY = 'updates'
const UPDATE_CANDIDATE_COLLAPSED_STORAGE_PREFIX = 'dockrev:overview:updateCandidates:collapsed:v1:'
const OVERVIEW_JOBS_SSE_REFRESH_DEBOUNCE_MS = 180
const OVERVIEW_JOBS_SSE_FALLBACK_POLL_MS = 5000
const OVERVIEW_JOBS_SSE_ERROR_THRESHOLD = 3
const OVERVIEW_JOBS_SSE_RECONNECT_MS = 1500
const UPDATE_CANDIDATE_FILTERS: UpdateCandidateFilter[] = [
  'all',
  'updatable',
  'hint',
  'archMismatch',
  'blocked',
]

function normalizeUpdateCandidateFilter(value: string | null): UpdateCandidateFilter | null {
  const v = (value ?? '').trim()
  if (!v) return null
  // `UpdateCandidateFilter` is a string union; keep this explicit to avoid accidental acceptance.
  if ((UPDATE_CANDIDATE_FILTERS as readonly string[]).includes(v)) return v as UpdateCandidateFilter
  return null
}

function readUpdateCandidateFilterFromUrl(): UpdateCandidateFilter | null {
  try {
    const params = new URLSearchParams(window.location.search)
    return normalizeUpdateCandidateFilter(params.get(UPDATE_CANDIDATE_FILTER_QUERY_KEY))
  } catch {
    return null
  }
}

function writeUpdateCandidateFilterToUrl(filter: UpdateCandidateFilter, mode: 'push' | 'replace') {
  const key = UPDATE_CANDIDATE_FILTER_QUERY_KEY
  try {
    const url = new URL(window.location.href)
    if (filter === 'all') url.searchParams.delete(key)
    else url.searchParams.set(key, filter)

    const next = `${url.pathname}${url.search}${url.hash}`
    if (mode === 'push') window.history.pushState({}, '', next)
    else window.history.replaceState({}, '', next)
  } catch {
    // ignore URL update errors (e.g. locked-down environments)
  }
}

function readCollapsedFromStorage(filter: UpdateCandidateFilter): Record<string, boolean> {
  const key = `${UPDATE_CANDIDATE_COLLAPSED_STORAGE_PREFIX}${filter}`
  try {
    const raw = window.localStorage.getItem(key)
    if (!raw) return {}
    const json = JSON.parse(raw)
    if (!json || typeof json !== 'object') return {}
    const out: Record<string, boolean> = {}
    for (const [k, v] of Object.entries(json as Record<string, unknown>)) {
      if (typeof k !== 'string' || !k) continue
      if (typeof v !== 'boolean') continue
      out[k] = v
    }
    return out
  } catch {
    return {}
  }
}

function writeCollapsedToStorage(filter: UpdateCandidateFilter, value: Record<string, boolean>) {
  const key = `${UPDATE_CANDIDATE_COLLAPSED_STORAGE_PREFIX}${filter}`
  try {
    window.localStorage.setItem(key, JSON.stringify(value))
  } catch {
    // ignore quota/serialization errors
  }
}

function withCollapseDefaults(
  collapsed: Record<string, boolean>,
  stacks: StackListItem[],
): Record<string, boolean> {
  const next = { ...collapsed }
  for (const st of stacks) {
    if (next[st.id] == null) next[st.id] = st.updates === 0
  }
  return next
}

export function OverviewPage(props: {
  onLastScanHint: (lastScan?: string) => void
  onTopActions: (node: ReactNode) => void
}) {
  const { onLastScanHint, onTopActions } = props
  const confirm = useConfirm()
  const [filter, setFilter] = useState<UpdateCandidateFilter>(() => readUpdateCandidateFilterFromUrl() ?? 'all')
  const [stacks, setStacks] = useState<StackListItem[]>([])
  const [details, setDetails] = useState<Record<string, StackDetail | undefined>>({})
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>(() => {
    const initialFilter = readUpdateCandidateFilterFromUrl() ?? 'all'
    return readCollapsedFromStorage(initialFilter)
  })
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [discoveredProjects, setDiscoveredProjects] = useState<DiscoveredProject[]>([])
  const [error, setError] = useState<string | null>(null)
  const [noticeJobId, setNoticeJobId] = useState<string | null>(null)
  const [noticeDiscoveryJobId, setNoticeDiscoveryJobId] = useState<string | null>(null)
  const [noticeCheckJobId, setNoticeCheckJobId] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const jobsRefreshErrorRef = useRef<string | null>(null)
  const refreshRequestIdRef = useRef(0)
  const latestAppliedStacksRequestIdRef = useRef(0)
  const latestAppliedJobsRequestIdRef = useRef(0)
  const latestAppliedProjectsRequestIdRef = useRef(0)
  const { beginSubmitting, endSubmitting, trackJob, isTargetBusy, getActiveJobByTarget, isTargetSubmitting } =
    useUpdateActionTracker()
  const supervisor = useSupervisorHealth()
  const selfUpgradeUrl = useMemo(() => selfUpgradeBaseUrl(), [])
  const allApplyActionBusy = isTargetBusy('all')
  const allApplyActiveJob = getActiveJobByTarget('all')
  const allApplySubmitting = isTargetSubmitting('all')

  const lastDiscoveryScanAt = useMemo(() => {
    const candidates = jobs
      .filter((j) => j.type === 'discovery' && j.status === 'success')
      .sort((a, b) => String(b.finishedAt ?? b.createdAt ?? '').localeCompare(String(a.finishedAt ?? a.createdAt ?? '')))
    const j = candidates[0]
    if (!j) return null
    return getDiscoveryScanStartedAt(j.summary) ?? j.finishedAt ?? j.createdAt ?? null
  }, [jobs])

  const lastDiscoveryProjectsScanAt = useMemo(() => {
    const ts = discoveredProjects
      .map((p) => p.lastScanAt ?? '')
      .filter(Boolean)
      .sort()
      .at(-1)
    return ts || null
  }, [discoveredProjects])

  const refresh = useCallback(async () => {
    const requestId = ++refreshRequestIdRef.current
    const errors: string[] = []
    setError(null)
    try {
      const stacksPromise = listStacks()
      const jobsPromise = listJobs()
      const projectsPromise = listDiscoveryProjects('exclude')

      const [stacksRes, jobsRes, projectsRes] = await Promise.allSettled([stacksPromise, jobsPromise, projectsPromise])

      if (jobsRes.status === 'rejected') errors.push('jobs unavailable')
      if (projectsRes.status === 'rejected') errors.push('discovery projects unavailable')
      if (stacksRes.status === 'rejected') throw stacksRes.reason

      const s = stacksRes.value
      const maxLastScan = s.map((x) => x.lastCheckAt).sort().at(-1)

      if (jobsRes.status === 'fulfilled' && requestId >= latestAppliedJobsRequestIdRef.current) {
        latestAppliedJobsRequestIdRef.current = requestId
        setJobs(jobsRes.value)
      }
      if (projectsRes.status === 'fulfilled' && requestId >= latestAppliedProjectsRequestIdRef.current) {
        latestAppliedProjectsRequestIdRef.current = requestId
        setDiscoveredProjects(projectsRes.value)
      }
      if (requestId < latestAppliedStacksRequestIdRef.current) return
      latestAppliedStacksRequestIdRef.current = requestId
      setStacks(s)
      onLastScanHint(maxLastScan)
      setCollapsed((prev) => {
        const next = { ...prev }
        for (const st of s) {
          if (next[st.id] == null) next[st.id] = st.updates === 0
        }
        return next
      })
      setError(errors.length > 0 ? errors.join(' · ') : null)

      const ids = s.map((x) => x.id)
      const results = await Promise.all(
        ids.map(async (id) => {
          try {
            return [id, await getStack(id)] as const
          } catch {
            return [id, undefined] as const
          }
        }),
      )
      if (requestId < latestAppliedStacksRequestIdRef.current) return
      setDetails(Object.fromEntries(results))
    } catch (error: unknown) {
      if (requestId < latestAppliedStacksRequestIdRef.current) return
      throw error
    }
  }, [onLastScanHint])

  const patchStackDetails = useCallback(async (stackIds: string[]) => {
    const ids = [...new Set(stackIds.map((id) => id.trim()).filter(Boolean))]
    if (ids.length === 0) return

    const results = await Promise.all(
      ids.map(async (id) => {
        try {
          return [id, await getStack(id)] as const
        } catch {
          return [id, undefined] as const
        }
      }),
    )

    setDetails((prev) => ({ ...prev, ...Object.fromEntries(results) }))
  }, [])

  const patchStackList = useCallback(
    async (stackIds: string[]) => {
      const ids = new Set(stackIds.map((id) => id.trim()).filter(Boolean))
      if (ids.size === 0) return

      const next = await listStacks()
      const byId = new Map(next.map((item) => [item.id, item] as const))
      const maxLastScan = next.map((item) => item.lastCheckAt).sort().at(-1)

      setStacks((prev) => prev.map((item) => byId.get(item.id) ?? item))
      onLastScanHint(maxLastScan)
      setCollapsed((prev) => {
        const merged = { ...prev }
        for (const item of next) {
          if (merged[item.id] == null) merged[item.id] = item.updates === 0
        }
        return merged
      })
    },
    [onLastScanHint],
  )

  const patchServiceInStackDetails = useCallback(
    (stackId: string, serviceId: string, patch: (svc: Service) => Service) => {
      setDetails((prev) => {
        const stack = prev[stackId]
        if (!stack) return prev
        let changed = false
        const nextServices = stack.services.map((svc) => {
          if (svc.id !== serviceId) return svc
          changed = true
          return patch(svc)
        })
        if (!changed) return prev
        return {
          ...prev,
          [stackId]: {
            ...stack,
            services: nextServices,
          },
        }
      })
    },
    [],
  )

  const resolveSettledStackIds = useCallback(
    (detail: UpdateJobSettledDetail): string[] => {
      const explicitStackId = (detail.stackId ?? '').trim()
      if (explicitStackId) return [explicitStackId]

      const explicitServiceId = (detail.serviceId ?? '').trim()
      if (explicitServiceId) {
        const matched = Object.entries(details)
          .filter(([, stack]) => stack?.services.some((svc) => svc.id === explicitServiceId))
          .map(([stackId]) => stackId)
        if (matched.length > 0) return matched
      }

      if (detail.target.startsWith('stack:')) return [detail.target.slice('stack:'.length)]
      if (detail.target.startsWith('service:')) {
        const serviceId = detail.target.slice('service:'.length)
        return Object.entries(details)
          .filter(([, stack]) => stack?.services.some((svc) => svc.id === serviceId))
          .map(([stackId]) => stackId)
      }

      if (detail.scope === 'all' || detail.target === 'all') return stacks.map((stack) => stack.id)
      return []
    },
    [details, stacks],
  )

  const requestRefresh = usePageResumeRefresh(refresh, {
    onError: (e: unknown) => setError(e instanceof Error ? e.message : String(e)),
  })

  useEffect(() => {
    void requestRefresh().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
  }, [requestRefresh])

  useEffect(() => {
    let closed = false
    let es: EventSource | null = null
    let errorStreak = 0
    let lastEventId = 0
    let refreshRequestId = 0
    let refreshTimer: number | null = null
    let pollTimer: number | null = null
    let reconnectTimer: number | null = null

    const refreshJobs = async () => {
      const requestId = ++refreshRequestId
      try {
        const next = await listJobs()
        if (requestId !== refreshRequestId) return
        if (!closed) {
          setJobs(next)
          const previousJobsError = jobsRefreshErrorRef.current
          jobsRefreshErrorRef.current = null
          if (previousJobsError) {
            setError((prev) => (prev === previousJobsError ? null : prev))
          }
        }
      } catch (e: unknown) {
        if (requestId !== refreshRequestId) return
        if (!closed) {
          const message = e instanceof Error ? e.message : String(e)
          jobsRefreshErrorRef.current = message
          setError(message)
        }
      }
    }

    const clearRefreshTimer = () => {
      if (refreshTimer != null) window.clearTimeout(refreshTimer)
      refreshTimer = null
    }

    const scheduleRefresh = (delayMs: number) => {
      if (refreshTimer != null) return
      refreshTimer = window.setTimeout(() => {
        refreshTimer = null
        void refreshJobs()
      }, delayMs)
    }

    const stopPolling = () => {
      if (pollTimer != null) window.clearInterval(pollTimer)
      pollTimer = null
    }

    const startPolling = () => {
      if (pollTimer != null) return
      pollTimer = window.setInterval(() => {
        void refreshJobs()
      }, OVERVIEW_JOBS_SSE_FALLBACK_POLL_MS)
    }

    const clearReconnectTimer = () => {
      if (reconnectTimer != null) window.clearTimeout(reconnectTimer)
      reconnectTimer = null
    }

    const trackEventId = (evt: Event) => {
      const idRaw = (evt as MessageEvent).lastEventId
      if (typeof idRaw !== 'string') return
      const parsed = Number.parseInt(idRaw, 10)
      if (Number.isFinite(parsed) && parsed > 0) lastEventId = parsed
    }

    const connect = () => {
      if (closed) return
      const opts = lastEventId > 0 ? { afterId: lastEventId } : undefined
      es = newJobsEventsSource(opts)

      es.addEventListener('open', () => {
        errorStreak = 0
        stopPolling()
        scheduleRefresh(0)
      })

      es.addEventListener('job_event', (evt: Event) => {
        trackEventId(evt)
        scheduleRefresh(OVERVIEW_JOBS_SSE_REFRESH_DEBOUNCE_MS)
      })

      es.addEventListener('job_events_error', () => {
        scheduleRefresh(0)
      })

      es.onerror = () => {
        errorStreak += 1
        scheduleRefresh(0)
        if (errorStreak < OVERVIEW_JOBS_SSE_ERROR_THRESHOLD) return
        es?.close()
        es = null
        startPolling()
        if (reconnectTimer != null) return
        reconnectTimer = window.setTimeout(() => {
          reconnectTimer = null
          connect()
        }, OVERVIEW_JOBS_SSE_RECONNECT_MS)
      }
    }

    connect()

    return () => {
      closed = true
      clearRefreshTimer()
      clearReconnectTimer()
      stopPolling()
      es?.close()
    }
  }, [])

  useEffect(() => {
    let closed = false
    const timers = new Set<number>()

    const handleRefreshError = (error: unknown) => {
      if (closed) return
      setError(error instanceof Error ? error.message : String(error))
    }

    const schedule = (task: () => Promise<void>) => {
      const timer = window.setTimeout(() => {
        timers.delete(timer)
        void task().catch(handleRefreshError)
      }, UPDATE_JOB_SETTLE_RETRY_MS)
      timers.add(timer)
    }

    const onUpdateJobSettled = (evt: Event) => {
      const detail = evt instanceof CustomEvent ? (evt.detail as UpdateJobSettledDetail | null) : null
      if (!detail) return

      const isAll = detail.scope === 'all' || detail.target === 'all'
      const stackIds = resolveSettledStackIds(detail)
      if (isAll || stackIds.length === 0) {
        void requestRefresh().catch(handleRefreshError)
        schedule(async () => {
          await requestRefresh()
        })
        return
      }

      void patchStackDetails(stackIds).catch(handleRefreshError)
      schedule(async () => {
        await patchStackDetails(stackIds)
        await patchStackList(stackIds)
      })
    }

    window.addEventListener(UPDATE_JOB_SETTLED_EVENT, onUpdateJobSettled)
    return () => {
      closed = true
      for (const timer of timers) window.clearTimeout(timer)
      window.removeEventListener(UPDATE_JOB_SETTLED_EVENT, onUpdateJobSettled)
    }
  }, [patchStackDetails, patchStackList, requestRefresh, resolveSettledStackIds])

  const applyDigestSnapshotUpdate = useCallback(
    (detail: DigestSnapshotUpdatedDetail) => {
      // Popover-triggered refresh stays local to the clicked service, but when that service's
      // current/candidate happen to share one digest both sides should consume the new snapshot.
      const imageRepo = (detail.imageRepo ?? '').trim().toLowerCase()
      const digestNorm = normalizeDigest(detail.digest)?.toLowerCase() ?? null
      const triggerServiceId = (detail.triggerServiceId ?? '').trim()
      if (!imageRepo || !triggerServiceId || !digestNorm) return

      const failures = scanHasFailures(detail.scan)
      const complete = scanIsComplete(detail.scan)

      const patchService = (svc: Service): Service => {
        if (svc.id !== triggerServiceId) return svc
        const svcRepo = imageRepoFromImageRef(svc.image.ref)
        if (!svcRepo || svcRepo !== imageRepo) return svc

        let changed = false
        let next: Service = svc

        const currentDigest = normalizeDigest(svc.image.digest)?.toLowerCase() ?? null
        if (currentDigest && currentDigest === digestNorm && !isStrictSemverTag(svc.image.tag)) {
          const inferred = inferResolvedTagsFromSnapshot(detail.tags, svc.image.tag)
          const inferredFirst = inferred[0] ?? null
          if (inferredFirst || (!failures && complete)) {
            changed = true
            next = {
              ...next,
              image: {
                ...next.image,
                resolvedTag: inferredFirst,
                resolvedTags: inferred.length > 1 ? inferred : null,
              },
            }
          }
        }

        const candidate = next.candidate
        const candidateDigest = candidate ? normalizeDigest(candidate.digest)?.toLowerCase() ?? null : null
        if (candidate && candidateDigest && candidateDigest === digestNorm && !isStrictSemverTag(candidate.tag)) {
          const inferred = inferResolvedTagsFromSnapshot(detail.tags, candidate.tag)
          const inferredFirst = inferred[0] ?? null
          if (inferredFirst || (!failures && complete)) {
            changed = true
            next = {
              ...next,
              candidate: { ...candidate, resolvedTag: inferredFirst },
            }
          }
        }

        return changed ? next : svc
      }

      setDetails((prev) => {
        let changed = false
        const next: Record<string, StackDetail | undefined> = { ...prev }

        for (const [stackId, stack] of Object.entries(prev)) {
          if (!stack) continue
          let stackChanged = false
          const nextServices = stack.services.map((svc) => {
            const patched = patchService(svc)
            if (patched !== svc) stackChanged = true
            return patched
          })
          if (!stackChanged) continue
          changed = true
          next[stackId] = { ...stack, services: nextServices }
        }

        return changed ? next : prev
      })
    },
    [],
  )

  useEffect(() => {
    if (typeof window === 'undefined') return
    const onDigestSnapshotUpdated = (evt: Event) => {
      const detail =
        evt instanceof CustomEvent
          ? (evt.detail as DigestSnapshotUpdatedDetail | null)
          : null
      if (!detail) return
      applyDigestSnapshotUpdate(detail)
    }
    window.addEventListener(DIGEST_SNAPSHOT_UPDATED_EVENT, onDigestSnapshotUpdated)
    return () => {
      window.removeEventListener(DIGEST_SNAPSHOT_UPDATED_EVENT, onDigestSnapshotUpdated)
    }
  }, [applyDigestSnapshotUpdate])

  const pendingInferenceStackIds = useMemo(() => {
    const ids: string[] = []
    for (const [stackId, detail] of Object.entries(details)) {
      if (!detail) continue
      const hasPending = detail.services.some(
        (svc) => !svc.archived && svc.versionInference?.status === 'pending',
      )
      if (hasPending) ids.push(stackId)
    }
    return ids
  }, [details])

  useEffect(() => {
    if (pendingInferenceStackIds.length === 0) return
    let closed = false
    let timer: number | null = null

    const poll = async () => {
      const ids = [...pendingInferenceStackIds]
      const results = await Promise.all(
        ids.map(async (id) => {
          try {
            return [id, await getStack(id)] as const
          } catch {
            return [id, undefined] as const
          }
        }),
      )
      if (closed) return
      setDetails((prev) => ({ ...prev, ...Object.fromEntries(results) }))
      timer = window.setTimeout(() => {
        void poll()
      }, 1200)
    }

    timer = window.setTimeout(() => {
      void poll()
    }, 1200)

    return () => {
      closed = true
      if (timer != null) window.clearTimeout(timer)
    }
  }, [pendingInferenceStackIds])

  useEffect(() => {
    let closed = false
    let es: EventSource | null = null
    let timer: number | null = null
    const pending = new Set<string>()

    const flush = async () => {
      timer = null
      const ids = Array.from(pending)
      pending.clear()
      if (ids.length === 0) return

      const results = await Promise.all(
        ids.map(async (id) => {
          try {
            return [id, await getStack(id)] as const
          } catch {
            return [id, undefined] as const
          }
        }),
      )

      if (closed) return
      setDetails((prev) => ({ ...prev, ...Object.fromEntries(results) }))
    }

    const scheduleFlush = (stackId: string) => {
      if (!stackId) return
      pending.add(stackId)
      if (timer != null) return
      timer = window.setTimeout(() => {
        void flush()
      }, 200)
    }

    const start = async () => {
      let jobId: string | null = null
      try {
        const resp = await triggerRuntimeScan('all')
        jobId = resp.jobId
      } catch (e: unknown) {
        if (e instanceof ApiError && e.status === 409) {
          const d = e.details
          const existingJobId =
            d && typeof d === 'object' && d !== null && 'existingJobId' in d && typeof (d as Record<string, unknown>).existingJobId === 'string'
              ? ((d as Record<string, unknown>).existingJobId as string)
              : null
          jobId = existingJobId
        }
      }

      if (closed || !jobId) return
      es = newJobEventsSource(jobId)

      es.addEventListener('runtime_scan_service', (evt: Event) => {
        const data = (evt as MessageEvent).data
        if (typeof data !== 'string' || !data) return
        try {
          const parsed = JSON.parse(data) as unknown
          if (!parsed || typeof parsed !== 'object') return
          const p = parsed as Record<string, unknown>
          if (p.type !== 'runtime_scan_service') return
          if (p.changed !== true) return
          const stackId = typeof p.stackId === 'string' ? p.stackId : ''
          if (stackId) scheduleFlush(stackId)
        } catch {
          // ignore invalid events
        }
      })

      es.addEventListener('runtime_scan_finished', () => {
        es?.close()
        void requestRefresh().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
      })
    }

    void start()

    return () => {
      closed = true
      if (timer != null) window.clearTimeout(timer)
      es?.close()
    }
  }, [requestRefresh])

  const applyFilter = useCallback(
    (next: UpdateCandidateFilter, mode: 'push' | 'replace') => {
      setFilter(next)
      writeUpdateCandidateFilterToUrl(next, mode)
      setCollapsed(withCollapseDefaults(readCollapsedFromStorage(next), stacks))
    },
    [stacks],
  )

  const onChangeFilter = useCallback(
    (next: UpdateCandidateFilter) => {
      if (next === filter) return
      applyFilter(next, 'push')
    },
    [applyFilter, filter],
  )

  const toggleStackCollapsed = useCallback(
    (stackId: string) => {
      setCollapsed((prev) => {
        const next = { ...prev, [stackId]: !(prev[stackId] ?? false) }
        writeCollapsedToStorage(filter, next)
        return next
      })
    },
    [filter],
  )

  // Sync state from URL for back/forward or manual edits.
  useEffect(() => {
    const onNav = () => {
      const next = readUpdateCandidateFilterFromUrl() ?? 'all'
      // Only update when necessary to avoid resetting local UI state.
      if (next === filter) return
      setFilter(next)
      setCollapsed(withCollapseDefaults(readCollapsedFromStorage(next), stacks))
    }
    window.addEventListener('popstate', onNav)
    window.addEventListener('hashchange', onNav)
    return () => {
      window.removeEventListener('popstate', onNav)
      window.removeEventListener('hashchange', onNav)
    }
  }, [filter, stacks])

  const countsAll = useMemo(() => {
    const c: Record<Exclude<RowStatus, 'ok'>, number> = {
      updatable: 0,
      hint: 0,
      archMismatch: 0,
      blocked: 0,
    }
    for (const st of stacks) {
      const d = details[st.id]
      if (!d) continue
      for (const svc of d.services) {
        if (svc.archived) continue
        const stt = serviceRowStatus(svc)
        if (stt === 'ok') continue
        c[stt] += 1
      }
    }
    return c
  }, [details, stacks])

  const aggregateAll = useMemo(() => {
    const counts = emptyAggregateUpdateCounts()
    const actionablePreviewItems: AggregateUpdatePreviewListItem[] = []
    const guardedPreviewItems: AggregateUpdatePreviewListItem[] = []

    for (const st of stacks) {
      const d = details[st.id]
      if (!d) continue

      const partition = partitionAggregateUpdateServices(d.services)
      counts.updatable += partition.counts.updatable
      counts.hint += partition.counts.hint
      counts.archMismatch += partition.counts.archMismatch
      counts.blocked += partition.counts.blocked
      actionablePreviewItems.push(...withAggregateDisplayName(partition.actionable, d.name, st.id))
      guardedPreviewItems.push(...withAggregateDisplayName(partition.guardedDockrevPreview, d.name, st.id))
    }

    return { counts, actionablePreviewItems, guardedPreviewItems }
  }, [details, stacks])

  const totalServicesAll = useMemo(() => {
    let total = 0
    for (const st of stacks) {
      const d = details[st.id]
      if (!d) continue
      total += d.services.filter((svc) => !svc.archived).length
    }
    return total
  }, [details, stacks])

  const allApply = useMemo(
    () =>
      resolveAggregateUpdateActionState({
        counts: aggregateAll.counts,
        guardedDockrevPreview: aggregateAll.guardedPreviewItems,
      }),
    [aggregateAll],
  )

  const jobsSummary = useMemo(() => {
    const total = jobs.length
    const running = jobs.filter((j) => j.status === 'running').length
    const failed = jobs.filter((j) => j.status === 'failed').length
    const rolled = jobs.filter((j) => j.status === 'rolled_back').length
    const success = jobs.filter((j) => j.status === 'success').length
    const other = total - running - failed - rolled - success
    return { total, running, failed, rolled, success, other }
  }, [jobs])
  const overviewCardJobs = useMemo(
    () => selectOverviewJobsForCard(jobs, { maxItems: 10 }).map((job) => toOverviewJobCardItem(job)),
    [jobs],
  )

  const discoverySummary = useMemo(() => {
    const active = discoveredProjects.filter((p) => p.status === 'active' && !p.archived)
    const warning = discoveredProjects.filter((p) => p.status === 'active' && !p.archived && !!p.lastError)
    const missing = discoveredProjects.filter((p) => p.status === 'missing' && !p.archived)
    const invalid = discoveredProjects.filter((p) => p.status === 'invalid' && !p.archived)
    const issues = [
      ...invalid.map((project) => buildDiscoveryIssue(project, 'invalid')),
      ...missing.map((project) => buildDiscoveryIssue(project, 'missing')),
      ...warning.map((project) => buildDiscoveryIssue(project, 'warning')),
    ]
      .sort((a, b) => {
        const aStamp = String(a.lastSeenAt ?? a.lastScanAt ?? '')
        const bStamp = String(b.lastSeenAt ?? b.lastScanAt ?? '')
        const recencyDelta = bStamp.localeCompare(aStamp)
        if (recencyDelta !== 0) return recencyDelta
        return DISCOVERY_ISSUE_ORDER[a.tone] - DISCOVERY_ISSUE_ORDER[b.tone]
      })
      .slice(0, 4)
    return {
      active,
      warning,
      missing,
      invalid,
      issues,
      issueCount: warning.length + missing.length + invalid.length,
    }
  }, [discoveredProjects])
  const effectiveDiscoveryScanAt = lastDiscoveryScanAt ?? lastDiscoveryProjectsScanAt

  const runDiscoveryScan = useCallback(async () => {
    const ok = await confirm({
      title: '确认执行发现扫描？',
      body: (
        <>
          <div className="modalLead">发现扫描会拉取 discovery projects，并标记 missing/invalid。</div>
          <div className="modalKvGrid">
            <div className="modalKvLabel">操作</div>
            <div className="modalKvValue">
              <Mono>discovery scan</Mono>
            </div>
            <div className="modalKvLabel">可能影响</div>
            <div className="modalKvValue">创建/更新 stacks，或将 stacks 标记为 missing/invalid。</div>
          </div>
          <div className="modalDivider" />
          <div className="muted">这是“发现异常”用的扫描，不会直接重启容器。</div>
        </>
      ),
      confirmText: '开始扫描',
      cancelText: '取消',
      confirmVariant: 'primary',
      badgeText: '扫描任务',
      badgeTone: 'warn',
    })
    if (!ok) return
    setBusy(true)
    setError(null)
    try {
      const resp = await triggerDiscoveryScan()
      setNoticeDiscoveryJobId(resp.jobId)
      setJobs(await listJobs())

      const started = Date.now()
      while (Date.now() - started < 60_000) {
        const job = await getJob(resp.jobId)
        if (job.status !== 'running') break
        await new Promise((r) => setTimeout(r, 500))
      }
      setDiscoveredProjects(await listDiscoveryProjects('exclude'))
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [confirm])

  const triggerApply = useCallback(
    async (input: {
      scope: 'all' | 'stack' | 'service'
      stackId?: string
      serviceId?: string
      targetLabel: string
      buildRequest: () => Promise<TriggerUpdateInput>
      confirmBody?: ReactNode
      confirmTitle?: string
    }) => {
      const scopeLabel = input.scope === 'all' ? 'all' : input.scope === 'stack' ? 'stack' : 'service'
      const confirmVariant = input.scope === 'service' ? 'primary' : 'danger'
      const ok = await confirm({
        title: input.confirmTitle ?? '确认执行更新？',
        body:
          input.confirmBody ?? (
            <>
              <div className="modalKvGrid">
                <div className="modalKvLabel">模式</div>
                <div className="modalKvValue">
                  <Mono>apply</Mono>
                </div>
                <div className="modalKvLabel">范围</div>
                <div className="modalKvValue">
                  <Mono>{scopeLabel}</Mono>
                </div>
                <div className="modalKvLabel">目标</div>
                <div className="modalKvValue">
                  <Mono>{input.targetLabel}</Mono>
                </div>
                <div className="modalKvLabel">备份</div>
                <div className="modalKvValue">
                  <Mono>inherit</Mono>
                </div>
                <div className="modalKvLabel">架构不匹配</div>
                <div className="modalKvValue">
                  <Mono>disallow</Mono>
                </div>
              </div>
            </>
          ),
        confirmText: '执行更新',
        cancelText: '取消',
        confirmVariant,
        // Hide the pill badge; it doesn't add value for operators (scope/kv already shows intent).
        badgeText: null,
      })
      if (!ok) return

      const targetKey = resolveUpdateActionTargetKey(input.scope, input.stackId, input.serviceId)

      setError(null)
      setNoticeJobId(null)
      if (targetKey) beginSubmitting(targetKey)
      try {
        const resp = await triggerUpdate(await input.buildRequest())
        setNoticeJobId(resp.jobId)
        if (targetKey) trackJob(targetKey, resp.jobId, 'queued')
      } catch (e: unknown) {
        if (e instanceof ApiError) {
          if (e.status === 401) setError('需要登录/鉴权（Forward Auth）')
          else if (e.status === 409) {
            setError('扫描结果已变化，请刷新并重新扫描后再更新')
            await requestRefresh()
          }
          else setError(e.message)
        } else {
          setError(e instanceof Error ? e.message : String(e))
        }
      } finally {
        if (targetKey) endSubmitting(targetKey)
      }
    },
    [beginSubmitting, confirm, endSubmitting, requestRefresh, trackJob],
  )

  useEffect(() => {
    onTopActions(
      <>
        <Button
          variant="primary"
          disabled={busy}
          onClick={() => {
            void (async () => {
              setBusy(true)
              setError(null)
              setNoticeCheckJobId(null)
              try {
                const resp = await triggerCheck('all')
                setNoticeCheckJobId(resp.checkId)
                await requestRefresh()
              } catch (e: unknown) {
                if (e instanceof ApiError) {
                  if (e.status === 401) setError('需要登录/鉴权（Forward Auth）')
                  else if (e.status === 409) {
                    const d = e.details
                    const existingJobId =
                      d &&
                      typeof d === 'object' &&
                      d !== null &&
                      'existingJobId' in d &&
                      typeof (d as Record<string, unknown>).existingJobId === 'string'
                        ? ((d as Record<string, unknown>).existingJobId as string)
                        : null
                    if (existingJobId) setNoticeCheckJobId(existingJobId)
                    else setError(e.message)
                  } else setError(e.message)
                } else {
                  setError(e instanceof Error ? e.message : String(e))
                }
              } finally {
                setBusy(false)
              }
            })()
          }}
        >
          立即扫描
        </Button>
        <Button
          variant="danger"
          disabled={
            allApplyActiveJob
              ? false
              : !allApply.enabled || busy || allApplySubmitting
          }
          loading={allApplyActionBusy}
          loadingClickable={Boolean(allApplyActiveJob)}
          title={allApplyActiveJob ? '任务进行中，点击查看任务详情' : (allApply.title ?? undefined)}
          hint={allApplyActiveJob ? '任务进行中，点击查看任务详情' : (!allApply.enabled ? (allApply.hint ?? undefined) : undefined)}
          onClick={() => {
            if (allApplyActiveJob) {
              navigate({ name: 'job', jobId: allApplyActiveJob.jobId })
              return
            }
            const previewItems = [...aggregateAll.actionablePreviewItems, ...aggregateAll.guardedPreviewItems]
            const totalCandidates = aggregateAll.actionablePreviewItems.length
            const anomalyCount = previewItems.filter((item) => isSemverDowngradeAnomaly(item.svc)).length
            const body = (
              <>
                <div className="modalKvGrid">
                  <div className="modalKvLabel">范围</div>
                  <div className="modalKvValue">
                    <Mono>all</Mono>
                  </div>
                  <div className="modalKvLabel">候选服务</div>
                  <div className="modalKvValue">{totalCandidates} 个（可更新/需确认）</div>
                  <div className="modalKvLabel">其中</div>
                  <div className="modalKvValue">
                    可更新 {aggregateAll.counts.updatable} · 需确认 {aggregateAll.counts.hint}
                  </div>
                  <div className="modalKvLabel">将跳过</div>
                  <div className="modalKvValue">
                    架构不匹配 {aggregateAll.counts.archMismatch} · 被阻止 {aggregateAll.counts.blocked}
                  </div>
                </div>
                {anomalyCount > 0 ? (
                  <div className="muted" style={{ marginTop: 10 }}>
                    ⚠ 检测到 {anomalyCount} 个版本异常（候选低于当前）；手动确认后仍可继续更新。
                  </div>
                ) : null}
	                <div className="modalDivider" />
	                <div className="modalLead">将更新的服务（预览）</div>
	                <AggregateUpdatePreviewList
	                  items={previewItems}
	                  dockrevGuardHint={DOCKREV_AGGREGATE_GUARD_HINT}
                    onServiceResolvedTags={(update) => {
                      const stackId = (update.stackId ?? '').trim()
                      if (!stackId) return
                      patchServiceInStackDetails(stackId, update.serviceId, (prev) => ({
                        ...prev,
                        image: {
                          ...prev.image,
                          resolvedTag: update.resolvedTag,
                          resolvedTags: update.resolvedTags,
                        },
                      }))
                    }}
                    onServiceCandidateResolvedTag={(update) => {
                      const stackId = (update.stackId ?? '').trim()
                      if (!stackId) return
                      patchServiceInStackDetails(stackId, update.serviceId, (prev) => ({
                        ...prev,
                        candidate: prev.candidate
                          ? {
                              ...prev.candidate,
                              resolvedTag: update.resolvedTag,
                            }
                          : prev.candidate,
                      }))
                    }}
	                />
	                <div className="modalDivider" />
	              </>
	            )
                        void triggerApply({
                          scope: 'all',
                          targetLabel: '全部服务',
                          buildRequest: async () => ({
                            scope: 'all',
                            targets: await buildUpdateServiceTargets(
                              aggregateAll.actionablePreviewItems.map((item) => item.svc),
                            ),
                            mode: 'apply',
                            allowArchMismatch: false,
                            backupMode: 'inherit',
                          }),
                          confirmBody: body,
                          confirmTitle: '确认更新全部服务？',
                        })
          }}
        >
          {allApplyActiveJob?.status === 'queued'
            ? '排队中…'
            : allApplyActiveJob
              ? '更新中…'
              : allApplySubmitting
                ? '提交中…'
                : '更新全部'}
        </Button>
      </>,
    )
	  }, [
	    aggregateAll,
	    allApply,
	    allApplyActiveJob,
	    allApplyActionBusy,
	    allApplySubmitting,
	    busy,
	    onTopActions,
	    patchServiceInStackDetails,
	    requestRefresh,
	    triggerApply,
	  ])

  return (
    <div className="page">
      <div className="twoCol">
        <div className="card">
          <div className="sectionRow">
            <div>
              <div className="title">运行态与结果</div>
              <div className="muted">更新任务（队列）摘要</div>
            </div>
            <div style={{ marginLeft: 'auto', display: 'flex', gap: 10 }}>
              <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: 'queue' })}>
                查看队列
              </Button>
            </div>
          </div>
          <div className="chipRow" style={{ marginTop: 14 }}>
            <div className="chipStatic">{`运行中: ${jobsSummary.running}`}</div>
            <div className="chipStatic">{`失败: ${jobsSummary.failed}`}</div>
            <div className="chipStatic">{`回滚: ${jobsSummary.rolled}`}</div>
            <div className="chipStatic">{`成功: ${jobsSummary.success}`}</div>
            {jobsSummary.other > 0 ? <div className="chipStatic">{`其他: ${jobsSummary.other}`}</div> : null}
          </div>
          <div className="overviewJobsList">
            {overviewCardJobs.length === 0 ? (
              <div className="muted">暂无任务</div>
            ) : (
              overviewCardJobs.map((job) => {
                const progressTitle =
                  job.progressMode === 'determinate' && job.progressPercent !== null
                    ? ` · progress ${job.progressPercent}%`
                    : job.progressMode === 'indeterminate'
                      ? ' · progress running'
                      : ''
                const title = `${job.status} · ${job.primaryLabel}${job.scopeTag ? ` ${job.scopeTag}` : ''} · ${formatShort(job.createdAt)} · by ${job.createdBy} · reason ${job.reason}${progressTitle}`
                const ariaLabel = `${job.status}，${job.primaryLabel}${job.scopeTag ? ` ${job.scopeTag}` : ''}，创建时间 ${formatShort(job.createdAt)}，创建人 ${job.createdBy}，来源 ${job.reason}${
                  job.progressMode === 'determinate' && job.progressPercent !== null
                    ? `，进度 ${job.progressPercent}%`
                    : job.progressMode === 'indeterminate'
                      ? '，进度运行中'
                      : ''
                }`
                return (
                  <button
                    key={job.jobId}
                    type="button"
                    className="overviewJobListRow"
                    data-progress-mode={job.progressMode}
                    style={
                      (job.progressMode === 'determinate' && job.progressPercent !== null
                        ? { '--overview-row-progress': `${job.progressPercent}%` }
                        : undefined) as CSSProperties | undefined
                    }
                    onClick={() => navigate({ name: 'job', jobId: job.jobId })}
                    title={title}
                    aria-label={ariaLabel}
                  >
                    {job.progressMode !== 'none' ? (
                      <span className="overviewJobProgressBg" aria-hidden="true">
                        <span className="overviewJobProgressBgFill" />
                        <span className="overviewJobProgressBgShimmer" />
                      </span>
                    ) : null}
                    <span className="overviewJobLine">
                      <span className="overviewJobStatusTag" data-status={job.status}>
                        {job.status}
                      </span>
                      <span className={`overviewJobTitle overviewJobTitle-${job.typeTone}`}>
                        {job.primaryLabel}
                        {job.scopeTag ? <span className="overviewJobScope"> · {job.scopeTag}</span> : null}
                      </span>
                      <span className="overviewJobLineMeta">
                        <span>{formatCompactDateTime(job.createdAt)}</span>
                        <span className="overviewJobLineMetaSep">·</span>
                        <span>{job.createdBy}</span>
                        {job.reason && job.reason !== 'ui' ? (
                          <>
                            <span className="overviewJobLineMetaSep">·</span>
                            <span>{job.reason}</span>
                          </>
                        ) : null}
                      </span>
                    </span>
                  </button>
                )
              })
            )}
          </div>
        </div>

        <div className="card">
          <div className="sectionRow">
            <div className="discoveryCardHeader">
              <div className="title">扫描与发现异常</div>
              <div className="muted">按最近发现结果聚焦 warning / missing / invalid 项目</div>
            </div>
            <div className="discoveryCardActions">
              <Button variant="ghost" disabled={busy} onClick={runDiscoveryScan}>
                执行发现扫描
              </Button>
              <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: 'services' })}>
                查看服务
              </Button>
            </div>
          </div>
          <div className="chipRow discoverySummaryRow">
            <div className="discoveryStatChip discoveryStatChipTotal">
              <span className="discoveryStatLabel">异常项目</span>
              <span className="discoveryStatValue">{discoverySummary.issueCount}</span>
            </div>
            <div className="discoveryStatChip discoveryStatChipWarn">
              <span className="discoveryStatLabel">告警</span>
              <span className="discoveryStatValue">{discoverySummary.warning.length}</span>
            </div>
            <div className="discoveryStatChip discoveryStatChipBad">
              <span className="discoveryStatLabel">缺失</span>
              <span className="discoveryStatValue">{discoverySummary.missing.length}</span>
            </div>
            <div className="discoveryStatChip discoveryStatChipBad">
              <span className="discoveryStatLabel">无效</span>
              <span className="discoveryStatValue">{discoverySummary.invalid.length}</span>
            </div>
            <div className="discoveryStatChip discoveryStatChipInfo">
              <span className="discoveryStatLabel">活跃</span>
              <span className="discoveryStatValue">{discoverySummary.active.length}</span>
            </div>
            {effectiveDiscoveryScanAt ? (
              <div className="discoveryStatChip discoveryStatChipScan">
                <span className="discoveryStatLabel">最近扫描</span>
                <span className="discoveryStatValue">{formatCompactDateTime(effectiveDiscoveryScanAt)}</span>
              </div>
            ) : null}
          </div>
          <div className="muted discoverySummaryLead">
            {discoverySummary.issueCount > 0
              ? `共 ${discoverySummary.issueCount} 个异常项目，优先展示最近 ${discoverySummary.issues.length} 个需要立即处理的条目。`
              : '最近一次扫描未发现需要处理的 warning / missing / invalid 项目。'}
          </div>
          {discoverySummary.issues.length > 0 ? (
            <div className="discoveryIssueList">
              {discoverySummary.issues.map((issue) => {
                const metaParts = [
                  issue.lastSeenAt ? `最近发现 ${formatCompactDateTime(issue.lastSeenAt)}` : null,
                  issue.lastScanAt ? `最近扫描 ${formatCompactDateTime(issue.lastScanAt)}` : null,
                  issue.configSummary,
                  issue.stackId ? `关联 ${issue.stackId}` : null,
                ].filter((part): part is string => Boolean(part))
                const pillTone = issue.tone === 'warning' ? 'warn' : 'bad'

                return (
                  <div key={`${issue.tone}:${issue.project}`} className="discoveryIssueRow">
                    <div className="discoveryIssueHeadline">
                      <div className="discoveryIssuePrimary">
                        <Pill tone={pillTone}>{issue.label}</Pill>
                        <span className="mono monoPrimary discoveryIssueProject" title={issue.project}>
                          {issue.project}
                        </span>
                      </div>
                      <div className="discoveryIssueSummaryWrap">
                        <span className="discoveryIssueSummary" title={issue.fullError ?? issue.summary}>
                          {issue.summary}
                        </span>
                        {issue.fullError ? (
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <button
                                type="button"
                                className="discoveryIssueDetailsBtn"
                                aria-label={`查看 ${issue.project} 的完整异常详情`}
                                title={issue.fullError}
                              >
                                详情
                              </button>
                            </TooltipTrigger>
                            <TooltipContent className="discoveryIssueTooltip">{issue.fullError}</TooltipContent>
                          </Tooltip>
                        ) : null}
                      </div>
                    </div>
                    {metaParts.length > 0 ? (
                      <div className="discoveryIssueMeta">
                        {metaParts.map((part, index) => (
                          <span key={`${issue.project}:${part}`} className="discoveryIssueMetaPart">
                            {index > 0 ? <span className="discoveryIssueMetaSep">·</span> : null}
                            <span>{part}</span>
                          </span>
                        ))}
                      </div>
                    ) : null}
                  </div>
                )
              })}
            </div>
          ) : (
            <div className="discoveryIssueEmpty">
              <div className="discoveryIssueEmptyTitle">当前没有需要处理的发现异常</div>
              <div className="muted">需要时仍可执行发现扫描，刷新 discovery projects 与 stacks 的最新状态。</div>
            </div>
          )}
        </div>
      </div>

        <div className="overviewIndent">
        <div className="title">更新候选</div>

        <div style={{ marginTop: 14 }}>
          <UpdateCandidateFilters value={filter} onChange={onChangeFilter} total={totalServicesAll} counts={countsAll} />
        </div>

        <div className="table" style={{ marginTop: 14 }}>
          <div className="tableHeader">
            <div>Service</div>
            <div>Image</div>
            <div>Versions</div>
            <div>状态 / 备注</div>
            <div>操作</div>
          </div>

	          {stacks.map((st) => {
	            const d = details[st.id]
	            if (!d) return null
	
	            const rows = d.services
	              .filter((svc) => !svc.archived)
	              .map((svc) => ({ svc, stt: serviceRowStatus(svc) }))
	              .filter((x) => filter === 'all' || x.stt === filter)

	            if (rows.length === 0) return null

            const aggregatePartition = partitionAggregateUpdateServices(d.services)
            const aggregatePreviewItems = [
              ...withAggregateDisplayName(aggregatePartition.actionable, undefined, st.id),
              ...withAggregateDisplayName(aggregatePartition.guardedDockrevPreview, undefined, st.id),
            ]
            const isCollapsed = collapsed[st.id] ?? false
            const totalServices = d.services.filter((svc) => !svc.archived).length
            const groupSummary = formatGroupSummary(totalServices, aggregatePartition.counts)
            const stackApply = resolveAggregateUpdateActionState(aggregatePartition)
            const stackApplyActionKey = resolveUpdateActionTargetKey('stack', st.id, null)
            const stackApplyActiveJob = stackApplyActionKey ? getActiveJobByTarget(stackApplyActionKey) : null
            const stackApplySubmitting = stackApplyActionKey ? isTargetSubmitting(stackApplyActionKey) : false

            return (
              <div key={st.id} className={isCollapsed ? 'tableGroup' : 'tableGroup tableGroupExpanded'}>
                {!isCollapsed ? <GroupGuide /> : null}
                <div
                  className="groupHead"
                  role="button"
                  tabIndex={0}
                  onClick={() => toggleStackCollapsed(st.id)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault()
                      toggleStackCollapsed(st.id)
                    }
                  }}
                >
                  <div className="cellService cellServiceGroup">
                    <StackIcon variant={isCollapsed ? 'collapsed' : 'expanded'} />
                    <div className="groupTitle">{d.name}</div>
                  </div>
                  <div className="groupMeta">{groupSummary}</div>
                  <div />
                  <div />
                  <div
                    className="actionCell"
                    onClick={(e) => e.stopPropagation()}
                    onKeyDown={(e) => e.stopPropagation()}
                  >
                    <Button
                      variant="ghost"
                      disabled={
                        stackApplyActiveJob
                          ? false
                          : !stackApply.enabled || busy || stackApplySubmitting
                      }
                      loading={stackApplyActionKey ? isTargetBusy(stackApplyActionKey) : false}
                      loadingClickable={Boolean(stackApplyActiveJob)}
                      title={stackApplyActiveJob ? '任务进行中，点击查看任务详情' : (stackApply.title ?? undefined)}
                      hint={stackApplyActiveJob ? '任务进行中，点击查看任务详情' : (!stackApply.enabled ? (stackApply.hint ?? undefined) : undefined)}
                      onClick={() => {
                          if (stackApplyActiveJob) {
                            navigate({ name: 'job', jobId: stackApplyActiveJob.jobId })
                            return
                          }
			                        const totalCandidates = aggregatePartition.actionable.length
                        const anomalyCount = aggregatePreviewItems.filter((item) =>
                          isSemverDowngradeAnomaly(item.svc),
                        ).length
                        const body = (
                          <>
                            <div className="modalKvGrid">
                              <div className="modalKvLabel">范围</div>
                              <div className="modalKvValue">
                                <Mono>stack</Mono>
                              </div>
                              <div className="modalKvLabel">目标</div>
                              <div className="modalKvValue">
                                <Mono>{d.name}</Mono>
                              </div>
                              <div className="modalKvLabel">候选服务</div>
                              <div className="modalKvValue">{totalCandidates} 个（可更新/需确认）</div>
                              <div className="modalKvLabel">其中</div>
                              <div className="modalKvValue">
                                可更新 {aggregatePartition.counts.updatable} · 需确认 {aggregatePartition.counts.hint}
                              </div>
                              <div className="modalKvLabel">将跳过</div>
                              <div className="modalKvValue">
                                架构不匹配 {aggregatePartition.counts.archMismatch} · 被阻止 {aggregatePartition.counts.blocked}
                              </div>
                            </div>
                            {anomalyCount > 0 ? (
                              <div className="muted" style={{ marginTop: 10 }}>
                                ⚠ 检测到 {anomalyCount} 个版本异常（候选低于当前）；手动确认后仍可继续更新。
                              </div>
                            ) : null}
	                            <div className="modalDivider" />
	                            <div className="modalLead">将更新的服务（预览）</div>
	                            <AggregateUpdatePreviewList
	                              items={aggregatePreviewItems}
	                              dockrevGuardHint={DOCKREV_AGGREGATE_GUARD_HINT}
                                onServiceResolvedTags={(update) => {
                                  const stackId = (update.stackId ?? '').trim() || st.id
                                  patchServiceInStackDetails(stackId, update.serviceId, (prev) => ({
                                    ...prev,
                                    image: {
                                      ...prev.image,
                                      resolvedTag: update.resolvedTag,
                                      resolvedTags: update.resolvedTags,
                                    },
                                  }))
                                }}
                                onServiceCandidateResolvedTag={(update) => {
                                  const stackId = (update.stackId ?? '').trim() || st.id
                                  patchServiceInStackDetails(stackId, update.serviceId, (prev) => ({
                                    ...prev,
                                    candidate: prev.candidate
                                      ? {
                                          ...prev.candidate,
                                          resolvedTag: update.resolvedTag,
                                        }
                                      : prev.candidate,
                                  }))
                                }}
	                            />
	                            <div className="modalDivider" />
	                          </>
	                        )
                                                void triggerApply({
	                          scope: 'stack',
	                          stackId: st.id,
	                          targetLabel: `stack:${d.name}`,
	                          buildRequest: async () => ({
	                            scope: 'stack',
	                            stackId: st.id,
	                            targets: await buildUpdateServiceTargets(
	                              aggregatePartition.actionable.map((item) => item.svc),
	                            ),
	                            mode: 'apply',
	                            allowArchMismatch: false,
	                            backupMode: 'inherit',
	                          }),
	                          confirmBody: body,
	                          confirmTitle: `确认更新此 stack？`,
	                        })
	                      }}
	                    >
                        {stackApplyActiveJob?.status === 'queued'
                          ? '排队中…'
                          : stackApplyActiveJob
                            ? '更新中…'
                            : stackApplySubmitting
                              ? '提交中…'
                              : '更新此 stack'}
	                    </Button>
                  </div>
                </div>

                {!isCollapsed
                  ? rows.map(({ svc, stt }) => {
                      const isDockrev = isDockrevService(svc)
                      const currentDisplayTag = formatTagDisplay(
                        svc.image.tag,
                        svc.image.resolvedTag,
                        svc.versionInference?.status,
                      )
                      const inferencePending = svc.versionInference?.status === 'pending'
                      const rawTagTrim = (svc.image.tag ?? '').trim()
                      const showRawTag = Boolean(rawTagTrim && rawTagTrim !== currentDisplayTag)
                      const candidateTag = svc.candidate?.tag && svc.candidate.tag !== '-' ? svc.candidate.tag : null
	                      const candidateDisplayTag = candidateTag
	                        ? formatCandidateTagDisplay(
                            candidateTag,
                            svc.candidate?.resolvedTag ?? null,
                            svc.versionInference?.status,
                          )
	                        : null
	                      const showCandidate = Boolean(candidateDisplayTag && candidateDisplayTag !== currentDisplayTag)
	                      const candidatePrefetchOnMount =
	                        candidateTag && candidateDisplayTag
	                          ? shouldPrefetchFloatingCandidate(
	                              candidateTag,
	                              svc.candidate?.resolvedTag ?? null,
	                              svc.candidate?.digest ?? null,
	                            )
	                          : false
	                      const arrowPulse = inferencePending
                      const svcApply =
                        stt === 'updatable'
                          ? { enabled: true, title: null as string | null, note: null as string | null }
                          : stt === 'hint'
                            ? { enabled: true, title: '需确认候选；将由服务端计算是否实际变更', note: '需确认' }
                            : stt === 'ok'
                              ? { enabled: false, title: '无候选版本', note: null }
                              : stt === 'archMismatch'
                                ? { enabled: false, title: '架构不匹配（仅提示，不允许更新）', note: null }
                                : { enabled: false, title: svc.ignore?.reason ?? '被阻止', note: null }
                      const svcApplyActionKey = resolveUpdateActionTargetKey('service', null, svc.id)
                      const svcApplyActiveJob = svcApplyActionKey ? getActiveJobByTarget(svcApplyActionKey) : null
                      const svcApplySubmitting = svcApplyActionKey ? isTargetSubmitting(svcApplyActionKey) : false
                      return (
                        <div
                          key={svc.id}
                          className="rowLine"
                          onClick={(e) => {
                            const t = e.target as unknown
                            const el =
                              t instanceof Element
                                ? t
                                : t && (t as { parentElement?: unknown }).parentElement instanceof Element
                                  ? (t as { parentElement: Element }).parentElement
                                  : null
                            if (el?.closest('button, a, input, select, textarea')) return
                            navigate({ name: 'service', stackId: st.id, serviceId: svc.id })
                          }}
                          role="button"
                          tabIndex={0}
                          onKeyDown={(e) => {
                            const t = e.target as unknown
                            const el =
                              t instanceof Element
                                ? t
                                : t && (t as { parentElement?: unknown }).parentElement instanceof Element
                                  ? (t as { parentElement: Element }).parentElement
                                  : null
                            if (el?.closest('button, a, input, select, textarea')) return
                            if (e.key === 'Enter' || e.key === ' ') {
                              e.preventDefault()
                              navigate({ name: 'service', stackId: st.id, serviceId: svc.id })
                            }
                          }}
	                        >
	                          <div className="cellService">
	                            <span className="svcBullet" aria-hidden="true" />
	                            <span className="svcName">{svc.name}</span>
	                          </div>
	                          {(() => {
	                            const img = splitImageRef(svc.image.ref)
	                            const dn = splitImageNameForDisplay(img.name, svc.image.tag)
	                            const stopRowLink = (event: MouseEvent<HTMLAnchorElement>) => {
	                              event.stopPropagation()
	                            }
	                            return (
	                              <div className="cellTwoLine">
	                                <div
	                                  className="mono monoPrimary monoSplit imageLinkRow"
	                                  title={dn.suffix ? `${dn.base}${dn.suffix}` : dn.base}
	                                >
	                                  <span className="monoSplitBase">{dn.base}</span>
	                                  <ImageLinkIcons imageRef={svc.image.ref} onClick={stopRowLink} repoUrl={svc.settings.repoUrl} />
	                                </div>
	                                <div className="mono monoSecondary">{img.registry}</div>
	                              </div>
	                            )
	                          })()}
	                          <div className="cellTwoLine">
                              <div className="versionLine">
                                <CurrentVersionPopover
                                  serviceId={svc.id}
                                  displayTag={currentDisplayTag}
                                  imageTag={svc.image.tag}
                                  imageDigest={svc.image.digest ?? null}
                                  resolvedTag={svc.image.resolvedTag}
                                  resolvedTags={svc.image.resolvedTags}
                                  onLocalResolvedTags={(update) => {
                                    patchServiceInStackDetails(st.id, svc.id, (prev) => ({
                                      ...prev,
                                      image: {
                                        ...prev.image,
                                        resolvedTag: update.resolvedTag,
                                        resolvedTags: update.resolvedTags,
                                      },
                                    }))
                                  }}
                                  inferenceLoading={inferencePending}
                                />
                                {showCandidate ? (
                                  <>
                                    <span className={arrowPulse ? 'inlineIconLoading' : 'inlineIconMuted'}>
                                      <ArrowRightIcon className="inlineIcon" />
                                    </span>
                                    <VersionTagsPopover
                                      serviceId={svc.id}
                                      candidateTag={candidateTag}
                                      candidateDigest={svc.candidate?.digest ?? null}
                                      prefetchOnMount={candidatePrefetchOnMount}
                                      onLocalResolvedTag={(resolvedTag) => {
                                        patchServiceInStackDetails(st.id, svc.id, (prev) => ({
                                          ...prev,
                                          candidate: prev.candidate
                                            ? {
                                                ...prev.candidate,
                                                resolvedTag,
                                              }
                                            : prev.candidate,
                                        }))
                                      }}
                                    >
                                      {candidateDisplayTag}
                                    </VersionTagsPopover>
                                  </>
                                ) : null}
                              </div>
	                            {showRawTag ? (
                                <div>
                                  <CurrentVersionPopover
                                    serviceId={svc.id}
                                    displayTag={svc.image.tag}
                                    imageTag={svc.image.tag}
                                    imageDigest={svc.image.digest ?? null}
                                    resolvedTag={svc.image.resolvedTag}
                                    resolvedTags={svc.image.resolvedTags}
                                    onLocalResolvedTags={(update) => {
                                      patchServiceInStackDetails(st.id, svc.id, (prev) => ({
                                        ...prev,
                                        image: {
                                          ...prev.image,
                                          resolvedTag: update.resolvedTag,
                                          resolvedTags: update.resolvedTags,
                                        },
                                      }))
                                    }}
                                    preferSource="rawTag"
                                    triggerClassName="versionTagsTrigger mono monoSecondary"
                                  >
                                    {svc.image.tag}
                                  </CurrentVersionPopover>
                                </div>
	                            ) : null}
	                          </div>
	                          <StatusRemark service={svc} status={stt} />
                          <div
                            className="actionCell"
                            onClick={(e) => e.stopPropagation()}
                            onKeyDown={(e) => e.stopPropagation()}
                          >
                            {isDockrev ? (
                              <div className="actionStack">
                                <Button
                                  variant="ghost"
                                  disabled={busy || supervisor.state.status !== 'ok'}
                                  title={
                                    supervisor.state.status === 'offline'
                                      ? `自我升级不可用（supervisor offline） · ${supervisor.state.errorAt} · ${supervisor.state.error}`
                                      : supervisor.state.status === 'checking'
                                        ? '检查 supervisor 中…'
                                        : undefined
                                  }
                                  onClick={() => {
                                    window.location.href = selfUpgradeUrl
                                  }}
                                >
                                  升级 Dockrev
                                </Button>
                                {supervisor.state.status !== 'ok' ? (
                                  <Button
                                    variant="ghost"
                                    disabled={busy || supervisor.state.status === 'checking'}
                                    onClick={() => {
                                      void supervisor.check()
                                    }}
                                  >
                                    重试
                                  </Button>
                                ) : null}
                                {supervisor.state.status === 'offline' ? (
                                  <div className="muted">
                                    supervisor offline · {supervisor.state.errorAt} · <Mono>{supervisor.state.error}</Mono>
                                  </div>
                                ) : null}
                              </div>
                            ) : (
                              <Button
                                variant="ghost"
                                disabled={
                                  svcApplyActiveJob
                                    ? false
                                    : !svcApply.enabled || busy || svcApplySubmitting
                                }
                                loading={svcApplyActionKey ? isTargetBusy(svcApplyActionKey) : false}
                                loadingClickable={Boolean(svcApplyActiveJob)}
                                title={svcApplyActiveJob ? '任务进行中，点击查看任务详情' : (svcApply.title ?? undefined)}
                                hint={svcApplyActiveJob ? '任务进行中，点击查看任务详情' : undefined}
                                onClick={() => {
                                          if (svcApplyActiveJob) {
                                            navigate({ name: 'job', jobId: svcApplyActiveJob.jobId })
                                            return
                                          }
			                                  const body = (
			                                    <>
		                                      <div className="modalLead">将对该服务执行更新（apply）。</div>
		                                      <div className="modalKvGrid">
                                        <div className="modalKvLabel">范围</div>
                                        <div className="modalKvValue">
                                          <Mono>service</Mono>
                                        </div>
                                        <div className="modalKvLabel">目标</div>
                                        <div className="modalKvValue">
                                          <Mono>{`${d.name}/${svc.name}`}</Mono>
	                                        </div>
	                                        <div className="modalKvLabel">镜像</div>
	                                        <div className="modalKvValue">
	                                          {(() => {
	                                            const img = splitImageRef(svc.image.ref)
	                                            const dn = splitImageNameForDisplay(img.name, svc.image.tag)
	                                            return (
	                                              <div className="cellTwoLine">
	                                                <div
	                                                  className="mono monoPrimary monoSplit imageLinkRow"
	                                                  title={dn.suffix ? `${dn.base}${dn.suffix}` : dn.base}
	                                                >
	                                                  <span className="monoSplitBase">{dn.base}</span>
	                                                  <ImageLinkIcons imageRef={svc.image.ref} repoUrl={svc.settings.repoUrl} />
	                                                </div>
	                                                <div className="mono monoSecondary">{img.registry}</div>
	                                              </div>
	                                            )
	                                          })()}
		                                        </div>
		                                        <div className="modalKvLabel">目标版本</div>
		                                        <div className="modalKvValue">
                                          <ConfirmServiceVersionCell
                                            serviceId={svc.id}
                                            imageTag={svc.image.tag}
                                            imageDigest={svc.image.digest ?? null}
                                            resolvedTag={svc.image.resolvedTag}
                                            resolvedTags={svc.image.resolvedTags}
                                            inferenceStatus={svc.versionInference?.status}
                                            candidateTag={svc.candidate?.tag}
                                            candidateDigest={svc.candidate?.digest ?? null}
                                            candidateResolvedTag={svc.candidate?.resolvedTag}
                                            prefetchOnMount={candidatePrefetchOnMount}
                                            onHostResolvedTags={(update) => {
                                              patchServiceInStackDetails(st.id, svc.id, (prev) => ({
                                                ...prev,
                                                image: {
                                                  ...prev.image,
                                                  resolvedTag: update.resolvedTag,
                                                  resolvedTags: update.resolvedTags,
                                                },
                                              }))
                                            }}
                                            onHostCandidateResolvedTag={(resolvedTag) => {
                                              patchServiceInStackDetails(st.id, svc.id, (prev) => ({
                                                ...prev,
                                                candidate: prev.candidate
                                                  ? {
                                                      ...prev.candidate,
                                                      resolvedTag,
                                                    }
                                                  : prev.candidate,
                                              }))
                                            }}
                                          />
	                                        </div>
                                        <div className="modalKvLabel">状态</div>
                                        <div className="modalKvValue">
                                          <Mono>{stt}</Mono>
                                        </div>
                                      </div>
                                      <div className="modalDivider" />
                                    </>
                                  )
		                                  void triggerApply({
		                                    scope: 'service',
		                                    stackId: st.id,
		                                    serviceId: svc.id,
		                                    targetLabel: `service:${d.name}/${svc.name}`,
		                                    buildRequest: async () => ({
		                                      scope: 'service',
		                                      stackId: st.id,
		                                      ...(await buildUpdateServiceTarget(svc)),
		                                      mode: 'apply',
		                                      allowArchMismatch: false,
		                                      backupMode: 'inherit',
		                                    }),
		                                    confirmBody: body,
		                                    confirmTitle: `确认更新服务 ${svc.name}？`,
		                                  })
	                                }}
                              >
                                {svcApplyActiveJob?.status === 'queued'
                                  ? '排队中…'
                                  : svcApplyActiveJob
                                    ? '更新中…'
                                    : svcApplySubmitting
                                      ? '提交中…'
                                      : '执行更新'}
                              </Button>
                            )}
                          </div>
                        </div>
                      )
                    })
                  : null}
              </div>
            )
          })}
        </div>
      </div>

      {error ? <div className="error">{error}</div> : null}
      {noticeJobId ? (
        <div className="success">
          已创建更新任务 <Mono>{noticeJobId}</Mono> ·{' '}
          <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: 'queue' })}>
            查看队列
          </Button>
        </div>
      ) : null}
      {noticeDiscoveryJobId ? (
        <div className="success">
          已创建扫描任务 <Mono>{noticeDiscoveryJobId}</Mono> ·{' '}
          <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: 'queue' })}>
            查看队列
          </Button>
        </div>
      ) : null}
      {noticeCheckJobId ? (
        <div className="success">
          扫描任务 <Mono>{noticeCheckJobId}</Mono> ·{' '}
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => navigate({ name: 'job', jobId: noticeCheckJobId })}
          >
            查看任务
          </Button>
        </div>
      ) : null}
      {busy ? <div className="muted">处理中…</div> : null}
    </div>
  )
}
