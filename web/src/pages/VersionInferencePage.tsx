import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import {
  getVersionInferenceOverview,
  type VersionInferenceCacheRow,
  type VersionInferenceOverviewResponse,
  type VersionInferenceTask,
  type VersionInferenceTaskProgress,
} from '../api'
import { Button, Mono, Pill } from '../ui'

type StatusFilter = 'all' | 'queued' | 'running' | 'ready' | 'stale' | 'all_failed'

const STATUS_FILTERS: readonly StatusFilter[] = ['all', 'queued', 'running', 'ready', 'stale', 'all_failed']
const PER_PAGE_OPTIONS = [20, 50, 100, 200] as const
const QUERY_DEBOUNCE_MS = 250
const ACTIVE_POLL_MS = 2_000
const IDLE_POLL_MS = 15_000

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function formatShort(ts?: string | null): string {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function statusLabel(status: string): string {
  if (status === 'all') return '全部'
  if (status === 'queued') return '排队中'
  if (status === 'running') return '执行中'
  if (status === 'ready') return '已就绪'
  if (status === 'stale') return '已过期'
  if (status === 'all_failed') return '全部失败'
  return status || '-'
}

function statusTone(status: string): 'ok' | 'warn' | 'bad' | 'muted' {
  if (status === 'ready') return 'ok'
  if (status === 'queued' || status === 'running' || status === 'stale') return 'warn'
  if (status === 'all_failed') return 'bad'
  return 'muted'
}

function knownPercent(progress?: VersionInferenceTaskProgress | null): number | null {
  if (!progress) return null
  const total = Number.isFinite(progress.total) ? Math.max(0, progress.total) : 0
  if (total <= 0) return null
  if (!Number.isFinite(progress.percent)) return null
  return Math.max(0, Math.min(100, Math.round(progress.percent)))
}

function sortTasks(tasks: VersionInferenceTask[]): VersionInferenceTask[] {
  const next = [...tasks]
  next.sort((a, b) => {
    const byUpdated = String(b.updatedAt || '').localeCompare(String(a.updatedAt || ''))
    if (byUpdated !== 0) return byUpdated
    return String(a.key || '').localeCompare(String(b.key || ''))
  })
  return next
}

function statusCount(summary: VersionInferenceOverviewResponse['summary'] | null, key: StatusFilter): number {
  if (!summary) return 0
  if (key === 'queued') return summary.queued
  if (key === 'running') return summary.running
  if (key === 'ready') return summary.ready
  if (key === 'stale') return summary.stale
  if (key === 'all_failed') return summary.allFailed
  return summary.queued + summary.running + summary.ready + summary.stale + summary.allFailed
}

function rowSortValue(status: string): number {
  if (status === 'running') return 0
  if (status === 'queued') return 1
  if (status === 'stale') return 2
  if (status === 'all_failed') return 3
  if (status === 'ready') return 4
  return 9
}

function sortRows(rows: VersionInferenceCacheRow[]): VersionInferenceCacheRow[] {
  const next = [...rows]
  next.sort((a, b) => {
    const byStatus = rowSortValue(a.status) - rowSortValue(b.status)
    if (byStatus !== 0) return byStatus
    const byUpdated = String(b.updatedAt || '').localeCompare(String(a.updatedAt || ''))
    if (byUpdated !== 0) return byUpdated
    return String(a.key || '').localeCompare(String(b.key || ''))
  })
  return next
}

export function VersionInferencePage(props: {
  onComposeHint?: (hint: { path?: string; profile?: string; lastScan?: string }) => void
  onTopActions: (node: ReactNode) => void
}) {
  const { onComposeHint, onTopActions } = props
  const [overview, setOverview] = useState<VersionInferenceOverviewResponse | null>(null)
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all')
  const [queryInput, setQueryInput] = useState('')
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(1)
  const [perPage, setPerPage] = useState<number>(50)
  const [loading, setLoading] = useState(true)
  const [manualBusy, setManualBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [lastRefreshAt, setLastRefreshAt] = useState<string | null>(null)
  const refreshRequestIdRef = useRef(0)

  useEffect(() => {
    onComposeHint?.({})
  }, [onComposeHint])

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setPage(1)
      setQuery(queryInput.trim())
    }, QUERY_DEBOUNCE_MS)
    return () => window.clearTimeout(timer)
  }, [queryInput])

  const refresh = useCallback(
    async (opts?: { silent?: boolean }) => {
      const requestId = ++refreshRequestIdRef.current
      const silent = opts?.silent === true

      if (!silent) setManualBusy(true)
      setError(null)

      try {
        const next = await getVersionInferenceOverview({
          q: query || null,
          status: statusFilter === 'all' ? null : statusFilter,
          page,
          perPage,
        })
        if (requestId !== refreshRequestIdRef.current) return

        setOverview(next)
        setLastRefreshAt(new Date().toISOString())
        setPage((prev) => {
          const normalized = Number.isFinite(next.page) ? Math.max(1, Math.round(next.page)) : prev
          return prev === normalized ? prev : normalized
        })
      } catch (e: unknown) {
        if (requestId !== refreshRequestIdRef.current) return
        setError(errorMessage(e))
      } finally {
        if (requestId === refreshRequestIdRef.current) {
          setLoading(false)
          if (!silent) setManualBusy(false)
        }
      }
    },
    [page, perPage, query, statusFilter],
  )

  useEffect(() => {
    setLoading(true)
    void refresh({ silent: true })
  }, [refresh])

  const hasActiveTasks = (overview?.summary.running ?? 0) + (overview?.summary.queued ?? 0) > 0

  useEffect(() => {
    const pollMs = hasActiveTasks ? ACTIVE_POLL_MS : IDLE_POLL_MS
    const timer = window.setInterval(() => {
      void refresh({ silent: true })
    }, pollMs)
    return () => window.clearInterval(timer)
  }, [hasActiveTasks, refresh])

  useEffect(() => {
    onTopActions(
      <Button
        variant="ghost"
        disabled={manualBusy}
        onClick={() => {
          void refresh({ silent: false })
        }}
      >
        刷新
      </Button>,
    )
  }, [manualBusy, onTopActions, refresh])

  const totalPages = useMemo(() => {
    const total = overview?.total ?? 0
    const per = overview?.perPage ?? perPage
    return Math.max(1, Math.ceil(total / Math.max(1, per)))
  }, [overview?.perPage, overview?.total, perPage])

  const currentPage = overview?.page ?? page
  const summary = overview?.summary ?? null
  const tasks = useMemo(() => sortTasks(overview?.tasks ?? []), [overview?.tasks])
  const rows = useMemo(() => sortRows(overview?.rows ?? []), [overview?.rows])

  return (
    <div className="page versionInferencePage">
      <div className="card">
        <div className="sectionRow">
          <div className="title">任务与缓存总览</div>
          <div className="chipRow" style={{ marginLeft: 'auto' }}>
            <Pill tone={hasActiveTasks ? 'warn' : 'ok'}>{hasActiveTasks ? `高频刷新 ${ACTIVE_POLL_MS / 1000}s` : `低频刷新 ${IDLE_POLL_MS / 1000}s`}</Pill>
            <Pill tone="muted">最近刷新：{formatShort(lastRefreshAt)}</Pill>
          </div>
        </div>

        <div className="versionInferenceMetrics">
          <div className="versionInferenceMetric">
            <span>并发上限</span>
            <strong>{overview?.worker.maxConcurrency ?? 0}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>队列中</span>
            <strong>{overview?.worker.queued ?? 0}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>运行中</span>
            <strong>{overview?.worker.running ?? 0}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>进行中总数</span>
            <strong>{overview?.worker.inFlight ?? 0}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>缓存快照</span>
            <strong>{summary?.snapshotsTotal ?? 0}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>已就绪</span>
            <strong>{summary?.ready ?? 0}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>stale + all_failed</span>
            <strong>{(summary?.stale ?? 0) + (summary?.allFailed ?? 0)}</strong>
          </div>
        </div>

        <div className="versionInferenceGcMeta">
          <span>GC 保留 {overview?.gc.retentionDays ?? '-'} 天</span>
          <span>间隔 {overview?.gc.intervalSeconds ?? '-'}s</span>
          <span>最近执行 {formatShort(overview?.gc.lastRunAt ?? null)}</span>
          <span>最近删除 {overview?.gc.lastDeleted ?? 0}</span>
          {overview?.gc.lastError ? <span className="versionInferenceGcError">GC 错误：{overview.gc.lastError}</span> : null}
        </div>
      </div>

      <div className="card">
        <div className="sectionRow">
          <div className="title">筛选与分页</div>
        </div>
        <div className="versionInferenceControls">
          <input
            className="input versionInferenceSearch"
            placeholder="搜索镜像仓库（q）"
            value={queryInput}
            onChange={(e) => setQueryInput(e.target.value)}
          />

          <div className="chipRow">
            {STATUS_FILTERS.map((key) => (
              <button
                key={key}
                type="button"
                className={statusFilter === key ? 'chip chipActive' : 'chip'}
                onClick={() => {
                  setStatusFilter(key)
                  setPage(1)
                }}
              >
                <span>{statusLabel(key)}</span>
                <span className="chipCount">{statusCount(summary, key)}</span>
              </button>
            ))}
          </div>

          <div className="versionInferencePager">
            <label className="label" htmlFor="version-inference-per-page">
              每页
            </label>
            <select
              id="version-inference-per-page"
              className="select"
              value={perPage}
              onChange={(e) => {
                const next = Number.parseInt(e.target.value, 10)
                setPerPage(Number.isFinite(next) && next > 0 ? next : 50)
                setPage(1)
              }}
            >
              {PER_PAGE_OPTIONS.map((v) => (
                <option key={v} value={v}>
                  {v}
                </option>
              ))}
            </select>
            <span className="muted">
              第 {currentPage} / {totalPages} 页（总计 {overview?.total ?? 0}）
            </span>
            <Button variant="ghost" disabled={manualBusy || currentPage <= 1} onClick={() => setPage((p) => Math.max(1, p - 1))}>
              上一页
            </Button>
            <Button
              variant="ghost"
              disabled={manualBusy || currentPage >= totalPages}
              onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
            >
              下一页
            </Button>
          </div>
        </div>
      </div>

      {error ? <div className="error">{error}</div> : null}

      <div className="card">
        <div className="sectionRow">
          <div className="title">进行中任务</div>
        </div>
        <div className="versionInferenceList">
          {tasks.length === 0 ? <div className="muted">当前无 in-flight 任务</div> : null}
          {tasks.map((task) => {
            const progressPercent = knownPercent(task.progress)
            return (
              <div key={task.key} className="versionInferenceItem">
                <div className="versionInferenceItemHead">
                  <div className="versionInferenceItemTitle">
                    <Mono>{task.imageRepo}</Mono>
                  </div>
                  <Pill tone={statusTone(task.status)}>{statusLabel(task.status)}</Pill>
                </div>
                <div className="versionInferenceItemMeta">
                  <span>平台：{task.hostPlatform}</span>
                  <span>原因：{task.reason || '-'}</span>
                  <span>入队：{formatShort(task.enqueuedAt)}</span>
                  <span>开始：{formatShort(task.startedAt ?? null)}</span>
                  <span>更新：{formatShort(task.updatedAt)}</span>
                </div>
                {task.progress ? (
                  <>
                    <div className="versionInferenceProgressBar">
                      <div
                        className={
                          progressPercent == null
                            ? 'versionInferenceProgressFill versionInferenceProgressFillIndeterminate'
                            : 'versionInferenceProgressFill'
                        }
                        style={progressPercent == null ? undefined : { width: `${progressPercent}%` }}
                      />
                    </div>
                    <div className="versionInferenceProgressMeta">
                      <span>{task.progress.phase || '执行中'}</span>
                      <span>{task.progress.message || '-'}</span>
                      <span>
                        {task.progress.current}/{task.progress.total}
                      </span>
                      <span>{progressPercent == null ? '进行中' : `${progressPercent}%`}</span>
                    </div>
                  </>
                ) : (
                  <div className="muted">等待 worker 返回进度…</div>
                )}
              </div>
            )
          })}
        </div>
      </div>

      <div className="card">
        <div className="sectionRow">
          <div className="title">缓存状态列表</div>
        </div>
        <div className="versionInferenceList">
          {loading && !overview ? <div className="muted">正在加载…</div> : null}
          {!loading && overview && rows.length === 0 ? <div className="muted">当前筛选条件下没有数据</div> : null}

          {rows.map((row) => {
            const progressPercent = knownPercent(row.progress)
            return (
              <div key={row.key} className="versionInferenceItem">
                <div className="versionInferenceItemHead">
                  <div className="versionInferenceItemTitle">
                    <Mono>{row.imageRepo}</Mono>
                  </div>
                  <Pill tone={statusTone(row.status)}>{statusLabel(row.status)}</Pill>
                </div>
                <div className="versionInferenceItemMeta">
                  <span>平台：{row.hostPlatform}</span>
                  <span>服务数：{row.serviceCount}</span>
                  <span>检查时间：{formatShort(row.checkedAt ?? null)}</span>
                  <span>更新时间：{formatShort(row.updatedAt ?? null)}</span>
                </div>
                {row.reason ? <div className="muted">原因：{row.reason}</div> : null}
                {row.progress ? (
                  <>
                    <div className="versionInferenceProgressBar">
                      <div
                        className={
                          progressPercent == null
                            ? 'versionInferenceProgressFill versionInferenceProgressFillIndeterminate'
                            : 'versionInferenceProgressFill'
                        }
                        style={progressPercent == null ? undefined : { width: `${progressPercent}%` }}
                      />
                    </div>
                    <div className="versionInferenceProgressMeta">
                      <span>{row.progress.phase || '执行中'}</span>
                      <span>{row.progress.message || '-'}</span>
                      <span>
                        {row.progress.current}/{row.progress.total}
                      </span>
                      <span>{progressPercent == null ? '进行中' : `${progressPercent}%`}</span>
                    </div>
                  </>
                ) : null}
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}
