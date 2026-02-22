import { useCallback, useEffect, useState } from 'react'
import { getJob, newJobEventsSource, type JobDetail, type JobLogLine, type JobProgress } from '../api'
import { navigate } from '../routes'
import { Button, Chip, Mono, Pill } from '../ui'

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
  const percentRaw = Number.isFinite(input.percent) ? input.percent : total > 0 ? Math.floor((current * 100) / total) : 0
  const percent = Math.min(100, Math.max(0, percentRaw))
  return { ...input, current, total, percent }
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
            if (j.status !== 'running' || j.finishedAt) {
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

        // Nothing to stream for finished jobs.
        if (j.status !== 'running' || j.finishedAt) return

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

  return (
    <div className="page jobDetailPage">
      <div className="card">
        <div className="sectionRow">
          <div className="title">任务详情</div>
          <div className="muted" style={{ marginLeft: 'auto' }}>
            job: <Mono>{jobId}</Mono>
          </div>
          {job ? <Pill tone={statusTone(job.status)}>{job.status}</Pill> : null}
        </div>

        {job ? (
          <div className="muted" style={{ marginTop: 8 }}>
            <div>
              type <Mono>{job.type}</Mono> · scope <Mono>{job.scope}</Mono> · by <Mono>{job.createdBy}</Mono> · reason{' '}
              <Mono>{job.reason}</Mono>
            </div>
            <div style={{ marginTop: 6 }}>
              created <Mono>{formatShort(job.createdAt)}</Mono> · started <Mono>{formatShort(job.startedAt)}</Mono> ·
              finished <Mono>{formatShort(job.finishedAt)}</Mono>
            </div>
          </div>
        ) : null}
        {progress ? (
          <div className="jobProgress">
            <div className="jobProgressHeader">
              <div className="title">进度</div>
              <div className="mono">{`${progress.percent}%`}</div>
            </div>
            <div className="jobProgressBar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress.percent}>
              <div className="jobProgressFill" style={{ width: `${progress.percent}%` }} />
            </div>
            <div className="jobProgressMeta">
              <span>
                <Mono>{progress.phase || '-'}</Mono>
                {progress.message ? ` · ${progress.message}` : ''}
              </span>
              <span>
                <Mono>{`${progress.current}/${progress.total}`}</Mono>
                {progress.currentTarget ? ` · ${progress.currentTarget}` : ''}
              </span>
              <span>
                updated <Mono>{formatShort(progress.updatedAt)}</Mono>
              </span>
            </div>
          </div>
        ) : job?.status === 'running' ? (
          <div className="muted" style={{ marginTop: 8 }}>
            运行中，等待进度数据…
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
