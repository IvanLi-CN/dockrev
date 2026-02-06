import { useCallback, useEffect, useMemo, useState } from 'react'
import { listJobs, type JobListItem } from '../api'
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

export function QueuePage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [filter, setFilter] = useState<Filter>('all')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const refresh = useCallback(async () => {
    setError(null)
    setJobs(await listJobs())
  }, [])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
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
          {filtered.map((j) => (
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

        {error ? <div className="error">{error}</div> : null}
      </div>
    </div>
  )
}
