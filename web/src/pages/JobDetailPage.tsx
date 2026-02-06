import { useCallback, useEffect, useState } from 'react'
import { getJob, type JobDetail, type JobLogLine } from '../api'
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

export function JobDetailPage(props: { jobId: string; onTopActions: (node: React.ReactNode) => void }) {
  const { jobId, onTopActions } = props
  const [job, setJob] = useState<JobDetail | null>(null)
  const [logs, setLogs] = useState<JobLogLine[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [logTz, setLogTz] = useState<LogTimeZone>('local')

  const refresh = useCallback(async () => {
    setError(null)
    const j = await getJob(jobId)
    setJob(j)
    setLogs(j.logs)
  }, [jobId])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(errorMessage(e)))
  }, [refresh])

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
