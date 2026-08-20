import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import {
  getVersionInferenceOverview,
  type VersionInferenceCacheRow,
  type VersionInferenceOverviewResponse,
  type VersionInferenceTaskProgress,
} from '../api'
import { ReadonlySnapshotNotice } from '../components/ReadonlySnapshotNotice'
import { useManagementEventBatch, useManagementEvents } from '../managementEvents'
import { usePwaStatus } from '../pwaStatus'
import { buildReadonlySnapshotKey, readReadonlySnapshot, writeReadonlySnapshot } from '../readonlySnapshotCache'
import { AsyncDataRegion, AsyncDataSkeleton } from '../components/AsyncDataRegion'
import { asyncFreshnessWindow, isAsyncDataBusy, type AsyncDataPhase, type AsyncDataSource, type AsyncDataTrigger } from '../asyncData'
import { Button, Input, Mono, Pill, SelectField, ToggleGroup, ToggleGroupItem } from '../ui'

type StatusFilter = 'all' | 'queued' | 'running' | 'ready' | 'stale' | 'all_failed'

type VersionInferenceQuery = {
  statusFilter: StatusFilter
  query: string
  page: number
  perPage: number
}

const STATUS_FILTERS: readonly StatusFilter[] = ['all', 'queued', 'running', 'ready', 'stale', 'all_failed']
const PER_PAGE_OPTIONS = [20, 50, 100, 200] as const
const QUERY_DEBOUNCE_MS = 250
const VERSION_INFERENCE_SNAPSHOT_KEY = buildReadonlySnapshotKey('queue', 'version-inference-overview')
const VERSION_INFERENCE_SNAPSHOT_STALE_MS = asyncFreshnessWindow('operational')

type VersionInferenceSnapshotPayload = {
  version: 2
  readiness: { overview: boolean }
  committedQueryKey: string
  query: VersionInferenceQuery
  overview: VersionInferenceOverviewResponse
}

function versionInferenceQueryKey(query: VersionInferenceQuery): string {
  return `${query.statusFilter}:${query.query}:${query.page}:${query.perPage}`
}

function isVersionInferenceQuery(value: unknown): value is VersionInferenceQuery {
  if (!value || typeof value !== 'object') return false
  const query = value as Record<string, unknown>
  return STATUS_FILTERS.includes(query.statusFilter as StatusFilter) &&
    typeof query.query === 'string' &&
    typeof query.page === 'number' && Number.isFinite(query.page) && query.page >= 1 &&
    typeof query.perPage === 'number' && PER_PAGE_OPTIONS.includes(query.perPage as typeof PER_PAGE_OPTIONS[number])
}

function isVersionInferenceSnapshotPayload(value: unknown): value is VersionInferenceSnapshotPayload {
  if (!value || typeof value !== 'object') return false
  const payload = value as Record<string, unknown>
  return payload.version === 2 && payload.readiness instanceof Object &&
    (payload.readiness as Record<string, unknown>).overview === true &&
    isVersionInferenceQuery(payload.query) &&
    payload.committedQueryKey === versionInferenceQueryKey(payload.query) &&
    Boolean(payload.overview)
}

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
  if (status === 'stale') return '需处理'
  if (status === 'all_failed') return '全部失败'
  return status || '-'
}

function statusTone(status: string): 'ok' | 'warn' | 'bad' | 'muted' | 'info' {
  if (status === 'ready') return 'ok'
  if (status === 'running') return 'info'
  if (status === 'queued') return 'warn'
  if (status === 'stale') return 'warn'
  if (status === 'all_failed') return 'bad'
  return 'muted'
}

function sseStatusLabel(status: 'connecting' | 'live' | 'stale'): string {
  if (status === 'live') return 'SSE 已连接'
  if (status === 'stale') return 'SSE 重连中'
  return 'SSE 连接中'
}

