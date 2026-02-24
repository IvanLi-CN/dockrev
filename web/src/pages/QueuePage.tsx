import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { listJobs, newJobsEventsSource, type JobListItem } from '../api'
import { navigate } from '../routes'
import { Button, Mono, Pill } from '../ui'

type Filter = 'all' | 'running' | 'success' | 'failed' | 'rolled_back'

function statusTone(status: string): 'ok' | 'warn' | 'bad' | 'muted' {
  if (status === 'success') return 'ok'
  if (status === 'rolled_back') return 'warn'
  if (status === 'failed') return 'bad'
  if (status === 'running') return 'warn'
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

export function QueuePage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [filter, setFilter] = useState<Filter>('all')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const refreshRequestIdRef = useRef(0)

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
              await refresh()
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
  }, [busy, onTopActions, refresh])

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
            {(['all', 'running', 'success', 'failed', 'rolled_back'] as const).map((k) => (
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

        <div className="queueList">
          {filtered.map((j) => {
            const progressLabel = formatProgressLabel(j)
            const progress = getProgressMetrics(j)
            const plannedPercent = progress?.plannedPercent ?? null
            const completedPercent = progress?.completedPercent ?? null
            return (
              <button
                key={j.id}
                className="queueItem"
                onClick={() => navigate({ name: 'job', jobId: j.id })}
              >
                <div className="queueMain">
                  <div className="queueTitle">
                    <Mono>{j.type}</Mono> · <Mono>{j.scope}</Mono>
                  </div>
                  <div className="queueMeta">
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
                      <div className={plannedPercent === null ? 'queueProgressBar queueProgressBarPlanned queueProgressBarIndeterminate' : 'queueProgressBar queueProgressBarPlanned'} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={plannedPercent ?? undefined} aria-valuetext={plannedPercent === null ? 'running' : `${plannedPercent}%`}>
                        <div className={plannedPercent === null ? 'queueProgressFill queueProgressFillPlanned queueProgressFillIndeterminate' : 'queueProgressFill queueProgressFillPlanned'} style={plannedPercent === null ? undefined : { width: `${plannedPercent}%` }} />
                      </div>
                      <div className={completedPercent === null ? 'queueProgressBar queueProgressBarIndeterminate' : 'queueProgressBar'} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={completedPercent ?? undefined} aria-valuetext={completedPercent === null ? 'running' : `${completedPercent}%`}>
                        <div className={completedPercent === null ? 'queueProgressFill queueProgressFillIndeterminate' : 'queueProgressFill'} style={completedPercent === null ? undefined : { width: `${completedPercent}%` }} />
                      </div>
                    </div>
                  ) : null}
                </div>
                <div className="queueStatus">
                  <Pill tone={statusTone(j.status)}>{j.status}</Pill>
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
