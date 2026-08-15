import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { ApiError, getJob, listJobs, type JobListItem } from './api'
import { useManagementEventBatch, type ManagementEvent } from './managementEvents'

export const UPDATE_JOB_SETTLED_EVENT = 'dockrev:update-job-settled'

export type UpdateActionTargetKey = 'all' | `stack:${string}` | `service:${string}`
export type UpdateActionJobStatus = 'queued' | 'running' | string

export type ActiveUpdateJob = {
  jobId: string
  status: UpdateActionJobStatus
  targetVersion?: string | null
}

type HydratedActiveUpdateJob = ActiveUpdateJob & {
  target: UpdateActionTargetKey
}

export type UpdateJobSettledDetail = {
  target: UpdateActionTargetKey
  jobId: string
  status: string
  scope: string
  stackId?: string | null
  serviceId?: string | null
  summary: unknown
}

type UpdateActionTracker = {
  beginSubmitting: (target: UpdateActionTargetKey) => void
  endSubmitting: (target: UpdateActionTargetKey) => void
  trackJob: (target: UpdateActionTargetKey, jobId: string, status?: UpdateActionJobStatus, targetVersion?: string | null) => void
  isTargetBusy: (target: UpdateActionTargetKey) => boolean
  getActiveJobByTarget: (target: UpdateActionTargetKey) => ActiveUpdateJob | null
  isTargetSubmitting: (target: UpdateActionTargetKey) => boolean
}

const UpdateActionTrackingContext = createContext<UpdateActionTracker | null>(null)

export function resolveUpdateActionTargetKey(
  scope: string,
  stackId?: string | null,
  serviceId?: string | null,
): UpdateActionTargetKey | null {
  if (scope === 'all') return 'all'
  if (scope === 'stack') {
    const id = (stackId ?? '').trim()
    return id ? `stack:${id}` : null
  }
  if (scope === 'service') {
    const id = (serviceId ?? '').trim()
    return id ? `service:${id}` : null
  }
  return null
}

export function isUpdateJobActiveStatus(status: string): boolean {
  return status === 'queued' || status === 'running'
}

export function resolveActiveUpdateJobStatus(
  current: UpdateActionJobStatus,
  incoming: UpdateActionJobStatus,
): UpdateActionJobStatus {
  if (current === 'running' && incoming === 'queued') return current
  return incoming
}

export function isUpdateJobSnapshotCurrent(requestRevision: number, currentRevision: number): boolean {
  return requestRevision === currentRevision
}

function resolveUpdateJobRecency(job: Pick<JobListItem, 'createdAt' | 'startedAt' | 'progress'>): number {
  const recencyCandidates = [job.startedAt, job.createdAt, job.progress?.updatedAt]
  for (const value of recencyCandidates) {
    if (typeof value !== 'string') continue
    const parsed = Date.parse(value)
    if (Number.isFinite(parsed)) return parsed
  }
  return Number.NEGATIVE_INFINITY
}

function resolveUpdateJobTargetVersion(summary: unknown): string | null {
  if (!summary || typeof summary !== 'object' || Array.isArray(summary)) return null
  const value = summary as Record<string, unknown>
  for (const key of ['targetDisplayTag', 'targetTag', 'to']) {
    if (typeof value[key] === 'string' && value[key].trim()) return value[key].trim()
  }
  return null
}