function sseStatusTone(status: 'connecting' | 'live' | 'stale'): 'ok' | 'warn' {
  if (status === 'live') return 'ok'
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
  const { isOnline } = usePwaStatus()
  const { connection: sseStatus } = useManagementEvents()
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
  const [, setSnapshotStatus] = useState<'missing' | 'fresh' | 'stale' | 'expired' | 'unsupported'>(
    'missing',
  )
  const [snapshotFetchedAt, setSnapshotFetchedAt] = useState<string | null>(null)
  const [snapshotAnchorFetchedAt, setSnapshotAnchorFetchedAt] = useState<string | null>(null)
  const [snapshotActive, setSnapshotActive] = useState(false)
  const [snapshotHydrated, setSnapshotHydrated] = useState(false)
  const [refreshSource, setRefreshSource] = useState<AsyncDataSource>('none')
  const [refreshTrigger, setRefreshTrigger] = useState<AsyncDataTrigger>('background')
  const refreshRequestIdRef = useRef(0)
  const snapshotActiveRef = useRef(snapshotActive)
  const committedQueryRef = useRef<VersionInferenceQuery>({ statusFilter, query, page, perPage })
  const retryQueryRef = useRef<VersionInferenceQuery>(committedQueryRef.current)
  const searchInitializedRef = useRef(false)
  snapshotActiveRef.current = snapshotActive
  committedQueryRef.current = { statusFilter, query, page, perPage }

  useEffect(() => {
    onLastScanHint?.(undefined)
  }, [onLastScanHint])

  useEffect(() => {
    let cancelled = false
    void (async () => {
      const snapshot = await readReadonlySnapshot<VersionInferenceSnapshotPayload>(VERSION_INFERENCE_SNAPSHOT_KEY)
      if (cancelled) return
      setSnapshotStatus(snapshot.status)
      setSnapshotFetchedAt(snapshot.record?.fetchedAt ?? null)
      setSnapshotAnchorFetchedAt(snapshot.record?.fetchedAt ?? null)
      if (snapshot.status !== 'fresh' || !isVersionInferenceSnapshotPayload(snapshot.record.payload)) {
        setSnapshotHydrated(true)
        return
      }
      const payload = snapshot.record.payload
      setOverview(payload.overview)
      setStatusFilter(payload.query.statusFilter)
      setQueryInput(payload.query.query)
      setQuery(payload.query.query)
      setPage(payload.query.page)
      setPerPage(payload.query.perPage)
      committedQueryRef.current = payload.query
      retryQueryRef.current = payload.query
      setLoading(false)
      setSnapshotActive(true)
      snapshotActiveRef.current = true
      setLastRefreshAt(snapshot.record.fetchedAt)
      setRefreshSource('fresh-snapshot')
      setSnapshotHydrated(true)
    })()
    return () => {
      cancelled = true
    }
  }, [])

  const refresh = useCallback(
    async (opts?: { silent?: boolean; source?: AsyncDataSource; trigger?: AsyncDataTrigger; query?: VersionInferenceQuery }) => {
      const requestId = ++refreshRequestIdRef.current
      const silent = opts?.silent === true
      const requestedQuery = opts?.query ?? committedQueryRef.current

      if (!silent) setManualBusy(true)
      setRefreshSource(snapshotActiveRef.current ? 'fresh-snapshot' : (opts?.source ?? 'live'))
      setRefreshTrigger(opts?.trigger ?? 'background')
      setLoading(true)
      setError(null)
      retryQueryRef.current = requestedQuery

      try {
        const next = await getVersionInferenceOverview({
          q: requestedQuery.query || null,
          status: requestedQuery.statusFilter === 'all' ? null : requestedQuery.statusFilter,
          page: requestedQuery.page,
          perPage: requestedQuery.perPage,
        })
        if (requestId !== refreshRequestIdRef.current) return

        setOverview(next)
        setLastRefreshAt(new Date().toISOString())
        setSnapshotActive(false)
        snapshotActiveRef.current = false
        setSnapshotAnchorFetchedAt(null)
        setStatusFilter(requestedQuery.statusFilter)
        setQuery(requestedQuery.query)
        setPerPage(requestedQuery.perPage)
        setPage(Number.isFinite(next.page) ? Math.max(1, Math.round(next.page)) : requestedQuery.page)
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
    [],
  )

  useEffect(() => {
    if (!snapshotHydrated) return
    if (!searchInitializedRef.current) {
      searchInitializedRef.current = true
      return
    }
    const timer = window.setTimeout(() => {
      void refresh({
        silent: true,
        source: 'memory',
        trigger: 'user-action',
        query: { ...committedQueryRef.current, page: 1, query: queryInput.trim() },
      })
    }, QUERY_DEBOUNCE_MS)
    return () => window.clearTimeout(timer)
  }, [queryInput, refresh, snapshotHydrated])

  useEffect(() => {
    if (!snapshotHydrated) return
    void refresh({ silent: true })
  }, [refresh, snapshotHydrated])

  useManagementEventBatch(({ events, resyncRequired }) => {
    if (!resyncRequired && !events.some((event) => event.domain === 'version_inference')) return
    void refresh({ silent: true })
  })

  useEffect(() => {
    onTopActions(
      <Button
        variant="ghost"
        disabled={manualBusy || !isOnline}
        onClick={() => {
          void refresh({ silent: false, source: 'memory', trigger: 'user-action' })
        }}
      >
        刷新
      </Button>,
    )
  }, [isOnline, manualBusy, onTopActions, refresh])

  useEffect(() => {
    if (!overview) return
    void writeReadonlySnapshot(VERSION_INFERENCE_SNAPSHOT_KEY, {
      version: 2,
      readiness: { overview: true },
      committedQueryKey: versionInferenceQueryKey({ statusFilter, query, page, perPage }),
      query: { statusFilter, query, page, perPage },
      overview,
    }, {
      staleAfterMs: VERSION_INFERENCE_SNAPSHOT_STALE_MS,
      fetchedAt: snapshotAnchorFetchedAt ? Date.parse(snapshotAnchorFetchedAt) || undefined : undefined,
    })
  }, [overview, page, perPage, query, snapshotAnchorFetchedAt, statusFilter])

  const totalPages = useMemo(() => {
    const total = overview?.total ?? 0
    const per = overview?.perPage ?? perPage
    return Math.max(1, Math.ceil(total / Math.max(1, per)))
  }, [overview?.perPage, overview?.total, perPage])

  const currentPage = overview?.page ?? page
  const summary = overview?.summary ?? null
  const rows = useMemo(() => sortRows(overview?.rows ?? []), [overview?.rows])
  const dataPhase: AsyncDataPhase = error
    ? 'error'
    : !overview
      ? 'initial-loading'
      : loading
        ? 'refreshing'
        : rows.length === 0
          ? 'ready-empty'
          : 'ready-data'
  const dataBusy = isAsyncDataBusy(dataPhase, refreshTrigger)
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
      {snapshotActive ? (
        <ReadonlySnapshotNotice
          tone={!isOnline ? 'warn' : 'info'}
          title={!isOnline ? '当前离线，显示已缓存的版本推测数据。' : '先显示已缓存的版本推测数据，后台会继续刷新。'}
          detail="SSE 连接、实时推测进度和 GC 最新结果仍以联网后的服务端状态为准。"
          fetchedAt={snapshotFetchedAt}
          actionLabel="重试刷新"
          actionDisabled={!isOnline || manualBusy}
          onAction={() => {
            void refresh({ silent: false, source: 'memory', trigger: 'user-action' })
          }}
        />
      ) : !overview && !loading && !isOnline ? (
        <ReadonlySnapshotNotice
          tone="bad"
          title="当前没有可用的离线版本推测数据。"
          detail="请恢复联网后重新加载该页面。"
        />
      ) : null}
      <AsyncDataRegion
        error={error}
        hasData={Boolean(overview)}
        label="正在刷新版本推测状态"
        onRetry={() => void refresh({ silent: false, source: 'memory', trigger: 'user-action', query: retryQueryRef.current })}
        phase={dataPhase}
        skeleton={<AsyncDataSkeleton className="versionInferenceLoadingSkeleton" lines={8} />}
        source={refreshSource}
        trigger={refreshTrigger}
      >
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
            <strong>{overview ? overview.worker.maxConcurrency : '—'}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>队列中</span>
            <strong>{overview ? overview.worker.queued : '—'}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>运行中</span>
            <strong>{overview ? overview.worker.running : '—'}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>进行中总数</span>
            <strong>{overview ? overview.worker.inFlight : '—'}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>缓存快照</span>
            <strong>{summary ? summary.snapshotsTotal : '—'}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>已就绪</span>
            <strong>{summary ? summary.ready : '—'}</strong>
          </div>
          <div className="versionInferenceMetric">
            <span>失败</span>
            <strong>{summary ? summary.allFailed : '—'}</strong>
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
            disabled={dataBusy}
            onChange={(e) => setQueryInput(e.target.value)}
            placeholder="搜索镜像仓库（q）"
            value={queryInput}
          />

          <ToggleGroup
            className="chipRow"
            onValueChange={(value) => {
              if (dataBusy) return
              if (!value) return
              void refresh({
                silent: true,
                source: 'memory',
                trigger: 'user-action',
                query: { ...committedQueryRef.current, statusFilter: value as StatusFilter, page: 1 },
              })
            }}
            type="single"
            value={statusFilter}
          >
            {STATUS_FILTERS.map((key) => (
              <ToggleGroupItem
                key={key}
                className={statusFilter === key ? 'chip chipActive' : 'chip'}
                disabled={dataBusy}
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
              disabled={dataBusy}
              id="version-inference-per-page"
              onChange={(value) => {
                const next = Number.parseInt(value, 10)
                void refresh({
                  silent: true,
                  source: 'memory',
                  trigger: 'user-action',
                  query: {
                    ...committedQueryRef.current,
                    page: 1,
                    perPage: Number.isFinite(next) && next > 0 ? next : 50,
                  },
                })
              }}
              options={PER_PAGE_OPTIONS.map((value) => ({ value: String(value), label: String(value) }))}
              value={String(perPage)}
            />
            <span className="muted">
              {overview ? `第 ${currentPage} / ${totalPages} 页（总计 ${overview.total}）` : '正在加载分页…'}
            </span>
            <Button
              variant="ghost"
              disabled={dataBusy || manualBusy || currentPage <= 1}
              onClick={() => void refresh({
                silent: true,
                source: 'memory',
                trigger: 'user-action',
                query: { ...committedQueryRef.current, page: Math.max(1, currentPage - 1) },
              })}
            >
              上一页
            </Button>
            <Button
              variant="ghost"
              disabled={dataBusy || manualBusy || currentPage >= totalPages}
              onClick={() => void refresh({
                silent: true,
                source: 'memory',
                trigger: 'user-action',
                query: { ...committedQueryRef.current, page: Math.min(totalPages, currentPage + 1) },
              })}
            >
              下一页
            </Button>
          </div>
        </div>
      </div>

      <div className="card">
        <div className="sectionRow versionInferenceListHead">
          <div className="title">统一状态列表（进行中 + 缓存）</div>
          <button
            type="button"
            className="versionInferenceSortHint"
            aria-label="排序说明"
            data-tip="执行中 > 排队中 > 全部失败 > 已就绪（同状态按更新时间倒序）"
          >
            ?
          </button>
        </div>
        <div className="versionInferenceList">
          {dataPhase === 'ready-empty' ? <div className="muted">当前筛选条件下没有数据</div> : null}

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
      </AsyncDataRegion>
    </div>
  )
}
