import { useCallback, useEffect, useRef, useState } from 'react'
import {
  getGitHubPackagesWebhookOverview,
  getVersionInferenceOverview,
  listJobsPage,
  type JobListItem,
} from '../api'
import { useManagementEventBatch } from '../managementEvents'
import { formatJobMachineName, formatJobReadableDisplay } from '../jobDisplay'
import { formatJobProgressDownload } from '../jobProgressDownload'
import { TaskResultReason } from '../components/TaskResultReason'
import { ReadonlySnapshotNotice } from '../components/ReadonlySnapshotNotice'
import { usePwaStatus } from '../pwaStatus'
import {
  buildReadonlySnapshotKey,
  readReadonlySnapshot,
  writeReadonlySnapshot,
} from '../readonlySnapshotCache'
import { navigate } from '../routes'
import { Button, Mono, Pill } from '../ui'
import { AsyncDataRegion, AsyncDataSkeleton } from '../components/AsyncDataRegion'
import { hasCompleteAsyncReadiness, type AsyncDataPhase, type AsyncDataSource, type AsyncDataTrigger } from '../asyncData'

type Filter = 'all' | 'queued' | 'running' | 'success' | 'failed' | 'rolled_back' | 'cancelled'
type VersionInferenceSummary = {
  snapshotsTotal: number
  queued: number
  running: number
  ready: number
  stale: number
  allFailed: number
}

const DEFAULT_VERSION_INFERENCE_SUMMARY: VersionInferenceSummary = {
  snapshotsTotal: 0,
  queued: 0,
  running: 0,
  ready: 0,
  stale: 0,
  allFailed: 0,
}

type GhcrWebhookSummary = {
  tracked: number
  ok: number
  missing: number
  error: number
  conflict: number
  jobsQueued: number
  jobsRunning: number
}

const DEFAULT_GHCR_SUMMARY: GhcrWebhookSummary = {
  tracked: 0,
  ok: 0,
  missing: 0,
  error: 0,
  conflict: 0,
  jobsQueued: 0,
  jobsRunning: 0,
}

function statusTone(status: string): 'ok' | 'warn' | 'bad' | 'muted' | 'info' {
  if (status === 'success') return 'ok'
  if (status === 'rolled_back') return 'warn'
  if (status === 'cancelled') return 'muted'
  if (status === 'failed') return 'bad'
  if (status === 'running') return 'info'
  if (status === 'queued') return 'warn'
  return 'muted'
}

