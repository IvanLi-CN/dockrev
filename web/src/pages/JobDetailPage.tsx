import { useCallback, useEffect, useState } from 'react'
import { getJob, type JobDetail, type JobLogLine } from '../api'
import { navigate } from '../routes'
import { Button, Mono, Pill } from '../ui'

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

export function JobDetailPage(props: { jobId: string; onTopActions: (node: React.ReactNode) => void }) {
  const { jobId, onTopActions } = props
  const [job, setJob] = useState<JobDetail | null>(null)
  const [logs, setLogs] = useState<JobLogLine[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

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
    <div className="page">
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

      <div className="card">
        <div className="sectionRow">
          <div className="title">日志</div>
        </div>

        <div className="logs">
          {logs.map((l, idx) => (
            <div key={`${l.ts}-${idx}`} className="logLine">
              <span className="mono logTs">{l.ts}</span>
              <span className="mono logLvl">{l.level}</span>
              <span className="logMsg">{l.msg}</span>
            </div>
          ))}
          {logs.length === 0 ? <div className="muted">无日志</div> : null}
        </div>
      </div>
    </div>
  )
}
