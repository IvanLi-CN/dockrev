import { useCallback,useEffect,useMemo,useRef,useState,type ReactNode } from 'react'
import {
DOCKREV_AGGREGATE_GUARD_HINT,
emptyAggregateUpdateCounts,
partitionAggregateUpdateServices,
resolveAggregateUpdateActionState,
} from '../aggregateUpdateGuard'
import {
ApiError,
getJob,
getStack,
listDiscoveryProjects,
listJobs,
listStacks,
newJobEventsSource,
newJobsEventsSource,
triggerCheck,
triggerDiscoveryScan,
triggerRuntimeScan,
triggerUpdate,
type DiscoveredProject,
type JobListItem,
type Service,
type StackDetail,
type StackListItem,
type TriggerUpdateInput
} from '../api'
import { AggregateUpdatePreviewList,type AggregateUpdatePreviewListItem } from '../components/AggregateUpdatePreviewList'
import { normalizeDigest } from '../components/digest'
import { type UpdateCandidateFilter } from '../components/UpdateCandidateFilters'
import { useConfirm } from '../confirm'
import {
DIGEST_SNAPSHOT_UPDATED_EVENT,
type DigestSnapshotUpdatedDetail,
} from '../digestInferenceTracker'
import { imageRepoFromImageRef } from '../imageRepo'
import { navigate } from '../routes'
import { selfUpgradeBaseUrl } from '../runtimeConfig'
import {
Button,
Mono
} from '../ui'
import {
resolveUpdateActionTargetKey,
UPDATE_JOB_SETTLE_RETRY_MS,
UPDATE_JOB_SETTLED_EVENT,
useUpdateActionTracker,
type UpdateJobSettledDetail,
} from '../updateActionTracking'
import { isSemverDowngradeAnomaly,serviceRowStatus,type RowStatus } from '../updateStatus'
import { buildUpdateServiceTargets } from '../updateTargets'
import { usePageResumeRefresh } from '../usePageResumeRefresh'
import { useSupervisorHealth } from '../useSupervisorHealth'
import {
inferResolvedTagsFromSnapshot,
isStrictSemverTag
} from '../versionDisplay'
import { selectOverviewJobsForCard,toOverviewJobCardItem } from './overviewJobsCard'

import {
buildDiscoveryIssue,
DISCOVERY_ISSUE_ORDER,
type DiscoveryIssueItem,
getDiscoveryScanStartedAt,
latestDiscoveryObservationAt,
readCollapsedFromStorage,
readUpdateCandidateFilterFromUrl,
scanHasFailures,
scanIsComplete,
withAggregateDisplayName,
withCollapseDefaults,
writeCollapsedToStorage,
writeUpdateCandidateFilterToUrl,
} from './overviewHelpers'

const OVERVIEW_JOBS_SSE_REFRESH_DEBOUNCE_MS = 180
const OVERVIEW_JOBS_SSE_FALLBACK_POLL_MS = 5000
const OVERVIEW_JOBS_SSE_ERROR_THRESHOLD = 3
const OVERVIEW_JOBS_SSE_RECONNECT_MS = 1500

export function useOverviewPageState(props: {
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
  const [activeDiscoveryIssue, setActiveDiscoveryIssue] = useState<DiscoveryIssueItem | null>(null)
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
        const aStamp = latestDiscoveryObservationAt(a)
        const bStamp = latestDiscoveryObservationAt(b)
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
  return {
    activeDiscoveryIssue,
    busy,
    collapsed,
    countsAll,
    details,
    discoverySummary,
    effectiveDiscoveryScanAt,
    error,
    filter,
    getActiveJobByTarget,
    isTargetBusy,
    isTargetSubmitting,
    jobsSummary,
    noticeCheckJobId,
    noticeDiscoveryJobId,
    noticeJobId,
    onChangeFilter,
    overviewCardJobs,
    patchServiceInStackDetails,
    runDiscoveryScan,
    selfUpgradeUrl,
    setActiveDiscoveryIssue,
    stacks,
    supervisor,
    toggleStackCollapsed,
    totalServicesAll,
    triggerApply,
  }
}
