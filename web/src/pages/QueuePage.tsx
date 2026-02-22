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
  const p = job.progress
  if (!p) return null
  const current = Number.isFinite(p.current) ? Math.max(0, p.current) : 0
  const total = Number.isFinite(p.total) ? Math.max(0, p.total) : 0
  const phase = (p.phase ?? '').trim()
  const message = (p.message ?? '').trim()
  const target = (p.currentTarget ?? '').trim()
  const parts: string[] = []
  if (total > 0) parts.push(`${current}/${total}`)
  if (phase) parts.push(phase)
  if (message) parts.push(message)
  if (target) parts.push(target)
  return parts.length > 0 ? parts.join(' · ') : null
}

function getProgressPercent(job: JobListItem): number | null {
  const p = job.progress
  if (!p) return null
  // Frontend must not derive percent; only trust backend-provided percent when total is known.
  const total = Number.isFinite(p.total) ? Math.max(0, p.total) : 0
  if (total <= 0) return null
  if (!Number.isFinite(p.percent)) return null
  return Math.max(0, Math.min(100, Math.round(p.percent)))
}

function shouldShowFinishedAt(job: JobListItem): boolean {
  return job.status !== 'running' && Boolean(job.finishedAt)
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
          {filtered.map((j) => {
            const progressLabel = formatProgressLabel(j)
            const progressPercent = getProgressPercent(j)
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
                    <div
                      className={progressPercent === null ? 'queueProgressBar queueProgressBarIndeterminate' : 'queueProgressBar'}
                      role="progressbar"
                      aria-valuemin={0}
                      aria-valuemax={100}
                      aria-valuenow={progressPercent ?? undefined}
                      aria-valuetext={progressPercent === null ? 'running' : `${progressPercent}%`}
                    >
                      <div
                        className={progressPercent === null ? 'queueProgressFill queueProgressFillIndeterminate' : 'queueProgressFill'}
                        style={progressPercent === null ? undefined : { width: `${progressPercent}%` }}
                      />
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
