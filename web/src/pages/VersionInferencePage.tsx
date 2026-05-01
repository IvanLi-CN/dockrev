import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import {
  getVersionInferenceOverview,
  newVersionInferenceEventsSource,
  type VersionInferenceCacheRow,
  type VersionInferenceOverviewResponse,
  type VersionInferenceTaskProgress,
} from '../api'
import { Button, Input, Mono, Pill, SelectField, ToggleGroup, ToggleGroupItem } from '../ui'

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

function statusTone(status: string): 'ok' | 'warn' | 'bad' | 'muted' | 'info' {
  if (status === 'ready') return 'ok'
  if (status === 'running') return 'info'
  if (status === 'queued' || status === 'stale') return 'warn'
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

type ProgressSegment = {
  current: number
  total: number
  percent: number | null
}

type SegmentedProgress = {
  assignment: ProgressSegment
  result: ProgressSegment
}

function normalizeSegment(current: number, total: number, percent?: number): ProgressSegment {
  const safeTotal = Number.isFinite(total) ? Math.max(0, total) : 0
  const safeCurrent = Number.isFinite(current) ? Math.max(0, current) : 0
  if (safeTotal <= 0) {
    return {
      current: safeCurrent,
      total: safeTotal,
      percent: Number.isFinite(percent ?? NaN) ? Math.max(0, Math.min(100, Math.round(percent ?? 0))) : null,
    }
  }
  const fallbackPercent = Math.round((Math.min(safeCurrent, safeTotal) * 100) / safeTotal)
  const normalizedPercent = Number.isFinite(percent ?? NaN) ? Math.round(percent ?? fallbackPercent) : fallbackPercent
  return {
    current: Math.min(safeCurrent, safeTotal),
    total: safeTotal,
    percent: Math.max(0, Math.min(100, normalizedPercent)),
  }
}

function segmentedProgress(progress?: VersionInferenceTaskProgress | null): SegmentedProgress | null {
  if (!progress) return null
  const result = normalizeSegment(progress.resultCurrent ?? progress.current, progress.resultTotal ?? progress.total, progress.resultPercent ?? progress.percent)
  const assignmentRaw = normalizeSegment(
    progress.assignedCurrent ?? progress.current,
    progress.assignedTotal ?? progress.total,
    progress.assignedPercent ?? progress.percent,
  )
  const assignment: ProgressSegment = {
    current: Math.max(result.current, assignmentRaw.current),
    total: Math.max(result.total, assignmentRaw.total),
    percent:
      assignmentRaw.percent == null
        ? result.percent
        : result.percent == null
          ? assignmentRaw.percent
          : Math.max(result.percent, assignmentRaw.percent),
  }
  return { assignment, result }
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
  onLastScanHint?: (lastScan?: string) => void
  onTopActions: (node: ReactNode) => void
}) {
  const { onLastScanHint, onTopActions } = props
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
    onLastScanHint?.(undefined)
  }, [onLastScanHint])

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
    let hasOpenedOnce = false

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
      try {
        es = newVersionInferenceEventsSource(opts)
      } catch {
        setSseStatus('reconnecting')
        scheduleReconnect()
        return
      }
      setSseStatus(lastEventId > 0 ? 'reconnecting' : 'connecting')

      es.addEventListener('open', () => {
        hasOpenedOnce = true
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
        if (hasOpenedOnce) {
          // Only force-sync after at least one successful stream connect to avoid error-loop hammering.
          scheduleRefresh(0)
        }
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
  const gcTip = useMemo(() => {
    const gc = overview?.gc
    if (!gc) return 'GC 状态加载中'
    const parts = [
      `保留 ${gc.retentionDays ?? '-'} 天`,
      `间隔 ${gc.intervalSeconds ?? '-'}s`,
      `最近执行 ${formatShort(gc.lastRunAt ?? null)}`,
      `最近删除 ${gc.lastDeleted ?? 0}`,
    ]
    if (gc.lastError) parts.push(`错误：${gc.lastError}`)
    return `GC ${parts.join('；')}`
  }, [overview?.gc])

  return (
    <div className="page versionInferencePage">
      <div className="card">
        <div className="sectionRow versionInferenceSummaryHead">
          <div className="title versionInferenceSummaryTitle">任务与缓存总览</div>
          <button type="button" className="versionInferenceSortHint versionInferenceGcHint" aria-label="GC 说明" data-tip={gcTip}>
            ?
          </button>
          <div className="chipRow versionInferenceSummaryStatus">
            {overview?.gc.lastError ? <Pill tone="bad">GC 异常</Pill> : null}
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
      </div>

      <div className="card">
        <div className="sectionRow">
          <div className="title">筛选与分页</div>
        </div>
        <div className="versionInferenceControls">
          <Input
            className="input versionInferenceSearch"
            onChange={(e) => setQueryInput(e.target.value)}
            placeholder="搜索镜像仓库（q）"
            value={queryInput}
          />

          <ToggleGroup
            className="chipRow"
            onValueChange={(value) => {
              if (!value) return
              setStatusFilter(value as StatusFilter)
              setPage(1)
            }}
            type="single"
            value={statusFilter}
          >
            {STATUS_FILTERS.map((key) => (
              <ToggleGroupItem
                key={key}
                className={statusFilter === key ? 'chip chipActive' : 'chip'}
                value={key}
                variant="outline"
              >
                <span>{statusLabel(key)}</span>
                <span className="chipCount">{statusCount(summary, key)}</span>
              </ToggleGroupItem>
            ))}
          </ToggleGroup>

          <div className="versionInferencePager">
            <label className="label" htmlFor="version-inference-per-page">
              每页
            </label>
            <SelectField
              className="select"
              id="version-inference-per-page"
              onChange={(value) => {
                const next = Number.parseInt(value, 10)
                setPerPage(Number.isFinite(next) && next > 0 ? next : 50)
                setPage(1)
              }}
              options={PER_PAGE_OPTIONS.map((value) => ({ value: String(value), label: String(value) }))}
              value={String(perPage)}
            />
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
            const progress = segmentedProgress(row.progress)
            const assignmentPercent = progress?.assignment.percent ?? null
            const resultPercent = progress?.result.percent ?? null
            return (
              <div key={row.key} className="versionInferenceItem">
                <div className="versionInferenceItemHead">
                  <div className="versionInferenceItemTitle">
                    <Mono>{row.imageRepo}</Mono>
                  </div>
                  <Pill tone={statusTone(row.status)} breathing={row.status === 'running'}>
                    {statusLabel(row.status)}
                  </Pill>
                </div>
                <div className="versionInferenceItemMeta">
                  <span>平台：{row.hostPlatform}</span>
                  <span>服务数：{row.serviceCount}</span>
                  <span>检查时间：{formatShort(row.checkedAt ?? null)}</span>
                  <span>更新时间：{formatShort(row.updatedAt ?? null)}</span>
                </div>
                {row.reason ? <div className="muted">原因：{row.reason}</div> : null}
                {progress ? (
                  <>
                    <div className="versionInferenceProgressBar">
                      <div
                        className={
                          assignmentPercent == null
                            ? 'versionInferenceProgressFill versionInferenceProgressFillAssigned versionInferenceProgressFillIndeterminate'
                            : 'versionInferenceProgressFill versionInferenceProgressFillAssigned'
                        }
                        style={assignmentPercent == null ? undefined : { transform: `scaleX(${assignmentPercent / 100})` }}
                      />
                      <div
                        className={
                          resultPercent == null
                            ? 'versionInferenceProgressFill versionInferenceProgressFillResult versionInferenceProgressFillIndeterminate'
                            : 'versionInferenceProgressFill versionInferenceProgressFillResult'
                        }
                        style={resultPercent == null ? undefined : { transform: `scaleX(${resultPercent / 100})` }}
                      />
                    </div>
                    <div className="versionInferenceProgressMeta">
                      <span>{row.progress?.phase || '执行中'}</span>
                      <span>{row.progress?.message || '-'}</span>
                      <span>
                        任务内 {progress.assignment.current}/{progress.assignment.total}
                      </span>
                      <span>
                        成功解析 {progress.result.current}/{progress.result.total}
                      </span>
                      <span>{assignmentPercent == null ? '任务内进行中' : `任务内 ${assignmentPercent}%`}</span>
                      <span>{resultPercent == null ? '成功解析进行中' : `成功解析 ${resultPercent}%`}</span>
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
