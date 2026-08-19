import { useCallback,useEffect,useMemo,useRef,useState,type ReactNode } from 'react'
import {
DOCKREV_AGGREGATE_GUARD_HINT,
readUpdateGuardBlockedReason,
resolveAggregateUpdateActionState,
} from '../aggregateUpdateGuard'
import {
ApiError,
  getStack,
listDiscoveryProjects,
listCompactJobsPage,
listStacks,
triggerCheck,
triggerDiscoveryScan,
triggerUpdate,
type DiscoveredProject,
type CompactJobListItem,
type Service,
type StackDetail,
type StackListItem,
type TriggerUpdateInput
} from '../api'
import { AggregateUpdatePreviewList } from '../components/AggregateUpdatePreviewList'
import { normalizeDigest } from '../components/digest'
import { imageRepoFromImageRef } from '../imageRepo'
import { type UpdateCandidateFilter } from '../components/UpdateCandidateFilters'
import { useConfirm } from '../confirm'
import { navigate } from '../routes'
import { selfUpgradeBaseUrl } from '../runtimeConfig'
import {
Button,
Mono
} from '../ui'
import {
resolveUpdateActionTargetKey,
UPDATE_JOB_SETTLED_EVENT,
useUpdateActionTracker,
type UpdateJobSettledDetail,
} from '../updateActionTracking'
import { isSemverDowngradeAnomaly,serviceRowStatus,type RowStatus } from '../updateStatus'
import { buildUpdateServiceTargets } from '../updateTargets'
import { useManagementEventBatch } from '../managementEvents'
import { usePwaStatus } from '../pwaStatus'
import { buildReadonlySnapshotKey, readReadonlySnapshot, writeReadonlySnapshot } from '../readonlySnapshotCache'
import { useSupervisorHealth } from '../useSupervisorHealth'
import { buildAllAggregateScope } from './aggregateUpdateScope'
import { selectOverviewJobsForCard,toOverviewJobCardItem } from './overviewJobsCard'

import {
buildDiscoveryIssue,
DISCOVERY_ISSUE_ORDER,
type DiscoveryIssueItem,
latestDiscoveryObservationAt,
readCollapsedFromStorage,
readUpdateCandidateFilterFromUrl,
withCollapseDefaults,
writeCollapsedToStorage,
writeUpdateCandidateFilterToUrl,
} from './overviewHelpers'

const SERVICES_OVERVIEW_SNAPSHOT_KEY = buildReadonlySnapshotKey('services', 'operations-dashboard')
const SERVICES_OVERVIEW_SNAPSHOT_STALE_MS = 60_000

type ServicesOverviewSnapshotPayload = {
  version: 2
  readiness: {
    stacks: boolean
    jobs: boolean
    discovery: boolean
  }
  committedQueryKey: string
  stacks: StackListItem[]
  details: Record<string, StackDetail | undefined>
  jobs: CompactJobListItem[]
  discoveredProjects: DiscoveredProject[]
}

function isServicesOverviewSnapshotPayload(value: unknown): value is ServicesOverviewSnapshotPayload {
  if (!value || typeof value !== 'object') return false
  const payload = value as Record<string, unknown>
  if (payload.version !== 2 || typeof payload.committedQueryKey !== 'string' || !payload.readiness || typeof payload.readiness !== 'object') return false
  const readiness = payload.readiness as Record<string, unknown>
  return Array.isArray(payload.stacks) && Array.isArray(payload.jobs) && Array.isArray(payload.discoveredProjects) && readiness.stacks === true && readiness.jobs === true && readiness.discovery === true
}

