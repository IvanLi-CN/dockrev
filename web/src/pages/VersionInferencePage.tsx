import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  getVersionInferenceOverview,
  newVersionInferenceEventsSource,
  type VersionInferenceOverviewResponse,
  type VersionInferenceOverviewRow,
  type VersionInferenceTaskProgress,
  type VersionInferenceTaskState,
} from '../api'
import { Button, Mono, Pill } from '../ui'

type StreamMode = 'connecting' | 'live' | 'polling'
type StatusFilter = 'all' | 'missing' | 'queued' | 'running' | 'ready' | 'stale' | 'all_failed'

type VersionInferenceEventPayload = {
  type?: string
  key?: string
  imageRepo?: string
  hostPlatform?: string
  reason?: string
  ts?: string
  status?: string
  checkedAt?: string
  allFailed?: boolean
  phase?: string
  message?: string
  current?: number
  total?: number
  percent?: number
  updatedAt?: string
  latestEventId?: number
  deleted?: number
  durationMs?: number
  ok?: boolean
  error?: string
}

const STATUS_FILTERS: readonly StatusFilter[] = ['all', 'missing', 'queued', 'running', 'ready', 'stale', 'all_failed']
const PER_PAGE_OPTIONS = [20, 50, 100, 200] as const
const QUERY_DEBOUNCE_MS = 250
const SSE_ERROR_THRESHOLD = 3
const SSE_RECONNECT_MS = 3000
const SSE_REFRESH_DEBOUNCE_MS = 250
const SSE_FALLBACK_POLL_MS = 10_000
const RESYNC_NOTICE_TIMEOUT_MS = 4500

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null
}

function asString(v: unknown): string | null {
  return typeof v === 'string' && v.trim() ? v.trim() : null
}

function asNumber(v: unknown): number | null {
  return typeof v === 'number' && Number.isFinite(v) ? v : null
}

