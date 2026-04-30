import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  getGitHubPackagesWebhookOverview,
  getVersionInferenceOverview,
  listJobs,
  newJobsEventsSource,
  type JobListItem,
} from '../api'
import { formatJobMachineName, formatJobReadableDisplay } from '../jobDisplay'
import { navigate } from '../routes'
import { Button, Mono, Pill } from '../ui'

type Filter = 'all' | 'queued' | 'running' | 'success' | 'failed' | 'rolled_back'
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
  const parts: string[] = []
  if (m.plannedTotal > 0 || m.completedTotal > 0) {
    parts.push(`安排 ${m.plannedCurrent}/${m.plannedTotal || '-'} · 完成 ${m.completedCurrent}/${m.completedTotal || '-'}`)
  }
  if (phase) parts.push(phase)
  if (message) parts.push(message)
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
    plannedTotal > 0 && Number.isFinite(p.plannedPercent)
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

const QUEUE_SSE_ERROR_THRESHOLD = 3
const QUEUE_SSE_RECONNECT_MS = 3000
const QUEUE_SSE_REFRESH_DEBOUNCE_MS = 250
const QUEUE_SSE_FALLBACK_POLL_MS = 10_000
const VERSION_INFERENCE_SUMMARY_POLL_MS = 15_000
const GHCR_SUMMARY_POLL_MS = 15_000

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
  if (summary.stale > 0) return 'stale'
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
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [filter, setFilter] = useState<Filter>('all')
  const [error, setError] = useState<string | null>(null)
  const [versionInferenceSummary, setVersionInferenceSummary] = useState<VersionInferenceSummary>(
    DEFAULT_VERSION_INFERENCE_SUMMARY,
  )
  const [versionInferenceLoaded, setVersionInferenceLoaded] = useState(false)
  const [versionInferenceError, setVersionInferenceError] = useState<string | null>(null)
  const [ghcrSummary, setGhcrSummary] = useState<GhcrWebhookSummary>(DEFAULT_GHCR_SUMMARY)
  const [ghcrLoaded, setGhcrLoaded] = useState(false)
  const [ghcrError, setGhcrError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const refreshRequestIdRef = useRef(0)
  const inferenceRequestIdRef = useRef(0)
  const ghcrRequestIdRef = useRef(0)

  const refresh = useCallback(async () => {
    const requestId = ++refreshRequestIdRef.current
    setError(null)
    try {
      const nextJobs = await listJobs()
      if (requestId !== refreshRequestIdRef.current) return
      setJobs(nextJobs)
    } catch (e: unknown) {
      if (requestId !== refreshRequestIdRef.current) return
      throw e
    }
  }, [])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
  }, [refresh])

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
    } catch (e: unknown) {
      if (requestId !== inferenceRequestIdRef.current) return
      setVersionInferenceError(e instanceof Error ? e.message : String(e))
      setVersionInferenceLoaded(true)
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
    } catch (e: unknown) {
      if (requestId !== ghcrRequestIdRef.current) return
      setGhcrError(e instanceof Error ? e.message : String(e))
      setGhcrLoaded(true)
    }
  }, [])

  useEffect(() => {
    void refreshVersionInferenceSummary()
  }, [refreshVersionInferenceSummary])

  useEffect(() => {
    void refreshGhcrSummary()
  }, [refreshGhcrSummary])

  useEffect(() => {
    const timer = window.setInterval(() => {
      void refreshVersionInferenceSummary()
    }, VERSION_INFERENCE_SUMMARY_POLL_MS)
    return () => window.clearInterval(timer)
  }, [refreshVersionInferenceSummary])

  useEffect(() => {
    const timer = window.setInterval(() => {
      void refreshGhcrSummary()
    }, GHCR_SUMMARY_POLL_MS)
    return () => window.clearInterval(timer)
  }, [refreshGhcrSummary])

  useEffect(() => {
    let closed = false
    let es: EventSource | null = null
    let errorStreak = 0
    let lastEventId = 0
    let refreshTimer: number | null = null
    let pollTimer: number | null = null
    let reconnectTimer: number | null = null

    const refreshSafely = async () => {
      try {
        await refresh()
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e))
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
        void refreshSafely()
      }, delayMs)
    }

    const stopPolling = () => {
      if (pollTimer != null) window.clearInterval(pollTimer)
      pollTimer = null
    }

    const startPolling = () => {
      if (pollTimer != null) return
      pollTimer = window.setInterval(() => {
        void refreshSafely()
      }, QUEUE_SSE_FALLBACK_POLL_MS)
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
        // Catch up once on successful subscribe so updates between initial list and SSE connect are not missed.
        scheduleRefresh(0)
      })

      es.addEventListener('job_event', (evt: Event) => {
        trackEventId(evt)
        scheduleRefresh(QUEUE_SSE_REFRESH_DEBOUNCE_MS)
      })

      es.addEventListener('job_events_error', () => {
        scheduleRefresh(0)
      })

      es.onerror = () => {
        errorStreak += 1
        scheduleRefresh(0)
        if (errorStreak < QUEUE_SSE_ERROR_THRESHOLD) return
        es?.close()
        es = null
        startPolling()
        if (reconnectTimer != null) return
        reconnectTimer = window.setTimeout(() => {
          reconnectTimer = null
          connect()
        }, QUEUE_SSE_RECONNECT_MS)
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
  }, [refresh])

  useEffect(() => {
    onTopActions(
      <Button
        variant="ghost"
        disabled={busy}
        onClick={() => {
          void (async () => {
            setBusy(true)
            try {
              await Promise.all([refresh(), refreshVersionInferenceSummary(), refreshGhcrSummary()])
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
  }, [busy, onTopActions, refresh, refreshGhcrSummary, refreshVersionInferenceSummary])

  const filtered = useMemo(() => {
    if (filter === 'all') return jobs
    return jobs.filter((j) => j.status === filter)
  }, [jobs, filter])

  return (
    <div className="page">
      <div className="card">
        <div className="sectionRow">
          <div className="title">任务队列</div>
          <div className="chipRow" style={{ marginLeft: 'auto' }}>
            {(['all', 'queued', 'running', 'success', 'failed', 'rolled_back'] as const).map((k) => (
              <button
                key={k}
                className={filter === k ? 'chip chipActive' : 'chip'}
                onClick={() => setFilter(k)}
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
                snapshots <Mono>{versionInferenceSummary.snapshotsTotal}</Mono>
              </span>
              <span>
                queued <Mono>{versionInferenceSummary.queued}</Mono>
              </span>
              <span>
                running <Mono>{versionInferenceSummary.running}</Mono>
              </span>
              <span>
                stale <Mono>{versionInferenceSummary.stale}</Mono>
              </span>
              <span>
                all_failed <Mono>{versionInferenceSummary.allFailed}</Mono>
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
                tracked <Mono>{ghcrSummary.tracked}</Mono>
              </span>
              <span>
                ok <Mono>{ghcrSummary.ok}</Mono>
              </span>
              <span>
                missing <Mono>{ghcrSummary.missing}</Mono>
              </span>
              <span>
                error <Mono>{ghcrSummary.error}</Mono>
              </span>
              <span>
                conflict <Mono>{ghcrSummary.conflict}</Mono>
              </span>
              <span>
                jobsQueued <Mono>{ghcrSummary.jobsQueued}</Mono>
              </span>
              <span>
                jobsRunning <Mono>{ghcrSummary.jobsRunning}</Mono>
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

        <div className="queueList">
          {filtered.map((j) => {
            const readable = formatJobReadableDisplay(j.type, j.scope)
            const progressLabel = formatProgressLabel(j)
            const progress = getProgressMetrics(j)
            const plannedPercent = progress?.plannedPercent ?? null
            const completedPercent = progress?.completedPercent ?? null
            const plannedAria = plannedPercent !== null ? `${plannedPercent}%` : 'running'
            const completedAria = completedPercent !== null ? `${completedPercent}%` : 'running'
            const isDualIndeterminate = plannedPercent === null || completedPercent === null
            return (
              <button
                key={j.id}
                className="queueItem"
                onClick={() => navigate({ name: 'job', jobId: j.id })}
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
              </button>
            )
          })}
          {filtered.length === 0 ? <div className="muted">暂无任务</div> : null}
        </div>

        {error ? <div className="error">{error}</div> : null}
      </div>
    </div>
  )
}
