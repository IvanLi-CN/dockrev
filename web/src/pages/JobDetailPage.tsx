import { useCallback, useEffect, useState } from 'react'
import { getJob, newJobEventsSource, type JobDetail, type JobLogLine, type JobProgress } from '../api'
import { navigate } from '../routes'
import { Button, Chip, Mono, Pill } from '../ui'

function statusTone(status: string): 'ok' | 'warn' | 'bad' | 'muted' | 'info' {
  if (status === 'success') return 'ok'
  if (status === 'rolled_back') return 'warn'
  if (status === 'failed') return 'bad'
  if (status === 'running') return 'info'
  return 'muted'
}

function formatShort(ts?: string | null) {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

type LogTimeZone = 'local' | 'utc'

const LOCAL_TZ = (() => {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || 'local'
  } catch {
    return 'local'
  }
})()

function pad2(n: number): string {
  return String(n).padStart(2, '0')
}

function pad3(n: number): string {
  return String(n).padStart(3, '0')
}

function formatLogTs(ts: string, tz: LogTimeZone): string {
  const s = (ts ?? '').trim()
  if (!s) return '-'

  const d = new Date(s)
  if (!Number.isNaN(d.valueOf())) {
    const h = tz === 'utc' ? d.getUTCHours() : d.getHours()
    const min = tz === 'utc' ? d.getUTCMinutes() : d.getMinutes()
    const sec = tz === 'utc' ? d.getUTCSeconds() : d.getSeconds()
    const ms = tz === 'utc' ? d.getUTCMilliseconds() : d.getMilliseconds()
    // Monospace-friendly, stable width (works well with the 14ch grid column).
    return `${pad2(h)}:${pad2(min)}:${pad2(sec)}.${pad3(ms)}`
  }

  // Common "YYYY-MM-DD HH:mm:ss" -> show time part.
  const m = s.match(/^\d{4}-\d{2}-\d{2}[ T](.+)$/)
  if (m) return m[1]

  return s
}

function formatLogTitle(ts: string): string {
  const s = (ts ?? '').trim()
  if (!s) return '-'
  const d = new Date(s)
  if (Number.isNaN(d.valueOf())) return s
  return `${LOCAL_TZ}: ${d.toLocaleString()} · UTC: ${d.toISOString()}`
}

function formatLogLevel(level: string): string {
  const s = (level ?? '').trim().toLowerCase()
  if (!s) return '-'
  if (s === 'info') return 'INFO'
  if (s === 'warn' || s === 'warning') return 'WARN'
  if (s === 'error' || s === 'err') return 'ERR'
  if (s === 'debug') return 'DBG'
  if (s === 'trace') return 'TRC'
  return s.slice(0, 4).toUpperCase()
}

function normalizeProgress(input: JobProgress | null | undefined): JobProgress | null {
  if (!input) return null
  const total = Number.isFinite(input.total) ? Math.max(0, input.total) : 0
  const current = Number.isFinite(input.current) ? Math.min(Math.max(0, input.current), total || input.current) : 0
  // Frontend only sanitizes backend values; percent is never derived from current/total.
  const percentRaw = Number.isFinite(input.percent) ? input.percent : 0
  const percent = Math.max(0, Math.min(100, Math.round(percentRaw)))
  const plannedTotalRaw = Number.isFinite(input.plannedTotal) ? Math.max(0, input.plannedTotal ?? 0) : total
  const plannedCurrentRaw = Number.isFinite(input.plannedCurrent) ? Math.max(0, input.plannedCurrent ?? 0) : current
  const plannedCurrent = Math.min(plannedCurrentRaw, plannedTotalRaw || plannedCurrentRaw)
  const plannedPercentRaw = Number.isFinite(input.plannedPercent) ? input.plannedPercent : percent
  const plannedPercent = Math.max(0, Math.min(100, Math.round(plannedPercentRaw ?? 0)))
  return { ...input, current, total, percent, plannedCurrent, plannedTotal: plannedTotalRaw, plannedPercent }
}

function getKnownProgressPercent(progress: JobProgress): number | null {
  const total = Number.isFinite(progress.total) ? Math.max(0, progress.total) : 0
  if (total <= 0) return null
  if (!Number.isFinite(progress.percent)) return null
  return Math.max(0, Math.min(100, Math.round(progress.percent)))
}

function getKnownPlannedProgressPercent(progress: JobProgress): number | null {
  const total = Number.isFinite(progress.plannedTotal) ? Math.max(0, progress.plannedTotal ?? 0) : 0
  if (total <= 0) return null
  if (!Number.isFinite(progress.plannedPercent)) return null
  return Math.max(0, Math.min(100, Math.round(progress.plannedPercent ?? 0)))
}

