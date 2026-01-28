import { useEffect, useMemo, useState } from 'react'
import { getJob, listJobs, type JobDetail, type JobListItem, type JobLogLine } from '../api'
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

export function QueuePage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [selected, setSelected] = useState<string | null>(null)
  const [selectedJob, setSelectedJob] = useState<JobDetail | null>(null)
  const [filter, setFilter] = useState<Filter>('all')
  const [logs, setLogs] = useState<JobLogLine[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function refresh() {
    setError(null)
    setJobs(await listJobs())
  }

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
  }, [])

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
  }, [busy, onTopActions])

  useEffect(() => {
    if (!selected) {
      setLogs([])
      setSelectedJob(null)
      return
    }
    setBusy(true)
    void (async () => {
      try {
        const job = await getJob(selected)
        setSelectedJob(job)
        setLogs(job.logs)
      } catch (e: unknown) {
        setError(e instanceof Error ? e.message : String(e))
      } finally {
        setBusy(false)
      }
    })()
  }, [selected])

  const filtered = useMemo(() => {
    if (filter === 'all') return jobs
    return jobs.filter((j) => j.status === filter)
  }, [jobs, filter])

  return (
    <div className="page twoCol">
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

        <div className="queueList">
          {filtered.map((j) => (
            <button
              key={j.id}
              className={selected === j.id ? 'queueItem queueItemActive' : 'queueItem'}
              onClick={() => setSelected(j.id)}
            >
              <div className="queueMain">
                <div className="queueTitle">
                  <Mono>{j.type}</Mono> · <Mono>{j.scope}</Mono>
                </div>
                <div className="queueMeta">
                  <span>
                    by <Mono>{j.createdBy}</Mono> · reason <Mono>{j.reason}</Mono>
                  </span>
                  <span>
                    created <Mono>{formatShort(j.createdAt)}</Mono>
                  </span>
                  <span>
                    started <Mono>{formatShort(j.startedAt)}</Mono>
                  </span>
                  <span>
                    finished <Mono>{formatShort(j.finishedAt)}</Mono>
                  </span>
                </div>
              </div>
              <div className="queueStatus">
                <Pill tone={statusTone(j.status)}>{j.status}</Pill>
              </div>
            </button>
          ))}
          {filtered.length === 0 ? <div className="muted">暂无任务</div> : null}
        </div>
      </div>

      <div className="card">
        <div className="sectionRow">
          <div className="title">日志</div>
          {selected ? (
            <div className="muted" style={{ marginLeft: 'auto' }}>
              job: <Mono>{selected}</Mono>
            </div>
          ) : null}
        </div>

        {selected ? (
          <>
            {selectedJob ? (
              <div className="muted" style={{ marginTop: 8 }}>
                <span>
                  type <Mono>{selectedJob.type}</Mono> · scope <Mono>{selectedJob.scope}</Mono>
                </span>
                {' · '}
                <span>
                  by <Mono>{selectedJob.createdBy}</Mono> · reason <Mono>{selectedJob.reason}</Mono>
                </span>
              </div>
            ) : null}
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
          </>
        ) : (
          <div className="muted">选择一条任务查看日志</div>
        )}

        {error ? <div className="error">{error}</div> : null}
      </div>
    </div>
  )
}