export function pickLatestActiveUpdateJobs(
  jobs: Array<
    Pick<JobListItem, 'id' | 'type' | 'scope' | 'stackId' | 'serviceId' | 'status' | 'createdAt' | 'startedAt' | 'progress' | 'summary'>
  >,
): HydratedActiveUpdateJob[] {
  const latestByTarget = new Map<
    UpdateActionTargetKey,
    {
      job: HydratedActiveUpdateJob
      recency: number
    }
  >()

  for (const job of jobs) {
    if (job.type !== 'update' || !isUpdateJobActiveStatus(job.status)) continue
    const target = resolveUpdateActionTargetKey(job.scope, job.stackId, job.serviceId)
    if (!target) continue

    const targetVersion = resolveUpdateJobTargetVersion(job.summary)
    const candidate = {
      target,
      jobId: job.id,
      status: job.status,
      ...(targetVersion ? { targetVersion } : {}),
    } satisfies HydratedActiveUpdateJob
    const recency = resolveUpdateJobRecency(job)
    const existing = latestByTarget.get(target)
    if (!existing) {
      latestByTarget.set(target, { job: candidate, recency })
      continue
    }

    if (recency > existing.recency || (recency === existing.recency && candidate.jobId > existing.job.jobId)) {
      latestByTarget.set(target, { job: candidate, recency })
    }
  }

  return Array.from(latestByTarget.values(), (entry) => entry.job)
}

type SettledUpdateJob = Pick<JobListItem, 'id' | 'scope' | 'stackId' | 'serviceId' | 'status' | 'summary'>

function toUpdateJobSettledDetail(target: UpdateActionTargetKey, job: SettledUpdateJob): UpdateJobSettledDetail {
  return {
    target,
    jobId: job.id,
    status: job.status,
    scope: job.scope,
    stackId: job.stackId ?? null,
    serviceId: job.serviceId ?? null,
    summary: job.summary,
  }
}

export function publishUpdateJobSettled(detail: UpdateJobSettledDetail) {
  if (typeof window === 'undefined') return
  window.dispatchEvent(new CustomEvent<UpdateJobSettledDetail>(UPDATE_JOB_SETTLED_EVENT, { detail }))
}

function toMissingUpdateJobSettledDetail(target: UpdateActionTargetKey, jobId: string): UpdateJobSettledDetail {
  if (target === 'all') return { target, jobId, status: 'missing', scope: 'all', summary: null }
  if (target.startsWith('stack:')) {
    return { target, jobId, status: 'missing', scope: 'stack', stackId: target.slice(6), summary: null }
  }
  return { target, jobId, status: 'missing', scope: 'service', serviceId: target.slice(8), summary: null }
}

export type UpdateJobSnapshotReconciliation = {
  active: HydratedActiveUpdateJob[]
  settled: Array<{ target: UpdateActionTargetKey; job: SettledUpdateJob }>
  unresolved: Array<{ target: UpdateActionTargetKey; jobId: string }>
}

export function resolveTrackedUpdateJobTransition(
  event: ManagementEvent,
  trackedJobs: Iterable<[UpdateActionTargetKey, ActiveUpdateJob]>,
): { target: UpdateActionTargetKey; jobId: string; status: UpdateActionJobStatus } | null {
  if (event.domain !== 'jobs') return null
  const jobId = typeof event.summary.jobId === 'string' ? event.summary.jobId : null
  const status = typeof event.summary.status === 'string' ? event.summary.status : null
  const terminal = event.summary.terminal === true
  if (!jobId || !status || (!isUpdateJobActiveStatus(status) && !terminal)) return null
  const tracked = Array.from(trackedJobs).find(([, job]) => job.jobId === jobId)
  if (!tracked) return null
  return { target: tracked[0], jobId, status }
}

export function reconcileTrackedUpdateJobs(
  trackedJobs: Iterable<[UpdateActionTargetKey, ActiveUpdateJob]>,
  jobs: JobListItem[],
): UpdateJobSnapshotReconciliation {
  const active = pickLatestActiveUpdateJobs(jobs)
  const jobsById = new Map(jobs.map((job) => [job.id, job]))
  const settled: UpdateJobSnapshotReconciliation['settled'] = []
  const unresolved: UpdateJobSnapshotReconciliation['unresolved'] = []

  for (const [target, tracked] of trackedJobs) {
    const snapshot = jobsById.get(tracked.jobId)
    if (!snapshot) {
      unresolved.push({ target, jobId: tracked.jobId })
      continue
    }
    if (!isUpdateJobActiveStatus(snapshot.status)) settled.push({ target, job: snapshot })
  }

  return { active, settled, unresolved }
}