export function JobDetailPage(props: { jobId: string; onTopActions: (node: React.ReactNode) => void }) {
  const { jobId, onTopActions } = props
  const [job, setJob] = useState<JobDetail | null>(null)
  const [logs, setLogs] = useState<JobLogLine[]>([])
  const [progress, setProgress] = useState<JobProgress | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [logTz, setLogTz] = useState<LogTimeZone>('local')

  const refresh = useCallback(async () => {
    setError(null)
    const j = await getJob(jobId)
    setJob(j)
    setLogs(j.logs)
    setProgress(normalizeProgress(j.progress))
    return j
  }, [jobId])

  useEffect(() => {
    let closed = false
    let es: EventSource | null = null
    let pollTimer: number | null = null
    let refreshTimer: number | null = null
    let errorStreak = 0

    const stopPolling = () => {
      if (pollTimer != null) window.clearInterval(pollTimer)
      pollTimer = null
    }

    const scheduleRefresh = (delayMs: number) => {
      if (refreshTimer != null) return
      refreshTimer = window.setTimeout(() => {
        refreshTimer = null
        void (async () => {
          try {
            const j = await refresh()
            if (closed) return
            if (j.status !== 'running' && j.status !== 'queued') {
              es?.close()
              stopPolling()
            }
          } catch {
            // ignore refresh failures; user can still use the manual refresh button
          }
        })()
      }, delayMs)
    }

    const startPolling = () => {
      if (pollTimer != null) return
      pollTimer = window.setInterval(() => {
        void refresh().catch(() => {})
      }, 1000)
    }

    const start = async () => {
      try {
        const j = await refresh()
        if (closed) return

        // Nothing to stream for terminal jobs.
        if (j.status !== 'running' && j.status !== 'queued') return

        try {
          es = newJobEventsSource(jobId, { afterId: j.logsLastId })
        } catch {
          startPolling()
          return
        }

        es.addEventListener('open', () => {
          errorStreak = 0
          stopPolling()
        })

        es.addEventListener('job_log', (evt: Event) => {
          const data = (evt as MessageEvent).data
          if (typeof data !== 'string' || !data) return
          try {
            const parsed = JSON.parse(data) as unknown
            if (!parsed || typeof parsed !== 'object') return
            const p = parsed as Record<string, unknown>
            if (p.type !== 'job_log') return
            const ts = typeof p.ts === 'string' ? p.ts : ''
            const level = typeof p.level === 'string' ? p.level : ''
            const msg = typeof p.msg === 'string' ? p.msg : ''
            setLogs((prev) => {
              const next = [...prev, { ts, level, msg }]
              return next.length > 500 ? next.slice(-500) : next
            })
          } catch {
            // ignore invalid events
          }
        })

        es.addEventListener('job_progress', (evt: Event) => {
          const data = (evt as MessageEvent).data
          if (typeof data !== 'string' || !data) return
          try {
            const parsed = JSON.parse(data) as unknown
            if (!parsed || typeof parsed !== 'object') return
            const p = parsed as Record<string, unknown>
            if (p.type !== 'job_progress') return
            const next = normalizeProgress({
              phase: typeof p.phase === 'string' ? p.phase : 'running',
              message: typeof p.message === 'string' ? p.message : '',
              current: typeof p.current === 'number' ? p.current : 0,
              total: typeof p.total === 'number' ? p.total : 0,
              percent: typeof p.percent === 'number' ? p.percent : 0,
              plannedCurrent: typeof p.plannedCurrent === 'number' ? p.plannedCurrent : null,
              plannedTotal: typeof p.plannedTotal === 'number' ? p.plannedTotal : null,
              plannedPercent: typeof p.plannedPercent === 'number' ? p.plannedPercent : null,
              currentTarget: typeof p.currentTarget === 'string' ? p.currentTarget : null,
              updatedAt: typeof p.updatedAt === 'string' ? p.updatedAt : new Date().toISOString(),
            })
            if (next) setProgress(next)
          } catch {
            // ignore invalid events
          }
        })

        es.onerror = () => {
          errorStreak += 1
          // The backend closes the SSE stream shortly after a job is finished (idle window).
          // Refresh once on close/error so status/finishedAt become up-to-date.
          scheduleRefresh(0)

          if (errorStreak >= 3) {
            // If SSE repeatedly fails (proxy buffering, auth, etc.), fall back to polling.
            es?.close()
            es = null
            startPolling()
          }
        }
      } catch (e: unknown) {
        setError(errorMessage(e))
      }
    }

    void start()

    return () => {
      closed = true
      if (refreshTimer != null) window.clearTimeout(refreshTimer)
      stopPolling()
      es?.close()
    }
  }, [jobId, refresh])

  useEffect(() => {
    onTopActions(
      <>
        <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: 'queue' })}>
          返回列表
        </Button>
        <Button
          variant="ghost"
          disabled={busy}
          onClick={() => {
            void (async () => {
              setBusy(true)
              try {
                await refresh()
              } catch (e: unknown) {
                setError(errorMessage(e))
              } finally {
                setBusy(false)
              }
            })()
          }}
        >
          刷新
        </Button>
      </>,
    )
  }, [busy, onTopActions, refresh])

  const knownProgressPercent = progress ? getKnownProgressPercent(progress) : null
  const knownPlannedPercent = progress ? getKnownPlannedProgressPercent(progress) : null
  const isRunning = job?.status === 'running'
  const completedZeroPercentWhileRunning =
    progress !== null &&
    isRunning &&
    knownProgressPercent === 0 &&
    progress.total > 0 &&
    progress.current < progress.total
  const plannedCurrent = progress ? (progress.plannedCurrent ?? progress.current) : 0
  const plannedTotal = progress ? (progress.plannedTotal ?? progress.total) : 0
  const plannedZeroPercentWhileRunning =
    progress !== null && isRunning && knownPlannedPercent === 0 && plannedTotal > 0 && plannedCurrent < plannedTotal
  const isCompletedIndeterminateRunning =
    progress !== null && isRunning && (knownProgressPercent === null || completedZeroPercentWhileRunning)
  const isPlannedIndeterminateRunning =
    progress !== null && isRunning && (knownPlannedPercent === null || plannedZeroPercentWhileRunning)
  const displayedCompletedPercent = isCompletedIndeterminateRunning ? null : knownProgressPercent
  const displayedPlannedPercent = isPlannedIndeterminateRunning ? null : knownPlannedPercent
  const plannedProgressLabel =
    displayedPlannedPercent !== null ? `${displayedPlannedPercent}%` : isRunning ? 'running' : job?.status ?? '-'
  const completedProgressLabel =
    displayedCompletedPercent !== null ? `${displayedCompletedPercent}%` : isRunning ? 'running' : job?.status ?? '-'
  const plannedProgressAriaText =
    displayedPlannedPercent !== null ? `${displayedPlannedPercent}%` : isRunning ? 'running' : job?.status ?? 'finished'
  const completedProgressAriaText =
    displayedCompletedPercent !== null ? `${displayedCompletedPercent}%` : isRunning ? 'running' : job?.status ?? 'finished'
  const dualProgressAriaText = `安排 ${plannedProgressAriaText} · 完成 ${completedProgressAriaText}`
  const isDualIndeterminate = isPlannedIndeterminateRunning || isCompletedIndeterminateRunning

  return (
    <div className="page jobDetailPage">
      <div className="card">
        <div className="sectionRow">
          <div className="title">任务详情</div>
          <div className="muted" style={{ marginLeft: 'auto' }}>
            job: <Mono>{jobId}</Mono>
          </div>
          {job ? (
            <Pill tone={statusTone(job.status)} breathing={job.status === 'running'}>
              {job.status}
            </Pill>
          ) : null}
        </div>

        {job ? (
          <div className="muted" style={{ marginTop: 8 }}>
            <div>
              type <Mono>{job.type}</Mono> · scope <Mono>{job.scope}</Mono> · by <Mono>{job.createdBy}</Mono> · reason{' '}
              <Mono>{job.reason}</Mono>
            </div>
            <div style={{ marginTop: 6 }}>
              created <Mono>{formatShort(job.createdAt)}</Mono> · started <Mono>{formatShort(job.startedAt)}</Mono>
              {job.status !== 'running' ? (
                <>
                  {' '}
                  · finished <Mono>{formatShort(job.finishedAt)}</Mono>
                </>
              ) : null}
            </div>
          </div>
        ) : null}
        {progress ? (
          <div className="jobProgress">
            <div className="jobProgressHeader">
              <div className="title">进度</div>
              <div className="mono">
                安排 {plannedProgressLabel} · 完成 {completedProgressLabel}
              </div>
            </div>
            <div
              className={isDualIndeterminate ? 'jobProgressBar jobProgressBarDual jobProgressBarIndeterminate' : 'jobProgressBar jobProgressBarDual'}
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={displayedCompletedPercent ?? displayedPlannedPercent ?? undefined}
              aria-valuetext={dualProgressAriaText}
            >
              <div
                className={
                  isPlannedIndeterminateRunning
                    ? 'jobProgressFill jobProgressFillPlanned jobProgressFillLayerPlanned jobProgressFillIndeterminate'
                    : 'jobProgressFill jobProgressFillPlanned jobProgressFillLayerPlanned'
                }
                style={
                  displayedPlannedPercent !== null
                    ? { width: `${displayedPlannedPercent}%` }
                    : isPlannedIndeterminateRunning
                      ? undefined
                      : { width: '100%' }
                }
              />
              <div
                className={
                  isCompletedIndeterminateRunning
                    ? 'jobProgressFill jobProgressFillCompleted jobProgressFillLayerCompleted jobProgressFillIndeterminate'
                    : 'jobProgressFill jobProgressFillCompleted jobProgressFillLayerCompleted'
                }
                style={
                  displayedCompletedPercent !== null
                    ? { width: `${displayedCompletedPercent}%` }
                    : isCompletedIndeterminateRunning
                      ? undefined
                      : { width: '100%' }
                }
              />
            </div>
            <div className="jobProgressCounters">
              <span>
                安排 <Mono>{plannedTotal > 0 ? `${plannedCurrent}/${plannedTotal}` : '-'}</Mono>
              </span>
              <span>
                完成 <Mono>{progress.total > 0 ? `${progress.current}/${progress.total}` : '-'}</Mono>
              </span>
            </div>
            <div className="jobProgressMeta">
              <span>
                <Mono>{progress.phase || '-'}</Mono>
                {progress.message ? ` · ${progress.message}` : ''}
              </span>
              <span>
                安排 <Mono>{plannedTotal > 0 ? `${plannedCurrent}/${plannedTotal}` : '-'}</Mono> · 完成{' '}
                <Mono>{progress.total > 0 ? `${progress.current}/${progress.total}` : '-'}</Mono>
                {progress.currentTarget ? ` · ${progress.currentTarget}` : ''}
              </span>
              <span>
                updated <Mono>{formatShort(progress.updatedAt)}</Mono>
              </span>
            </div>
          </div>
        ) : job?.status === 'running' ? (
          <div className="jobProgress">
            <div className="jobProgressHeader">
              <div className="title">进度</div>
              <div className="mono">安排 running · 完成 running</div>
            </div>
            <div className="jobProgressBar jobProgressBarDual jobProgressBarIndeterminate" role="progressbar" aria-valuetext="安排 running · 完成 running">
              <div className="jobProgressFill jobProgressFillPlanned jobProgressFillLayerPlanned jobProgressFillIndeterminate" />
              <div className="jobProgressFill jobProgressFillCompleted jobProgressFillLayerCompleted jobProgressFillIndeterminate" />
            </div>
            <div className="jobProgressCounters">
              <span>
                安排 <Mono>-</Mono>
              </span>
              <span>
                完成 <Mono>-</Mono>
              </span>
            </div>
            <div className="jobProgressMeta">
              <span>运行中，等待进度数据…</span>
            </div>
          </div>
        ) : null}

        {error ? <div className="error">{error}</div> : null}
      </div>

      <div className="card jobDetailLogsCard">
        <div className="sectionRow">
          <div className="title">日志</div>
          <div style={{ marginLeft: 'auto' }} className="chipRow">
            <span className="muted">时区</span>
            <Chip active={logTz === 'local'} onClick={() => setLogTz('local')} title={`浏览器时区：${LOCAL_TZ}`}>
              本地
            </Chip>
            <Chip active={logTz === 'utc'} onClick={() => setLogTz('utc')} title="后端存储的 job log ts 为 RFC3339（UTC）">
              UTC
            </Chip>
          </div>
        </div>

        <div className="logs">
          {logs.map((l, idx) => (
            <div
              key={`${l.ts}-${idx}`}
              className={`logLine logLine-${(l.level ?? '').trim().toLowerCase() || 'unknown'}`}
            >
              <span className="mono logTs" title={formatLogTitle(l.ts)}>
                {formatLogTs(l.ts, logTz)}
              </span>
              <span className={`mono logLvl logLvl-${(l.level ?? '').trim().toLowerCase()}`}>{formatLogLevel(l.level)}</span>
              <span className="logMsg">{l.msg}</span>
            </div>
          ))}
          {logs.length === 0 ? <div className="muted">无日志</div> : null}
        </div>
      </div>
    </div>
  )
}