function formatShort(ts?: string | null) {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function formatRunningDuration(startedAt?: string | null): string | null {
  if (!startedAt) return null
  const d = new Date(startedAt)
  if (Number.isNaN(d.valueOf())) return null
  const ms = Date.now() - d.valueOf()
  if (ms <= 0) return null
  const sec = Math.floor(ms / 1000)
  const min = Math.floor(sec / 60)
  const remSec = sec % 60
  if (min >= 60) {
    const h = Math.floor(min / 60)
    const remMin = min % 60
    return `${h}h ${remMin}m`
  }
  if (min > 0) return `${min}m ${remSec}s`
  return `${remSec}s`
}

function formatProgressLabel(job: JobListItem): string | null {
  const m = getProgressMetrics(job)
  if (!m) return null
  const phase = (job.progress?.phase ?? '').trim()
  const message = (job.progress?.message ?? '').trim()
  const target = (job.progress?.currentTarget ?? '').trim()
  const download = formatJobProgressDownload(job.progress?.download)
  const parts: string[] = []
  if (m.plannedTotal > 0 || m.completedTotal > 0) {
    parts.push(`安排 ${m.plannedCurrent}/${m.plannedTotal || '-'} · 完成 ${m.completedCurrent}/${m.completedTotal || '-'}`)
  }
  if (phase) parts.push(phase)
  if (message) parts.push(message)
  if (download) parts.push(`下载 ${download}`)
  if (target) parts.push(target)
  return parts.length > 0 ? parts.join(' · ') : null
}

function getProgressMetrics(job: JobListItem): {
  plannedCurrent: number
  plannedTotal: number
  plannedPercent: number | null
  completedCurrent: number
  completedTotal: number
  completedPercent: number | null
} | null {
  const p = job.progress
  if (!p) return null
  const completedTotal = Number.isFinite(p.total) ? Math.max(0, p.total) : 0
  const completedCurrentRaw = Number.isFinite(p.current) ? Math.max(0, p.current) : 0
  const completedCurrent = Math.min(completedCurrentRaw, completedTotal || completedCurrentRaw)
  const completedPercent =
    completedTotal > 0 && Number.isFinite(p.percent) ? Math.max(0, Math.min(100, Math.round(p.percent))) : null

  const plannedTotalRaw = p.plannedTotal
  const plannedCurrentInput = p.plannedCurrent
  const plannedTotal =
    typeof plannedTotalRaw === 'number' && Number.isFinite(plannedTotalRaw) ? Math.max(0, plannedTotalRaw) : completedTotal
  const plannedCurrentRaw =
    typeof plannedCurrentInput === 'number' && Number.isFinite(plannedCurrentInput)
      ? Math.max(0, plannedCurrentInput)
      : completedCurrent
  const plannedCurrent = Math.min(plannedCurrentRaw, plannedTotal || plannedCurrentRaw)
  const plannedPercent =
    p.plannedPercent === null
      ? null
      : plannedTotal > 0 && Number.isFinite(p.plannedPercent)
        ? Math.max(0, Math.min(100, Math.round(p.plannedPercent ?? 0)))
        : completedPercent

  const normalizedCompletedPercent =
    job.status === 'running' && completedPercent === 0 && completedCurrent < completedTotal ? null : completedPercent
  const normalizedPlannedPercent =
    job.status === 'running' && plannedPercent === 0 && plannedCurrent < plannedTotal ? null : plannedPercent

  return {
    plannedCurrent,
    plannedTotal,
    plannedPercent: normalizedPlannedPercent,
    completedCurrent,
    completedTotal,
    completedPercent: normalizedCompletedPercent,
  }
}

function shouldShowFinishedAt(job: JobListItem): boolean {
  return job.status !== 'running' && Boolean(job.finishedAt)
}

const QUEUE_SNAPSHOT_KEY = buildReadonlySnapshotKey('queue', 'jobs-overview')
const QUEUE_SNAPSHOT_STALE_MS = 60_000

type QueueSnapshotPayload = {
  version: 2
  readiness: {
    jobs: boolean
    versionInference: boolean
    ghcr: boolean
  }
  committedQueryKey: string
  jobs: JobListItem[]
  filter?: Filter
  currentCursor?: string | null
  nextCursor?: string | null
  cursorStack?: (string | null)[]
  versionInferenceSummary: VersionInferenceSummary
  versionInferenceLoaded: boolean
  ghcrSummary: GhcrWebhookSummary
  ghcrLoaded: boolean
}

type QueueRefreshOptions = {
  source?: AsyncDataSource
  trigger?: AsyncDataTrigger
}

function isQueueSnapshotPayload(value: unknown): value is QueueSnapshotPayload {
  if (!isRecord(value) || value.version !== 2 || !isRecord(value.readiness)) return false
  return (
    typeof value.committedQueryKey === 'string' &&
    Array.isArray(value.jobs) &&
    hasCompleteAsyncReadiness(value.readiness, ['jobs', 'versionInference', 'ghcr'])
  )
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null
}

function safeCount(v: unknown): number {
  if (typeof v !== 'number' || !Number.isFinite(v)) return 0
  return Math.max(0, Math.trunc(v))
}

function parseVersionInferenceSummary(data: unknown): VersionInferenceSummary {
  if (!isRecord(data)) return DEFAULT_VERSION_INFERENCE_SUMMARY
  const summary = isRecord(data.summary) ? data.summary : {}
  return {
    snapshotsTotal: safeCount(summary.snapshotsTotal),
    queued: safeCount(summary.queued),
    running: safeCount(summary.running),
    ready: safeCount(summary.ready),
    stale: safeCount(summary.stale),
    allFailed: safeCount(summary.allFailed),
  }
}

function parseGhcrWebhookSummary(data: unknown): GhcrWebhookSummary {
  if (!isRecord(data)) return DEFAULT_GHCR_SUMMARY
  const summary = isRecord(data.summary) ? data.summary : {}
  return {
    tracked: safeCount(summary.tracked),
    ok: safeCount(summary.ok),
    missing: safeCount(summary.missing),
    error: safeCount(summary.error),
    conflict: safeCount(summary.conflict),
    jobsQueued: safeCount(data.jobsQueued),
    jobsRunning: safeCount(data.jobsRunning),
  }
}

function versionInferenceTone(summary: VersionInferenceSummary): 'ok' | 'warn' | 'bad' | 'info' {
  if (summary.running > 0) return 'info'
  if (summary.queued > 0) return 'warn'
  if (summary.stale > 0 || summary.allFailed > 0) return 'bad'
  return 'ok'
}

function versionInferenceLabel(summary: VersionInferenceSummary): string {
  if (summary.running > 0) return 'running'
  if (summary.queued > 0) return 'queued'
  if (summary.stale > 0) return '需处理'
  if (summary.allFailed > 0) return 'all_failed'
  return 'ready'
}

function ghcrTone(summary: GhcrWebhookSummary): 'ok' | 'warn' | 'bad' {
  if (summary.jobsRunning > 0 || summary.jobsQueued > 0) return 'warn'
  if (summary.error > 0 || summary.conflict > 0 || summary.missing > 0) return 'bad'
  return 'ok'
}

function ghcrLabel(summary: GhcrWebhookSummary): string {
  if (summary.jobsRunning > 0) return 'running'
  if (summary.jobsQueued > 0) return 'queued'
  if (summary.error > 0) return 'error'
  if (summary.conflict > 0) return 'conflict'
  if (summary.missing > 0) return 'missing'
  return 'ok'
}

export function QueuePage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const { isOnline } = usePwaStatus()
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [jobsLoaded, setJobsLoaded] = useState(false)
  const [jobsPhase, setJobsPhase] = useState<AsyncDataPhase>('initial-loading')
  const [jobsSource, setJobsSource] = useState<AsyncDataSource>('none')
  const [jobsTrigger, setJobsTrigger] = useState<AsyncDataTrigger>('background')
  const [jobsLiveLoaded, setJobsLiveLoaded] = useState(false)
  const [filter, setFilter] = useState<Filter>('all')
  const [currentCursor, setCurrentCursor] = useState<string | null>(null)
  const [nextCursor, setNextCursor] = useState<string | null>(null)
  const [cursorStack, setCursorStack] = useState<(string | null)[]>([])
  const [paginationBusy, setPaginationBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [versionInferenceSummary, setVersionInferenceSummary] = useState<VersionInferenceSummary>(
    DEFAULT_VERSION_INFERENCE_SUMMARY,
  )
  const [versionInferenceLoaded, setVersionInferenceLoaded] = useState(false)
  const [versionInferenceLiveLoaded, setVersionInferenceLiveLoaded] = useState(false)
  const [versionInferenceError, setVersionInferenceError] = useState<string | null>(null)
  const [ghcrSummary, setGhcrSummary] = useState<GhcrWebhookSummary>(DEFAULT_GHCR_SUMMARY)
  const [ghcrLoaded, setGhcrLoaded] = useState(false)
  const [ghcrLiveLoaded, setGhcrLiveLoaded] = useState(false)
  const [ghcrError, setGhcrError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [, setSnapshotStatus] = useState<'missing' | 'fresh' | 'stale' | 'expired' | 'unsupported'>(
    'missing',
  )
  const [snapshotFetchedAt, setSnapshotFetchedAt] = useState<string | null>(null)
  const [snapshotAnchorFetchedAt, setSnapshotAnchorFetchedAt] = useState<string | null>(null)
  const [snapshotActive, setSnapshotActive] = useState(false)
  const [snapshotHydrated, setSnapshotHydrated] = useState(false)
  const refreshRequestIdRef = useRef(0)
  const jobsLoadedRef = useRef(false)
  const snapshotActiveRef = useRef(false)
  const currentCursorRef = useRef<string | null>(null)
  const filterRef = useRef<Filter>('all')
  const inferenceRequestIdRef = useRef(0)
  const ghcrRequestIdRef = useRef(0)

  useEffect(() => {
    let cancelled = false
    void (async () => {
      const snapshot = await readReadonlySnapshot<QueueSnapshotPayload>(QUEUE_SNAPSHOT_KEY)
      if (cancelled) return
      setSnapshotStatus(snapshot.status)
      setSnapshotFetchedAt(snapshot.record?.fetchedAt ?? null)
      setSnapshotAnchorFetchedAt(snapshot.record?.fetchedAt ?? null)
      if (snapshot.status !== 'fresh' || !isQueueSnapshotPayload(snapshot.record.payload)) {
        setSnapshotHydrated(true)
        return
      }
      const payload = snapshot.record.payload
      setJobs(payload.jobs)
      setJobsLoaded(true)
      jobsLoadedRef.current = true
      const snapshotFilter = payload.filter ?? 'all'
      const snapshotCursor = payload.currentCursor ?? null
      filterRef.current = snapshotFilter
      currentCursorRef.current = snapshotCursor
      setFilter(snapshotFilter)
      setCurrentCursor(snapshotCursor)
      setNextCursor(payload.nextCursor ?? null)
      setCursorStack(payload.cursorStack ?? [])
      setVersionInferenceSummary(payload.versionInferenceSummary)
      setVersionInferenceLoaded(payload.readiness.versionInference)
      setGhcrSummary(payload.ghcrSummary)
      setGhcrLoaded(payload.readiness.ghcr)
      setJobsSource('fresh-snapshot')
      setJobsPhase(payload.jobs.length === 0 ? 'ready-empty' : 'ready-data')
      setSnapshotActive(true)
      snapshotActiveRef.current = true
      setSnapshotHydrated(true)
    })()
    return () => {
      cancelled = true
    }
  }, [])

  const refresh = useCallback(async (
    cursor: string | null = currentCursorRef.current,
    requestedFilter: Filter = filterRef.current,
    options: QueueRefreshOptions = {},
  ) => {
    const source = options.source ?? 'live'
    const trigger = options.trigger ?? 'background'
    const requestId = ++refreshRequestIdRef.current
    setError(null)
    setJobsSource(snapshotActiveRef.current ? 'fresh-snapshot' : source)
    setJobsTrigger(trigger)
    setJobsPhase(jobsLoadedRef.current ? 'refreshing' : 'initial-loading')
    try {
      const page = await listJobsPage({
        cursor,
        limit: 100,
        status: requestedFilter === 'all' ? null : requestedFilter,
      })
      if (requestId !== refreshRequestIdRef.current) return false
      setJobs(page.jobs)
      filterRef.current = requestedFilter
      setFilter(requestedFilter)
      currentCursorRef.current = cursor
      setCurrentCursor(cursor)
      setNextCursor(page.nextCursor ?? null)
      setJobsLoaded(true)
      jobsLoadedRef.current = true
      setJobsLiveLoaded(true)
      setJobsPhase(page.jobs.length === 0 ? 'ready-empty' : 'ready-data')
      return true
    } catch (e: unknown) {
      if (requestId !== refreshRequestIdRef.current) return false
      setJobsPhase('error')
      throw e
    }
  }, [])

  const navigateCursor = useCallback(async (cursor: string | null, nextStack: (string | null)[]) => {
    if (paginationBusy) return
    setPaginationBusy(true)
    try {
      if (await refresh(cursor, filterRef.current, { source: 'memory', trigger: 'user-action' })) setCursorStack(nextStack)
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setPaginationBusy(false)
    }
  }, [paginationBusy, refresh])

  useEffect(() => {
    if (!snapshotHydrated) return
    void refresh().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
  }, [refresh, snapshotHydrated])

  const refreshVersionInferenceSummary = useCallback(async () => {
    const requestId = ++inferenceRequestIdRef.current
    try {
      const payload = await getVersionInferenceOverview({
        page: 1,
        perPage: 1,
      })
      if (requestId !== inferenceRequestIdRef.current) return
      setVersionInferenceSummary(parseVersionInferenceSummary(payload))
      setVersionInferenceError(null)
      setVersionInferenceLoaded(true)
      setVersionInferenceLiveLoaded(true)
    } catch (e: unknown) {
      if (requestId !== inferenceRequestIdRef.current) return
      setVersionInferenceError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  const refreshGhcrSummary = useCallback(async () => {
    const requestId = ++ghcrRequestIdRef.current
    try {
      const payload = await getGitHubPackagesWebhookOverview()
      if (requestId !== ghcrRequestIdRef.current) return
      setGhcrSummary(parseGhcrWebhookSummary(payload))
      setGhcrError(null)
      setGhcrLoaded(true)
      setGhcrLiveLoaded(true)
    } catch (e: unknown) {
      if (requestId !== ghcrRequestIdRef.current) return
      setGhcrError(e instanceof Error ? e.message : String(e))
    }
  }, [])

  useEffect(() => {
    void refreshVersionInferenceSummary()
  }, [refreshVersionInferenceSummary])

  useEffect(() => {
    void refreshGhcrSummary()
  }, [refreshGhcrSummary])

  useManagementEventBatch(({ events, resyncRequired }) => {
    const jobChanged = resyncRequired || events.some((event) => event.domain === 'jobs')
    const inferenceChanged = resyncRequired || events.some((event) =>
      event.domain === 'version_inference' || event.summary.jobType === 'version_inference',
    )
    const ghcrChanged = resyncRequired || events.some((event) =>
      event.domain === 'github_packages' || event.summary.jobType === 'github_packages_webhook',
    )
    if (jobChanged) void refresh().catch((error: unknown) => setError(error instanceof Error ? error.message : String(error)))
    if (inferenceChanged) void refreshVersionInferenceSummary()
    if (ghcrChanged) void refreshGhcrSummary()
  })

  useEffect(() => {
    onTopActions(
      <Button
        variant="ghost"
        disabled={busy || !isOnline}
        onClick={() => {
          void (async () => {
            setBusy(true)
            try {
              await Promise.all([refresh(currentCursorRef.current, filterRef.current, { source: 'memory', trigger: 'user-action' }), refreshVersionInferenceSummary(), refreshGhcrSummary()])
            } catch (e: unknown) {
              setError(e instanceof Error ? e.message : String(e))
            } finally {
              setBusy(false)
            }
          })()
        }}
      >
        刷新
      </Button>,
    )
  }, [busy, isOnline, onTopActions, refresh, refreshGhcrSummary, refreshVersionInferenceSummary])

  useEffect(() => {
    if (!jobsLiveLoaded || !versionInferenceLiveLoaded || !ghcrLiveLoaded) return
    setSnapshotActive(false)
    snapshotActiveRef.current = false
    setSnapshotAnchorFetchedAt(null)
  }, [ghcrLiveLoaded, jobsLiveLoaded, versionInferenceLiveLoaded])

  useEffect(() => {
    if (!jobsLiveLoaded || !versionInferenceLiveLoaded || !ghcrLiveLoaded) return
    const payload: QueueSnapshotPayload = {
      version: 2,
      readiness: {
        jobs: true,
        versionInference: true,
        ghcr: true,
      },
      committedQueryKey: `${filter}:${currentCursor ?? ''}:${nextCursor ?? ''}`,
      jobs,
      filter,
      currentCursor,
      nextCursor,
      cursorStack,
      versionInferenceSummary,
      versionInferenceLoaded: true,
      ghcrSummary,
      ghcrLoaded: true,
    }
    void writeReadonlySnapshot(QUEUE_SNAPSHOT_KEY, payload, {
      staleAfterMs: QUEUE_SNAPSHOT_STALE_MS,
      fetchedAt: snapshotAnchorFetchedAt ? Date.parse(snapshotAnchorFetchedAt) || undefined : undefined,
    })
  }, [
    ghcrLiveLoaded,
    ghcrSummary,
    currentCursor,
    cursorStack,
    filter,
    jobs,
    jobsLiveLoaded,
    snapshotAnchorFetchedAt,
    nextCursor,
    versionInferenceLiveLoaded,
    versionInferenceSummary,
  ])

  const filtered = filter === 'all' ? jobs : jobs.filter((job) => job.status === filter)

  return (
    <div className="page">
      {snapshotActive ? (
        <ReadonlySnapshotNotice
          tone={!isOnline ? 'warn' : 'info'}
          title={!isOnline ? '当前离线，显示已缓存的任务队列数据。' : '任务队列先显示已缓存数据，后台会继续刷新。'}
          detail="任务详情页、GHCR 管理写操作和其他高时效内容仍以联网结果为准。"
          fetchedAt={snapshotFetchedAt}
          actionLabel="重试刷新"
          actionDisabled={!isOnline || busy}
          onAction={() => {
            void (async () => {
              setBusy(true)
              try {
                await Promise.all([refresh(currentCursorRef.current, filterRef.current, { source: 'memory', trigger: 'user-action' }), refreshVersionInferenceSummary(), refreshGhcrSummary()])
              } catch (e: unknown) {
                setError(e instanceof Error ? e.message : String(e))
              } finally {
                setBusy(false)
              }
            })()
          }}
        />
      ) : !isOnline && !jobsLoaded && !versionInferenceLoaded && !ghcrLoaded ? (
        <ReadonlySnapshotNotice
          tone="bad"
          title="当前没有可用的离线任务队列数据。"
          detail="请恢复联网后重新加载该页面。"
        />
      ) : null}
      <div className="card">
        <div className="sectionRow">
          <div className="title">任务队列</div>
          <div className="chipRow" style={{ marginLeft: 'auto' }}>
            {(['all', 'queued', 'running', 'success', 'failed', 'rolled_back', 'cancelled'] as const).map((k) => (
              <button
                key={k}
                className={filter === k ? 'chip chipActive' : 'chip'}
                aria-busy={jobsPhase === 'refreshing' || undefined}
                disabled={jobsPhase === 'initial-loading' || jobsPhase === 'refreshing'}
                onClick={() => {
                  if (k === filter || jobsPhase === 'initial-loading' || jobsPhase === 'refreshing') return
                  void refresh(null, k, { source: 'memory', trigger: 'user-action' }).then((applied) => {
                    if (!applied) return
                    setCursorStack([])
                  }).catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
                }}
                type="button"
              >
                {k === 'all' ? '全部' : k}
              </button>
            ))}
          </div>
        </div>
        <div className="muted" style={{ marginTop: 8 }}>
          点击任务查看详情与日志
        </div>

        <button
          type="button"
          className="queueSummaryItem"
          style={{ marginTop: 12 }}
          onClick={() => navigate({ name: 'version-inference' })}
        >
          <div className="queueMain">
            <div className="queueTitle">
              <Mono>版本推测状态</Mono>
            </div>
            <div className="queueMeta">
              <span>
                snapshots <Mono>{versionInferenceLoaded ? versionInferenceSummary.snapshotsTotal : '—'}</Mono>
              </span>
              <span>
                queued <Mono>{versionInferenceLoaded ? versionInferenceSummary.queued : '—'}</Mono>
              </span>
              <span>
                running <Mono>{versionInferenceLoaded ? versionInferenceSummary.running : '—'}</Mono>
              </span>
              <span>
                all_failed <Mono>{versionInferenceLoaded ? versionInferenceSummary.allFailed : '—'}</Mono>
              </span>
            </div>
            <div className="muted" style={{ marginTop: 8 }}>
              查看版本推测任务与缓存状态
            </div>
            {versionInferenceError ? (
              <div className="error" style={{ marginTop: 8 }}>
                {versionInferenceError}
              </div>
            ) : null}
          </div>
          <div className="queueStatus">
            <Pill
              tone={versionInferenceTone(versionInferenceSummary)}
              breathing={versionInferenceLoaded && versionInferenceLabel(versionInferenceSummary) === 'running'}
            >
              {versionInferenceLoaded ? versionInferenceLabel(versionInferenceSummary) : 'loading'}
            </Pill>
          </div>
        </button>

        <button
          type="button"
          className="queueSummaryItem"
          style={{ marginTop: 12 }}
          onClick={() => navigate({ name: 'ghcr-webhooks' })}
        >
          <div className="queueMain">
            <div className="queueTitle">
              <Mono>GHCR Webhook 状态</Mono>
            </div>
            <div className="queueMeta">
              <span>
                tracked <Mono>{ghcrLoaded ? ghcrSummary.tracked : '—'}</Mono>
              </span>
              <span>
                ok <Mono>{ghcrLoaded ? ghcrSummary.ok : '—'}</Mono>
              </span>
              <span>
                missing <Mono>{ghcrLoaded ? ghcrSummary.missing : '—'}</Mono>
              </span>
              <span>
                error <Mono>{ghcrLoaded ? ghcrSummary.error : '—'}</Mono>
              </span>
              <span>
                conflict <Mono>{ghcrLoaded ? ghcrSummary.conflict : '—'}</Mono>
              </span>
              <span>
                jobsQueued <Mono>{ghcrLoaded ? ghcrSummary.jobsQueued : '—'}</Mono>
              </span>
              <span>
                jobsRunning <Mono>{ghcrLoaded ? ghcrSummary.jobsRunning : '—'}</Mono>
              </span>
            </div>
            <div className="muted" style={{ marginTop: 8 }}>
              查看 GHCR webhook 任务队列、仓库状态与巡检结果
            </div>
            {ghcrError ? (
              <div className="error" style={{ marginTop: 8 }}>
                {ghcrError}
              </div>
            ) : null}
          </div>
          <div className="queueStatus">
            <Pill tone={ghcrTone(ghcrSummary)}>{ghcrLoaded ? ghcrLabel(ghcrSummary) : 'loading'}</Pill>
          </div>
        </button>

        <AsyncDataRegion
          className="queueList"
          error={error}
          hasData={jobsLoaded}
          label="正在刷新任务队列"
          onRetry={() => void refresh(currentCursorRef.current, filterRef.current, { source: 'memory', trigger: 'user-action' }).catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))}
          phase={jobsPhase}
          skeleton={<AsyncDataSkeleton className="queueLoadingSkeleton" lines={4} />}
          source={jobsSource}
          trigger={jobsTrigger}
        >
          {filtered.map((j) => {
            const readable = formatJobReadableDisplay(j.type, j.scope, j.summary)
            const progressLabel = formatProgressLabel(j)
            const progress = getProgressMetrics(j)
            const plannedPercent = progress?.plannedPercent ?? null
            const completedPercent = progress?.completedPercent ?? null
            const plannedAria = plannedPercent !== null ? `${plannedPercent}%` : 'running'
            const completedAria = completedPercent !== null ? `${completedPercent}%` : 'running'
            const isDualIndeterminate = plannedPercent === null || completedPercent === null
            return (
              <div
                key={j.id}
                className="queueItem"
                role="button"
                tabIndex={0}
                onClick={() => navigate({ name: 'job', jobId: j.id })}
                onKeyDown={(event) => {
                  const target = event.target as HTMLElement | null
                  if (target && target !== event.currentTarget) return
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault()
                    navigate({ name: 'job', jobId: j.id })
                  }
                }}
              >
                <div className="queueMain">
                  <div className="queueTitle">
                    <span className="jobReadableTagGroup">
                      <span className={`jobTypeTag jobTypeTag-${readable.typeTone}`}>{readable.primaryLabel}</span>
                      {readable.scopeTag ? <span className="jobScopeTag">{readable.scopeTag}</span> : null}
                    </span>
                  </div>
                  <div className="queueMeta">
                    <span>
                      machine <Mono>{formatJobMachineName(j.type, j.scope)}</Mono>
                    </span>
                    <span>
                      by <Mono>{j.createdBy}</Mono> · reason <Mono>{j.reason}</Mono>
                    </span>
                    {j.status === 'running' ? (
                      <span>
                        progress <Mono>{progressLabel ?? `running ${formatRunningDuration(j.startedAt) ?? '-'}`}</Mono>
                      </span>
                    ) : null}
                    <span>
                      created <Mono>{formatShort(j.createdAt)}</Mono>
                    </span>
                    <span>
                      started <Mono>{formatShort(j.startedAt)}</Mono>
                    </span>
                    {shouldShowFinishedAt(j) ? (
                      <span>
                        finished <Mono>{formatShort(j.finishedAt)}</Mono>
                      </span>
                    ) : null}
                  </div>
                  <TaskResultReason reason={j.resultReason} lines={1} className="queueResultReason" />
                  {j.status === 'running' ? (
                    <div className="queueProgressLayers">
                      <div
                        className={isDualIndeterminate ? 'queueProgressBar queueProgressBarDual queueProgressBarIndeterminate' : 'queueProgressBar queueProgressBarDual'}
                        role="progressbar"
                        aria-valuemin={0}
                        aria-valuemax={100}
                        aria-valuenow={completedPercent ?? plannedPercent ?? undefined}
                        aria-valuetext={`安排 ${plannedAria} · 完成 ${completedAria}`}
                      >
                        <div
                          className={
                            plannedPercent === null
                              ? 'queueProgressFill queueProgressFillPlanned queueProgressFillLayerPlanned queueProgressFillIndeterminate'
                              : 'queueProgressFill queueProgressFillPlanned queueProgressFillLayerPlanned'
                          }
                          style={plannedPercent === null ? undefined : { transform: `scaleX(${plannedPercent / 100})` }}
                        />
                        <div
                          className={
                            completedPercent === null
                              ? 'queueProgressFill queueProgressFillCompleted queueProgressFillLayerCompleted queueProgressFillIndeterminate'
                              : 'queueProgressFill queueProgressFillCompleted queueProgressFillLayerCompleted'
                          }
                          style={completedPercent === null ? undefined : { transform: `scaleX(${completedPercent / 100})` }}
                        />
                      </div>
                    </div>
                  ) : null}
                </div>
                <div className="queueStatus">
                  <Pill tone={statusTone(j.status)} breathing={j.status === 'running'}>
                    {j.status}
                  </Pill>
                </div>
              </div>
            )
          })}
          {jobsLoaded && filtered.length === 0 ? <div className="muted">暂无任务</div> : null}
        </AsyncDataRegion>
        <div className="sectionRow" style={{ marginTop: 12 }}>
          <div className="muted">每页 100 条</div>
          <div className="chipRow" style={{ marginLeft: 'auto' }}>
            <Button
              variant="ghost"
              disabled={cursorStack.length === 0 || busy || paginationBusy || !isOnline}
              onClick={() => {
                const previous = cursorStack[cursorStack.length - 1] ?? null
                void navigateCursor(previous, cursorStack.slice(0, -1))
              }}
            >
              上一页
            </Button>
            <Button
              variant="ghost"
              disabled={!nextCursor || busy || paginationBusy || !isOnline}
              onClick={() => {
                if (!nextCursor) return
                void navigateCursor(nextCursor, [...cursorStack, currentCursor])
              }}
            >
              下一页
            </Button>
          </div>
        </div>
      </div>
    </div>
  )
}