function useProvideUpdateActionTracker(): UpdateActionTracker {
  const [submittingCounts, setSubmittingCounts] = useState<Record<string, number>>({})
  const [activeByTarget, setActiveByTarget] = useState<Record<string, ActiveUpdateJob>>({})
  const activeByTargetRef = useRef(new Map<UpdateActionTargetKey, ActiveUpdateJob>())
  const activeRevisionRef = useRef(0)
  const unmountedRef = useRef(false)

  const publishActive = useCallback(() => {
    if (unmountedRef.current) return
    const next: Record<string, ActiveUpdateJob> = {}
    for (const [target, job] of activeByTargetRef.current.entries()) next[target] = job
    setActiveByTarget(next)
  }, [])

  const clearRunningJob = useCallback(
    (target: UpdateActionTargetKey, jobId: string) => {
      const current = activeByTargetRef.current.get(target)
      if (!current || current.jobId !== jobId) return
      activeByTargetRef.current.delete(target)
      activeRevisionRef.current += 1
      publishActive()
    },
    [publishActive],
  )

  useEffect(() => {
    return () => {
      unmountedRef.current = true
    }
  }, [])

  const beginSubmitting = useCallback((target: UpdateActionTargetKey) => {
    setSubmittingCounts((prev) => ({
      ...prev,
      [target]: (prev[target] ?? 0) + 1,
    }))
  }, [])

  const endSubmitting = useCallback((target: UpdateActionTargetKey) => {
    setSubmittingCounts((prev) => {
      const nextCount = (prev[target] ?? 0) - 1
      if (nextCount > 0) return { ...prev, [target]: nextCount }
      if (!(target in prev)) return prev
      const next = { ...prev }
      delete next[target]
      return next
    })
  }, [])

  const storeTrackedJob = useCallback(
    (target: UpdateActionTargetKey, jobId: string, status: UpdateActionJobStatus = 'queued', targetVersion?: string | null) => {
      const previous = activeByTargetRef.current.get(target)
      const resolvedTargetVersion = targetVersion ?? previous?.targetVersion ?? null
      const resolvedStatus = previous?.jobId === jobId
        ? resolveActiveUpdateJobStatus(previous.status, status)
        : status
      activeByTargetRef.current.set(
        target,
        resolvedTargetVersion
          ? { jobId, status: resolvedStatus, targetVersion: resolvedTargetVersion }
          : { jobId, status: resolvedStatus },
      )
      activeRevisionRef.current += 1
      publishActive()
    },
    [publishActive],
  )

  const refreshTrackedJob = useCallback(
    async (target: UpdateActionTargetKey, jobId: string) => {
      try {
        const job = await getJob(jobId)
        if (unmountedRef.current) return
        const latest = activeByTargetRef.current.get(target)
        if (!latest || latest.jobId !== jobId) return
        if (isUpdateJobActiveStatus(job.status)) {
          const resolvedStatus = resolveActiveUpdateJobStatus(latest.status, job.status)
          if (latest.status !== resolvedStatus) storeTrackedJob(target, jobId, resolvedStatus)
          return
        }
        publishUpdateJobSettled(toUpdateJobSettledDetail(target, job))
        clearRunningJob(target, jobId)
      } catch {
        // Management events and reconnect snapshots remain the recovery path.
      }
    },
    [clearRunningJob, storeTrackedJob],
  )

  const trackJob = useCallback(
    (target: UpdateActionTargetKey, jobId: string, status: UpdateActionJobStatus = 'queued', targetVersion?: string | null) => {
      storeTrackedJob(target, jobId, status, targetVersion)
      void refreshTrackedJob(target, jobId)
    },
    [refreshTrackedJob, storeTrackedJob],
  )

  useManagementEventBatch(({ events, resyncRequired }) => {
    const active = Array.from(activeByTargetRef.current.entries())
    for (const event of events) {
      const transition = resolveTrackedUpdateJobTransition(event, active)
      if (!transition) continue
      const { target, jobId, status } = transition
      if (isUpdateJobActiveStatus(status)) {
        storeTrackedJob(target, jobId, status)
      } else {
        void refreshTrackedJob(target, jobId)
      }
    }
    if (!resyncRequired) return
    const requestRevision = activeRevisionRef.current
    void listJobs()
      .then((jobs) => {
        if (unmountedRef.current || !isUpdateJobSnapshotCurrent(requestRevision, activeRevisionRef.current)) return
        const reconciliation = reconcileTrackedUpdateJobs(activeByTargetRef.current.entries(), jobs)
        for (const job of reconciliation.active) {
          storeTrackedJob(job.target, job.jobId, job.status)
        }
        for (const { target, job } of reconciliation.settled) {
          publishUpdateJobSettled(toUpdateJobSettledDetail(target, job))
          clearRunningJob(target, job.id)
        }
        for (const { target, jobId } of reconciliation.unresolved) {
          void getJob(jobId)
            .then((job) => {
              if (unmountedRef.current || isUpdateJobActiveStatus(job.status)) return
              publishUpdateJobSettled(toUpdateJobSettledDetail(target, job))
              clearRunningJob(target, jobId)
            })
            .catch((error: unknown) => {
              if (!(error instanceof ApiError) || error.status !== 404) return
              publishUpdateJobSettled(toMissingUpdateJobSettledDetail(target, jobId))
              clearRunningJob(target, jobId)
            })
        }
      })
      .catch(() => {})
  })

  useEffect(() => {
    let cancelled = false

    void (async () => {
      try {
        const requestRevision = activeRevisionRef.current
        const jobs = await listJobs()
        if (
          cancelled
          || unmountedRef.current
          || !isUpdateJobSnapshotCurrent(requestRevision, activeRevisionRef.current)
        ) return
        const hydratedJobs = pickLatestActiveUpdateJobs(jobs)
        for (const job of hydratedJobs) {
          if (activeByTargetRef.current.has(job.target)) continue
          storeTrackedJob(job.target, job.jobId, job.status)
        }
      } catch {
        // Hydration is best-effort; normal click tracking still works without it.
      }
    })()

    return () => {
      cancelled = true
    }
  }, [storeTrackedJob])

  const isTargetBusy = useCallback(
    (target: UpdateActionTargetKey): boolean => {
      return Boolean((submittingCounts[target] ?? 0) > 0 || activeByTarget[target] != null)
    },
    [activeByTarget, submittingCounts],
  )

  const getActiveJobByTarget = useCallback(
    (target: UpdateActionTargetKey): ActiveUpdateJob | null => {
      return activeByTarget[target] ?? null
    },
    [activeByTarget],
  )

  const isTargetSubmitting = useCallback(
    (target: UpdateActionTargetKey): boolean => {
      return (submittingCounts[target] ?? 0) > 0
    },
    [submittingCounts],
  )

  return useMemo(
    () => ({
      beginSubmitting,
      endSubmitting,
      trackJob,
      isTargetBusy,
      getActiveJobByTarget,
      isTargetSubmitting,
    }),
    [beginSubmitting, endSubmitting, getActiveJobByTarget, isTargetBusy, isTargetSubmitting, trackJob],
  )
}

export function UpdateActionTrackerProvider(props: { children: ReactNode }) {
  const tracker = useProvideUpdateActionTracker()
  return createElement(UpdateActionTrackingContext.Provider, { value: tracker }, props.children)
}

export function useUpdateActionTracker(): UpdateActionTracker {
  const tracker = useContext(UpdateActionTrackingContext)
  if (!tracker) {
    throw new Error('useUpdateActionTracker must be used within UpdateActionTrackerProvider')
  }
  return tracker
}