export function useOverviewPageState(props: {
  onLastScanHint: (lastScan?: string) => void
  onTopActions: (node: ReactNode) => void
}) {

  const { onLastScanHint, onTopActions } = props
  const { isOnline } = usePwaStatus()
  const confirm = useConfirm()
  const [filter, setFilter] = useState<UpdateCandidateFilter>(() => readUpdateCandidateFilterFromUrl() ?? 'all')
  const [candidateSearch, setCandidateSearch] = useState('')
  const [stacks, setStacks] = useState<StackListItem[]>([])
  const [details, setDetails] = useState<Record<string, StackDetail | undefined>>({})
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>(() => {
    const initialFilter = readUpdateCandidateFilterFromUrl() ?? 'all'
    return readCollapsedFromStorage(initialFilter)
  })
  const [jobs, setJobs] = useState<CompactJobListItem[]>([])
  const [discoveredProjects, setDiscoveredProjects] = useState<DiscoveredProject[]>([])
  const [stacksLoaded, setStacksLoaded] = useState(false)
  const [stackDetailsLoaded, setStackDetailsLoaded] = useState(false)
  const [jobsLoaded, setJobsLoaded] = useState(false)
  const [discoveryLoaded, setDiscoveryLoaded] = useState(false)
  const [activeDiscoveryIssue, setActiveDiscoveryIssue] = useState<DiscoveryIssueItem | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [noticeJobId, setNoticeJobId] = useState<string | null>(null)
  const [noticeDiscoveryJobId, setNoticeDiscoveryJobId] = useState<string | null>(null)
  const [noticeCheckJobId, setNoticeCheckJobId] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [snapshotStatus, setSnapshotStatus] = useState<'missing' | 'fresh' | 'stale' | 'expired' | 'unsupported'>('missing')
  const [snapshotFetchedAt, setSnapshotFetchedAt] = useState<string | null>(null)
  const [snapshotAnchorFetchedAt, setSnapshotAnchorFetchedAt] = useState<string | null>(null)
  const [snapshotActive, setSnapshotActive] = useState(false)
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
  const readonlyOffline = !isOnline

  useEffect(() => {
    let cancelled = false
    void (async () => {
      const snapshot = await readReadonlySnapshot<ServicesOverviewSnapshotPayload>(SERVICES_OVERVIEW_SNAPSHOT_KEY)
      if (cancelled) return
      setSnapshotStatus(snapshot.status)
      setSnapshotFetchedAt(snapshot.record?.fetchedAt ?? null)
      setSnapshotAnchorFetchedAt(snapshot.record?.fetchedAt ?? null)
      if (snapshot.status !== 'fresh' || !isServicesOverviewSnapshotPayload(snapshot.record.payload)) return
      const payload = snapshot.record.payload
      setStacks(payload.stacks)
      setDetails(payload.details)
      setJobs(payload.jobs)
      setDiscoveredProjects(payload.discoveredProjects)
      setStacksLoaded(true)
      setStackDetailsLoaded(true)
      setJobsLoaded(true)
      setDiscoveryLoaded(true)
      const maxLastScan = payload.stacks.map((item) => item.lastCheckAt).sort().at(-1)
      onLastScanHint(maxLastScan)
      setSnapshotActive(true)
    })()
    return () => {
      cancelled = true
    }
  }, [onLastScanHint])

  const lastDiscoveryScanAt = useMemo(() => {
    const candidates = jobs
      .filter((j) => j.type === 'discovery' && j.status === 'success')
      .sort((a, b) => String(b.finishedAt ?? b.createdAt ?? '').localeCompare(String(a.finishedAt ?? a.createdAt ?? '')))
    const j = candidates[0]
    if (!j) return null
    return j.progress?.updatedAt ?? j.finishedAt ?? j.createdAt ?? null
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
      const jobsPromise = listCompactJobsPage({ limit: 200 })
      const projectsPromise = listDiscoveryProjects('exclude')

      const [stacksRes, jobsRes, projectsRes] = await Promise.allSettled([stacksPromise, jobsPromise, projectsPromise])

      if (jobsRes.status === 'rejected') errors.push('jobs unavailable')
      if (projectsRes.status === 'rejected') errors.push('discovery projects unavailable')
      if (stacksRes.status === 'rejected') throw stacksRes.reason

      const s = stacksRes.value
      const maxLastScan = s.map((x) => x.lastCheckAt).sort().at(-1)

      if (jobsRes.status === 'fulfilled' && requestId >= latestAppliedJobsRequestIdRef.current) {
        latestAppliedJobsRequestIdRef.current = requestId
        setJobs(jobsRes.value.jobs)
        setJobsLoaded(true)
      }
      if (projectsRes.status === 'fulfilled' && requestId >= latestAppliedProjectsRequestIdRef.current) {
        latestAppliedProjectsRequestIdRef.current = requestId
        setDiscoveredProjects(projectsRes.value)
        setDiscoveryLoaded(true)
      }
      if (requestId < latestAppliedStacksRequestIdRef.current) return
      latestAppliedStacksRequestIdRef.current = requestId
      setStacks(s)
      setStacksLoaded(true)
      onLastScanHint(maxLastScan)
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
      setStackDetailsLoaded(results.every(([, detail]) => detail !== undefined))
      if (errors.length === 0 && results.every(([, detail]) => detail !== undefined)) {
        setSnapshotActive(false)
        setSnapshotAnchorFetchedAt(null)
      }
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
    setStackDetailsLoaded(results.every(([, detail]) => detail !== undefined))
  }, [])

  const patchStackList = useCallback(
    async (stackIds: string[]) => {
      const ids = new Set(stackIds.map((id) => id.trim()).filter(Boolean))
      if (ids.size === 0) return

      const next = await listStacks()
      const maxLastScan = next.map((item) => item.lastCheckAt).sort().at(-1)

      setStacks(next)
      setStacksLoaded(true)
      onLastScanHint(maxLastScan)
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

  const requestRefresh = refresh

  useEffect(() => {
    void requestRefresh().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
  }, [requestRefresh])

  useEffect(() => {
    if (!stacksLoaded || !stackDetailsLoaded || !jobsLoaded || !discoveryLoaded) return
    void writeReadonlySnapshot(
      SERVICES_OVERVIEW_SNAPSHOT_KEY,
      {
        version: 2,
        readiness: { stacks: true, jobs: true, discovery: true },
        committedQueryKey: filter,
        stacks,
        details,
        jobs,
        discoveredProjects,
      },
      {
        staleAfterMs: SERVICES_OVERVIEW_SNAPSHOT_STALE_MS,
        fetchedAt: snapshotAnchorFetchedAt ? Date.parse(snapshotAnchorFetchedAt) || undefined : undefined,
      },
    )
  }, [details, discoveredProjects, discoveryLoaded, filter, jobs, jobsLoaded, snapshotAnchorFetchedAt, stackDetailsLoaded, stacks, stacksLoaded])

  useManagementEventBatch(({ events, resyncRequired }) => {
    const stackIds = new Set<string>()
    const jobsChanged = resyncRequired || events.some((event) => event.domain === 'jobs')
    const discoveryChanged = resyncRequired || events.some((event) => event.domain === 'discovery')
    let refreshAll = resyncRequired
    for (const event of events) {
      if (!['jobs', 'stacks', 'services', 'discovery', 'version_inference'].includes(event.domain)) continue
      if (event.summary.scope === 'all') refreshAll = true
      for (const entity of event.entities) {
        if (entity.entityType === 'stack') stackIds.add(entity.id)
        if (entity.entityType === 'service') {
          for (const [stackId, detail] of Object.entries(details)) {
            if (detail?.services.some((service) => service.id === entity.id)) {
              stackIds.add(stackId)
            }
          }
        }
      }
      if (typeof event.summary.stackId === 'string') stackIds.add(event.summary.stackId)
      if (event.domain === 'version_inference' && event.summary.phase === 'finished') {
        const imageRepo = typeof event.summary.imageRepo === 'string'
          ? event.summary.imageRepo.trim().toLowerCase()
          : ''
        const digest = typeof event.summary.digest === 'string'
          ? normalizeDigest(event.summary.digest)?.toLowerCase()
          : null
        if (!imageRepo || !digest) continue
        for (const [stackId, detail] of Object.entries(details)) {
          if (detail?.services.some((service) => {
            const serviceRepo = imageRepoFromImageRef(service.image.ref)
            const currentDigest = normalizeDigest(service.image.digest)?.toLowerCase()
            const candidateDigest = normalizeDigest(service.candidate?.digest)?.toLowerCase()
            return serviceRepo === imageRepo && (currentDigest === digest || candidateDigest === digest)
          })) stackIds.add(stackId)
        }
      }
    }
    const sync = async () => {
      if (refreshAll) return requestRefresh()
      const tasks: Promise<unknown>[] = []
      if (jobsChanged) tasks.push(listCompactJobsPage({ limit: 200 }).then((page) => {
        setJobs(page.jobs)
        setJobsLoaded(true)
      }))
      if (discoveryChanged) tasks.push(listDiscoveryProjects('exclude').then((projects) => {
        setDiscoveredProjects(projects)
        setDiscoveryLoaded(true)
      }))
      if (stackIds.size > 0) {
        const ids = [...stackIds]
        tasks.push(patchStackDetails(ids), patchStackList(ids))
      }
      await Promise.all(tasks)
    }
    void sync().catch((error: unknown) => setError(error instanceof Error ? error.message : String(error)))
  })

  useEffect(() => {
    const onUpdateJobSettled = (evt: Event) => {
      const detail = evt instanceof CustomEvent ? (evt.detail as UpdateJobSettledDetail | null) : null
      if (!detail) return

      const isAll = detail.scope === 'all' || detail.target === 'all'
      const stackIds = resolveSettledStackIds(detail)
      if (isAll || stackIds.length === 0) {
        void requestRefresh().catch((error: unknown) => setError(error instanceof Error ? error.message : String(error)))
        return
      }

      void Promise.all([patchStackDetails(stackIds), patchStackList(stackIds)])
        .catch((error: unknown) => setError(error instanceof Error ? error.message : String(error)))
    }

    window.addEventListener(UPDATE_JOB_SETTLED_EVENT, onUpdateJobSettled)
    return () => {
      window.removeEventListener(UPDATE_JOB_SETTLED_EVENT, onUpdateJobSettled)
    }
  }, [patchStackDetails, patchStackList, requestRefresh, resolveSettledStackIds])

  const applyFilter = useCallback(
    (next: UpdateCandidateFilter, mode: 'push' | 'replace') => {
      setFilter(next)
      writeUpdateCandidateFilterToUrl(next, mode)
      setCollapsed(withCollapseDefaults(readCollapsedFromStorage(next), stacks, details, next))
    },
    [details, stacks],
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
      setCollapsed(withCollapseDefaults(readCollapsedFromStorage(next), stacks, details, next))
    }
    window.addEventListener('popstate', onNav)
    window.addEventListener('hashchange', onNav)
    return () => {
      window.removeEventListener('popstate', onNav)
      window.removeEventListener('hashchange', onNav)
    }
  }, [details, filter, stacks])

  useEffect(() => {
    setCollapsed((prev) => {
      const next = withCollapseDefaults(prev, stacks, details, filter)
      const prevKeys = Object.keys(prev)
      const nextKeys = Object.keys(next)
      if (prevKeys.length !== nextKeys.length) return next
      for (const key of nextKeys) {
        if (next[key] !== prev[key]) return next
      }
      return prev
    })
  }, [details, filter, stacks])

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
    const scope = buildAllAggregateScope({
      stacks,
      details,
      filter,
      candidateSearch,
    })
    return {
      counts: scope.counts,
      actionablePreviewItems: scope.previewItems.filter((item) => !item.guardedDockrev),
      guardedPreviewItems: scope.previewItems.filter((item) => item.guardedDockrev),
      actionableCount: scope.actionableCount,
      visibleServiceCount: scope.visibleServiceCount,
      totalServiceCount: scope.totalServiceCount,
      isFilteredSubset: scope.isFilteredSubset,
      actionableServices: scope.actionableServices,
    }
  }, [candidateSearch, details, filter, stacks])

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
    const stopped = discoveredProjects.filter((p) => p.status === 'stopped' && !p.archived)
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
      stopped: stopped.slice(0, 6),
      stoppedCount: stopped.length,
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
          <div className="modalLead">发现扫描会拉取 discovery projects，并标记 stopped、missing 或 invalid。</div>
          <div className="modalKvGrid">
            <div className="modalKvLabel">操作</div>
            <div className="modalKvValue">
              <Mono>discovery scan</Mono>
            </div>
            <div className="modalKvLabel">可能影响</div>
            <div className="modalKvValue">创建/更新 stacks，或更新其运行、停止、缺失或配置异常状态。</div>
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
            const guardReason = readUpdateGuardBlockedReason(e)
            if (guardReason) setError(guardReason)
            else {
              setError('扫描结果已变化，请刷新并重新扫描后再更新')
              await requestRefresh()
            }
          } else setError(e.message)
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
          disabled={busy || readonlyOffline}
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
        <div className="actionStack">
          <Button
            variant="danger"
            disabled={
              allApplyActiveJob
                ? false
                : !allApply.enabled || busy || allApplySubmitting || readonlyOffline
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
              const totalCandidates = aggregateAll.actionableCount
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
                    aggregateAll.actionableServices,
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
        </div>
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
      readonlyOffline,
	    requestRefresh,
	    triggerApply,
	  ])
  return {
    activeDiscoveryIssue,
    busy,
    candidateSearch,
    collapsed,
    countsAll,
    details,
    discoveryLoaded,
    discoverySummary,
    effectiveDiscoveryScanAt,
    error,
    filter,
    getActiveJobByTarget,
    isTargetBusy,
    isTargetSubmitting,
    jobsSummary,
    jobsLoaded,
    noticeCheckJobId,
    noticeDiscoveryJobId,
    noticeJobId,
    readonlyOffline,
    onChangeFilter,
    overviewCardJobs,
    patchServiceInStackDetails,
    requestRefresh,
    runDiscoveryScan,
    selfUpgradeUrl,
    setCandidateSearch,
    setActiveDiscoveryIssue,
    snapshotActive,
    snapshotFetchedAt,
    snapshotStatus,
    stacks,
    stacksLoaded: stacksLoaded && stackDetailsLoaded,
    supervisor,
    toggleStackCollapsed,
    totalServicesAll,
    triggerApply,
  }
}
