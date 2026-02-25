import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import {
  getVersionInferenceOverview,
  newVersionInferenceEventsSource,
  type VersionInferenceCacheRow,
  type VersionInferenceOverviewResponse,
  type VersionInferenceTaskProgress,
} from '../api'
import { Button, Mono, Pill } from '../ui'

type StatusFilter = 'all' | 'queued' | 'running' | 'ready' | 'stale' | 'all_failed'

const STATUS_FILTERS: readonly StatusFilter[] = ['all', 'queued', 'running', 'ready', 'stale', 'all_failed']
const PER_PAGE_OPTIONS = [20, 50, 100, 200] as const
const QUERY_DEBOUNCE_MS = 250
const SSE_RECONNECT_MS = 3_000
const SSE_REFRESH_DEBOUNCE_MS = 250

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

function sseStatusLabel(status: 'connecting' | 'open' | 'reconnecting'): string {
  if (status === 'open') return 'SSE 已连接'
  if (status === 'reconnecting') return 'SSE 重连中'
  return 'SSE 连接中'
}

function sseStatusTone(status: 'connecting' | 'open' | 'reconnecting'): 'ok' | 'warn' {
  if (status === 'open') return 'ok'
  return 'warn'
}

function knownPercent(progress?: VersionInferenceTaskProgress | null): number | null {
  if (!progress) return null
  const total = Number.isFinite(progress.total) ? Math.max(0, progress.total) : 0
  if (total <= 0) return null
  if (!Number.isFinite(progress.percent)) return null
  return Math.max(0, Math.min(100, Math.round(progress.percent)))
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
  const [sseStatus, setSseStatus] = useState<'connecting' | 'open' | 'reconnecting'>('connecting')
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

  const refreshRef = useRef(refresh)
  useEffect(() => {
    refreshRef.current = refresh
  }, [refresh])

  useEffect(() => {
    setLoading(true)
    void refresh({ silent: true })
  }, [refresh])

  useEffect(() => {
    let closed = false
    let es: EventSource | null = null
    let reconnectTimer: number | null = null
    let refreshTimer: number | null = null
    let lastEventId = 0

    const clearReconnectTimer = () => {
      if (reconnectTimer != null) window.clearTimeout(reconnectTimer)
      reconnectTimer = null
    }

    const clearRefreshTimer = () => {
      if (refreshTimer != null) window.clearTimeout(refreshTimer)
      refreshTimer = null
    }

    const scheduleRefresh = (delayMs: number) => {
      if (closed || refreshTimer != null) return
      refreshTimer = window.setTimeout(() => {
        refreshTimer = null
        void refreshRef.current({ silent: true })
      }, delayMs)
    }

    const scheduleReconnect = () => {
      if (closed || reconnectTimer != null) return
      reconnectTimer = window.setTimeout(() => {
        reconnectTimer = null
        connect()
      }, SSE_RECONNECT_MS)
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
      es = newVersionInferenceEventsSource(opts)
      setSseStatus(lastEventId > 0 ? 'reconnecting' : 'connecting')

      es.addEventListener('open', () => {
        setSseStatus('open')
        // Catch up once on subscribe so in-between updates are reflected immediately.
        scheduleRefresh(0)
      })

      es.addEventListener('version_inference_event', (evt: Event) => {
        trackEventId(evt)
        scheduleRefresh(SSE_REFRESH_DEBOUNCE_MS)
      })

      es.onerror = () => {
        if (closed) return
        setSseStatus('reconnecting')
        es?.close()
        es = null
        // Trigger one immediate sync before reconnecting to reduce stale windows.
        scheduleRefresh(0)
        scheduleReconnect()
      }
    }

    connect()

    return () => {
      closed = true
      clearReconnectTimer()
      clearRefreshTimer()
      es?.close()
    }
  }, [])

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
  const rows = useMemo(() => sortRows(overview?.rows ?? []), [overview?.rows])

  return (
    <div className="page versionInferencePage">
      <div className="card">
        <div className="sectionRow">
          <div className="title">任务与缓存总览</div>
          <div className="chipRow" style={{ marginLeft: 'auto' }}>
            <Pill tone={sseStatusTone(sseStatus)}>{sseStatusLabel(sseStatus)}</Pill>
            <Pill tone="muted">最近更新：{formatShort(lastRefreshAt)}</Pill>
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
        <div className="sectionRow versionInferenceListHead">
          <div className="title">统一状态列表（进行中 + 缓存）</div>
          <button
            type="button"
            className="versionInferenceSortHint"
            aria-label="排序说明"
            data-tip="执行中 > 排队中 > 已过期 > 全部失败 > 已就绪（同状态按更新时间倒序）"
          >
            ?
          </button>
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