function formatShort(ts?: string | null): string {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function statusLabel(status: string): string {
  if (status === 'all') return '全部'
  if (status === 'missing') return '缺失'
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
  if (status === 'missing' || status === 'all_failed') return 'bad'
  return 'muted'
}

function streamModeLabel(mode: StreamMode): string {
  if (mode === 'live') return 'SSE 实时'
  if (mode === 'polling') return '轮询降级'
  return '连接中'
}

function streamModeTone(mode: StreamMode): 'ok' | 'warn' | 'bad' | 'muted' {
  if (mode === 'live') return 'ok'
  if (mode === 'polling') return 'warn'
  return 'muted'
}

function normalizeProgress(input: {
  phase: string
  message: string
  current: number
  total: number
  percent: number
  updatedAt: string
}): VersionInferenceTaskProgress {
  const current = Number.isFinite(input.current) ? Math.max(0, Math.round(input.current)) : 0
  const total = Number.isFinite(input.total) ? Math.max(0, Math.round(input.total)) : 0
  const percent = Number.isFinite(input.percent) ? Math.max(0, Math.min(100, Math.round(input.percent))) : 0
  return {
    phase: input.phase || 'running',
    message: input.message || '',
    current: total > 0 ? Math.min(current, total) : current,
    total,
    percent,
    updatedAt: input.updatedAt || new Date().toISOString(),
  }
}

function knownPercent(progress?: VersionInferenceTaskProgress | null): number | null {
  if (!progress) return null
  const total = Number.isFinite(progress.total) ? Math.max(0, progress.total) : 0
  if (total <= 0) return null
  if (!Number.isFinite(progress.percent)) return null
  return Math.max(0, Math.min(100, Math.round(progress.percent)))
}

function parseEventPayload(raw: unknown): VersionInferenceEventPayload | null {
  if (typeof raw !== 'string' || !raw) return null
  try {
    const parsed = JSON.parse(raw) as unknown
    if (!isRecord(parsed)) return null
    return parsed as VersionInferenceEventPayload
  } catch {
    return null
  }
}

function parseLastEventId(evt: Event): number | null {
  const idRaw = (evt as MessageEvent).lastEventId
  if (typeof idRaw !== 'string' || !idRaw.trim()) return null
  const parsed = Number.parseInt(idRaw, 10)
  if (!Number.isFinite(parsed) || parsed <= 0) return null
  return parsed
}

function sortTasks(tasks: VersionInferenceTaskState[]): VersionInferenceTaskState[] {
  const next = [...tasks]
  next.sort((a, b) => {
    const byUpdated = String(b.updatedAt || '').localeCompare(String(a.updatedAt || ''))
    if (byUpdated !== 0) return byUpdated
    return String(a.key || '').localeCompare(String(b.key || ''))
  })
  return next
}

function upsertTask(tasks: VersionInferenceTaskState[], nextTask: VersionInferenceTaskState): VersionInferenceTaskState[] {
  const idx = tasks.findIndex((task) => task.key === nextTask.key)
  if (idx < 0) return sortTasks([...tasks, nextTask])
  const next = [...tasks]
  next[idx] = nextTask
  return sortTasks(next)
}

function updateRowForTask(
  row: VersionInferenceOverviewRow,
  patch: Partial<VersionInferenceOverviewRow>,
): VersionInferenceOverviewRow {
  return {
    ...row,
    ...patch,
    progress: patch.progress ?? (patch.progress === null ? null : row.progress ?? null),
    reason: patch.reason ?? (patch.reason === null ? null : row.reason ?? null),
    checkedAt: patch.checkedAt ?? (patch.checkedAt === null ? null : row.checkedAt ?? null),
    updatedAt: patch.updatedAt ?? (patch.updatedAt === null ? null : row.updatedAt ?? null),
  }
}

function applyEventToOverview(
  prev: VersionInferenceOverviewResponse | null,
  payload: VersionInferenceEventPayload,
): VersionInferenceOverviewResponse | null {
  if (!prev || typeof payload.type !== 'string') return prev
  const type = payload.type
  const key = asString(payload.key)
  const ts = asString(payload.ts) ?? new Date().toISOString()
  const imageRepo = asString(payload.imageRepo) ?? ''
  const hostPlatform = asString(payload.hostPlatform) ?? ''
  const reason = asString(payload.reason) ?? ''

  if (type === 'task_enqueued' && key) {
    const nextTask: VersionInferenceTaskState = {
      key,
      imageRepo,
      hostPlatform,
      status: 'queued',
      reason,
      enqueuedAt: ts,
      startedAt: null,
      updatedAt: ts,
      progress: null,
    }
    return {
      ...prev,
      tasks: upsertTask(prev.tasks, nextTask),
      rows: prev.rows.map((row) =>
        row.key === key
          ? updateRowForTask(row, {
              status: 'queued',
              reason,
              updatedAt: ts,
              progress: null,
            })
          : row,
      ),
    }
  }

  if (type === 'task_started' && key) {
    const existing = prev.tasks.find((task) => task.key === key)
    const nextTask: VersionInferenceTaskState = {
      key,
      imageRepo: imageRepo || existing?.imageRepo || '',
      hostPlatform: hostPlatform || existing?.hostPlatform || '',
      status: 'running',
      reason: reason || existing?.reason || '',
      enqueuedAt: existing?.enqueuedAt || ts,
      startedAt: ts,
      updatedAt: ts,
      progress: existing?.progress ?? null,
    }
    return {
      ...prev,
      tasks: upsertTask(prev.tasks, nextTask),
      rows: prev.rows.map((row) =>
        row.key === key
          ? updateRowForTask(row, {
              status: 'running',
              reason: nextTask.reason || row.reason || null,
              updatedAt: ts,
            })
          : row,
      ),
    }
  }

  if (type === 'task_progress' && key) {
    const phase = asString(payload.phase) ?? 'running'
    const message = asString(payload.message) ?? ''
    const current = asNumber(payload.current) ?? 0
    const total = asNumber(payload.total) ?? 0
    const percent = asNumber(payload.percent) ?? 0
    const updatedAt = asString(payload.updatedAt) ?? ts
    const progress = normalizeProgress({ phase, message, current, total, percent, updatedAt })
    const existing = prev.tasks.find((task) => task.key === key)
    const nextTask: VersionInferenceTaskState = {
      key,
      imageRepo: imageRepo || existing?.imageRepo || '',
      hostPlatform: hostPlatform || existing?.hostPlatform || '',
      status: 'running',
      reason: reason || existing?.reason || '',
      enqueuedAt: existing?.enqueuedAt || ts,
      startedAt: existing?.startedAt || ts,
      updatedAt,
      progress,
    }
    return {
      ...prev,
      tasks: upsertTask(prev.tasks, nextTask),
      rows: prev.rows.map((row) =>
        row.key === key
          ? updateRowForTask(row, {
              status: 'running',
              reason: nextTask.reason || row.reason || null,
              updatedAt,
              progress,
            })
          : row,
      ),
    }
  }

  if (type === 'task_finished' && key) {
    const status = asString(payload.status) ?? ''
    const checkedAt = asString(payload.checkedAt)
    const allFailed = payload.allFailed === true
    const nextRows = prev.rows.map((row) => {
      if (row.key !== key) return row
      let nextStatus = row.status
      if (status === 'success') {
        nextStatus = allFailed ? 'all_failed' : 'ready'
      }
      const rowReason = status === 'error' ? asString(payload.error) ?? row.reason ?? null : row.reason ?? null
      return updateRowForTask(row, {
        status: nextStatus,
        reason: rowReason,
        checkedAt: checkedAt ?? row.checkedAt ?? null,
        updatedAt: ts,
        progress: null,
      })
    })
    return {
      ...prev,
      tasks: prev.tasks.filter((task) => task.key !== key),
      rows: nextRows,
    }
  }

  if (type === 'gc_ran') {
    const lastDeleted = asNumber(payload.deleted)
    const lastDurationMs = asNumber(payload.durationMs)
    const ok = payload.ok !== false
    return {
      ...prev,
      gc: {
        ...prev.gc,
        lastRunAt: ts,
        lastDeleted: lastDeleted ?? prev.gc.lastDeleted ?? null,
        lastDurationMs: lastDurationMs ?? prev.gc.lastDurationMs ?? null,
        lastError: ok ? null : asString(payload.error) ?? prev.gc.lastError ?? null,
      },
    }
  }

  return prev
}

function statusCount(summary: VersionInferenceOverviewResponse['summary'] | null, key: StatusFilter): number {
  if (!summary) return 0
  if (key === 'all') return summary.total
  if (key === 'missing') return summary.missing
  if (key === 'queued') return summary.queued
  if (key === 'running') return summary.running
  if (key === 'ready') return summary.ready
  if (key === 'stale') return summary.stale
  return summary.allFailed
}

export function VersionInferencePage(props: {
  onComposeHint?: (hint: { path?: string; profile?: string; lastScan?: string }) => void
  onTopActions: (node: React.ReactNode) => void
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
  const [streamMode, setStreamMode] = useState<StreamMode>('connecting')
  const [lastEventId, setLastEventId] = useState(0)
  const [lastEventAt, setLastEventAt] = useState<string | null>(null)
  const [resyncNotice, setResyncNotice] = useState<string | null>(null)
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
    async (opts?: { silent?: boolean; reason?: 'manual' | 'auto' | 'resync' }) => {
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
        setPage((prev) => {
          const normalized = Number.isFinite(next.page) ? Math.max(1, Math.round(next.page)) : prev
          return prev === normalized ? prev : normalized
        })
        if (opts?.reason === 'resync') {
          setResyncNotice('检测到事件偏移已过期，已完成全量同步。')
        }
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
    void refresh({ silent: true, reason: 'auto' })
  }, [refresh])

  useEffect(() => {
    if (!resyncNotice) return
    const timer = window.setTimeout(() => setResyncNotice(null), RESYNC_NOTICE_TIMEOUT_MS)
    return () => window.clearTimeout(timer)
  }, [resyncNotice])

  useEffect(() => {
    let closed = false
    let es: EventSource | null = null
    let errorStreak = 0
    let lastSeenEventId = 0
    let refreshTimer: number | null = null
    let pollTimer: number | null = null
    let reconnectTimer: number | null = null

    const clearRefreshTimer = () => {
      if (refreshTimer != null) window.clearTimeout(refreshTimer)
      refreshTimer = null
    }

    const clearReconnectTimer = () => {
      if (reconnectTimer != null) window.clearTimeout(reconnectTimer)
      reconnectTimer = null
    }

    const stopPolling = () => {
      if (pollTimer != null) window.clearInterval(pollTimer)
      pollTimer = null
    }

    const refreshSafely = async (reason: 'auto' | 'resync' = 'auto') => {
      await refresh({ silent: true, reason })
    }

    const scheduleRefresh = (delayMs: number, reason: 'auto' | 'resync' = 'auto') => {
      if (refreshTimer != null) return
      refreshTimer = window.setTimeout(() => {
        refreshTimer = null
        void refreshSafely(reason).catch(() => {})
      }, delayMs)
    }

    const startPolling = () => {
      if (pollTimer != null) return
      setStreamMode('polling')
      pollTimer = window.setInterval(() => {
        void refreshSafely('auto').catch(() => {})
      }, SSE_FALLBACK_POLL_MS)
    }

    const trackEventId = (evt: Event) => {
      const parsed = parseLastEventId(evt)
      if (!parsed) return
      lastSeenEventId = Math.max(lastSeenEventId, parsed)
      setLastEventId(lastSeenEventId)
      setLastEventAt(new Date().toISOString())
    }

    const connect = () => {
      if (closed) return
      setStreamMode('connecting')
      try {
        es = newVersionInferenceEventsSource(lastSeenEventId > 0 ? { afterId: lastSeenEventId } : undefined)
      } catch {
        startPolling()
        return
      }

      es.addEventListener('open', () => {
        errorStreak = 0
        setStreamMode('live')
        stopPolling()
        scheduleRefresh(0, 'auto')
      })

      es.addEventListener('version_inference_event', (evt: Event) => {
        trackEventId(evt)
        const payload = parseEventPayload((evt as MessageEvent).data)
        if (!payload) {
          scheduleRefresh(SSE_REFRESH_DEBOUNCE_MS, 'auto')
          return
        }

        setOverview((prev) => applyEventToOverview(prev, payload))

        if (payload.type === 'resync_required') {
          const latest = asNumber(payload.latestEventId)
          if (latest != null && latest > 0) {
            lastSeenEventId = Math.max(lastSeenEventId, Math.round(latest))
            setLastEventId(lastSeenEventId)
          }
          clearRefreshTimer()
          scheduleRefresh(0, 'resync')
          return
        }

        scheduleRefresh(SSE_REFRESH_DEBOUNCE_MS, 'auto')
      })

      es.onerror = () => {
        errorStreak += 1
        scheduleRefresh(0, 'auto')
        if (errorStreak < SSE_ERROR_THRESHOLD) return
        es?.close()
        es = null
        startPolling()
        if (reconnectTimer != null) return
        reconnectTimer = window.setTimeout(() => {
          reconnectTimer = null
          connect()
        }, SSE_RECONNECT_MS)
      }
    }

    connect()

    return () => {
      closed = true
      clearRefreshTimer()
      clearReconnectTimer()
      stopPolling()
      es?.close()
    }
  }, [refresh])

  useEffect(() => {
    onTopActions(
      <Button
        variant="ghost"
        disabled={manualBusy}
        onClick={() => {
          void refresh({ silent: false, reason: 'manual' })
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

  return (
    <div className="page versionInferencePage">
      <div className="card">
        <div className="sectionRow">
          <div className="title">任务与缓存总览</div>
          <div className="chipRow" style={{ marginLeft: 'auto' }}>
            <Pill tone={streamModeTone(streamMode)}>{streamModeLabel(streamMode)}</Pill>
            <Pill tone="muted">事件ID：{lastEventId > 0 ? String(lastEventId) : '-'}</Pill>
          </div>
        </div>
        <div className="muted" style={{ marginTop: 8 }}>
          {lastEventAt ? `最近事件：${formatShort(lastEventAt)}` : '等待事件流建立…'}
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
            <span>总条目</span>
            <strong>{summary?.total ?? 0}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>已就绪</span>
            <strong>{summary?.ready ?? 0}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>缺失 + 失败</span>
            <strong>{(summary?.missing ?? 0) + (summary?.allFailed ?? 0)}</strong>
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
      {resyncNotice ? <div className="success">{resyncNotice}</div> : null}

      <div className="versionInferenceColumns">
        <div className="card">
          <div className="sectionRow">
            <div className="title">缓存列表</div>
          </div>
          <div className="versionInferenceList">
            {loading && !overview ? (
              <div className="muted">正在加载…</div>
            ) : null}
            {!loading && overview && overview.rows.length === 0 ? (
              <div className="muted">当前筛选条件下没有数据</div>
            ) : null}

            {overview?.rows.map((row) => {
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
    </div>
  )
}
