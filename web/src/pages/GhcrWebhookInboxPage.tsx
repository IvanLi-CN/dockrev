import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  listGitHubPackagesWebhookDeliveries,
  type ListGitHubPackagesWebhookDeliveriesResponse,
} from '../api'
import { useManagementEventBatch } from '../managementEvents'
import { navigate } from '../routes'
import { Button, Chip, Input, Mono, Pill, SelectField } from '../ui'
import { AsyncDataRegion, AsyncDataSkeleton } from '../components/AsyncDataRegion'
import { isAsyncDataBusy, type AsyncDataPhase, type AsyncDataSource, type AsyncDataTrigger } from '../asyncData'

type DeliveryFilter = 'all' | 'processed' | 'ignored' | 'rejected'

type DeliveryQuery = {
  page: number
  perPage: number
  filter: DeliveryFilter
  query: string
}

function sameDeliveryQuery(left: DeliveryQuery, right: DeliveryQuery) {
  return left.page === right.page && left.perPage === right.perPage && left.filter === right.filter && left.query === right.query
}

function formatShort(ts?: string | null): string {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function formatRepo(owner?: string | null, repo?: string | null, fullName?: string | null): string {
  if (fullName) return fullName
  if (owner && repo) return `${owner}/${repo}`
  return '-'
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function decisionLabel(decision: string): string {
  if (decision === 'processed') return '已处理'
  if (decision === 'ignored') return '已忽略'
  if (decision === 'rejected') return '已拒绝'
  return decision || '未知'
}

function decisionTone(decision: string): 'ok' | 'warn' | 'bad' | 'muted' {
  if (decision === 'processed') return 'ok'
  if (decision === 'ignored') return 'warn'
  if (decision === 'rejected') return 'bad'
  return 'muted'
}

function responseTone(status?: number | null): 'ok' | 'warn' | 'bad' | 'muted' {
  if (typeof status !== 'number') return 'muted'
  if (status >= 500) return 'bad'
  if (status >= 400) return 'warn'
  if (status >= 200 && status < 300) return 'ok'
  return 'muted'
}

function deliveryJobIds(jobId?: string | null, jobIds?: string[] | null): string[] {
  if (Array.isArray(jobIds) && jobIds.length > 0) return jobIds
  if (jobId) return [jobId]
  return []
}

function taskLabel(jobIds: string[]): string {
  if (jobIds.length > 1) return `查看 ${jobIds.length} 个任务`
  if (jobIds.length === 1) return '查看扫描任务'
  return '无关联任务'
}

const PER_PAGE_OPTIONS = [25, 50, 100]

const EMPTY_DELIVERIES: ListGitHubPackagesWebhookDeliveriesResponse = {
  page: 1,
  perPage: 50,
  total: 0,
  filteredTotal: 0,
  summary: {
    processed: 0,
    ignored: 0,
    rejected: 0,
  },
  deliveries: [],
}

export function GhcrWebhookInboxPage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const [committedQuery, setCommittedQuery] = useState<DeliveryQuery>({
    page: 1,
    perPage: 50,
    filter: 'all',
    query: '',
  })
  const { page, perPage, filter, query } = committedQuery
  const [searchInput, setSearchInput] = useState('')
  const [data, setData] = useState<ListGitHubPackagesWebhookDeliveriesResponse>(EMPTY_DELIVERIES)
  const [phase, setPhase] = useState<AsyncDataPhase>('initial-loading')
  const [source, setSource] = useState<AsyncDataSource>('none')
  const [trigger, setTrigger] = useState<AsyncDataTrigger>('background')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [openReasonDeliveryId, setOpenReasonDeliveryId] = useState<string | null>(null)
  const refreshRequestIdRef = useRef(0)
  const hasCommittedDataRef = useRef(false)
  const initialLoadStartedRef = useRef(false)

  const refresh = useCallback(async (nextQuery: DeliveryQuery = committedQuery, nextSource: AsyncDataSource = 'live', nextTrigger: AsyncDataTrigger = 'background') => {
    const requestId = ++refreshRequestIdRef.current
    setSource(nextSource)
    setTrigger(nextTrigger)
    setPhase(hasCommittedDataRef.current ? 'refreshing' : 'initial-loading')
    setError(null)
    try {
      const next = await listGitHubPackagesWebhookDeliveries({
        page: nextQuery.page,
        perPage: nextQuery.perPage,
        decision: nextQuery.filter,
        q: nextQuery.query,
      })
      if (requestId !== refreshRequestIdRef.current) return
      setData(next)
      setCommittedQuery((current) => sameDeliveryQuery(current, nextQuery) ? current : nextQuery)
      hasCommittedDataRef.current = true
      setPhase(next.deliveries.length === 0 ? 'ready-empty' : 'ready-data')
    } catch (e: unknown) {
      if (requestId !== refreshRequestIdRef.current) return
      setError(errorMessage(e))
      setPhase('error')
      throw e
    }
  }, [committedQuery])

  useEffect(() => {
    if (initialLoadStartedRef.current) return
    initialLoadStartedRef.current = true
    void refresh().catch(() => undefined)
  }, [refresh])

  useManagementEventBatch(({ events, resyncRequired }) => {
    if (!resyncRequired && !events.some((event) => event.domain === 'github_packages')) return
    void refresh(committedQuery).catch((reason: unknown) => setError(errorMessage(reason)))
  })

  const queryBusy = isAsyncDataBusy(phase, trigger)

  useEffect(() => {
    onTopActions(
      <Button
        variant="ghost"
        disabled={busy || queryBusy}
        onClick={() => {
          void (async () => {
            setBusy(true)
            try {
              await refresh(committedQuery, 'memory', 'user-action')
            } finally {
              setBusy(false)
            }
          })()
        }}
      >
        刷新
      </Button>,
    )
  }, [busy, committedQuery, onTopActions, queryBusy, refresh])

  const maxPage = useMemo(() => Math.max(1, Math.ceil(data.filteredTotal / perPage)), [data.filteredTotal, perPage])

  useEffect(() => {
    if (page <= maxPage) return
    void refresh({ ...committedQuery, page: maxPage }, 'memory').catch(() => undefined)
  }, [committedQuery, maxPage, page, refresh])

  useEffect(() => {
    setOpenReasonDeliveryId(null)
  }, [filter, page, perPage, query])

  useEffect(() => {
    if (!openReasonDeliveryId) return

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as HTMLElement | null
      if (target?.closest('.ghcrInboxDecisionBadgeInteractive')) return
      setOpenReasonDeliveryId(null)
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      setOpenReasonDeliveryId(null)
    }

    document.addEventListener('pointerdown', onPointerDown)
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      document.removeEventListener('keydown', onKeyDown)
    }
  }, [openReasonDeliveryId])

  const summaryItems = useMemo(
    () => [
      { label: '总记录', value: data.total },
      { label: '已处理', value: data.summary.processed },
      { label: '已忽略', value: data.summary.ignored },
      { label: '已拒绝', value: data.summary.rejected },
    ],
    [data.summary.ignored, data.summary.processed, data.summary.rejected, data.total],
  )

  return (
    <div className="page ghcrInboxPage">
      <AsyncDataRegion
        error={error}
        hasData={hasCommittedDataRef.current}
        label="正在刷新 GHCR Webhook 收件箱"
        onRetry={() => void refresh(committedQuery, 'memory', 'user-action').catch(() => undefined)}
        phase={phase}
        skeleton={<AsyncDataSkeleton className="ghcrInboxLoadingSkeleton" lines={8} />}
        source={source}
        trigger={trigger}
      >
      <div className="ghcrInboxSummaryGrid">
        {summaryItems.map((item) => (
          <div key={item.label} className="ghcrInboxSummaryItem">
            <div className="muted">{item.label}</div>
            <div className="ghcrInboxSummaryValue">
              <Mono>{phase === 'initial-loading' ? '—' : item.value}</Mono>
            </div>
          </div>
        ))}
      </div>

      <div className="ghcrInboxToolbar">
        <div className="chipRow ghcrInboxFilterRow">
          <Chip
            active={filter === 'all'}
            disabled={busy || queryBusy}
            onClick={() => {
              if (!busy && !queryBusy) void refresh({ ...committedQuery, filter: 'all', page: 1 }, 'memory', 'user-action').catch(() => undefined)
            }}
          >
            <span>全部</span>
            <span className="chipCount">{phase === 'initial-loading' ? '—' : data.total}</span>
          </Chip>
          <Chip
            active={filter === 'processed'}
            disabled={busy || queryBusy}
            onClick={() => {
              if (!busy && !queryBusy) void refresh({ ...committedQuery, filter: 'processed', page: 1 }, 'memory', 'user-action').catch(() => undefined)
            }}
          >
            <span>已处理</span>
            <span className="chipCount">{phase === 'initial-loading' ? '—' : data.summary.processed}</span>
          </Chip>
          <Chip
            active={filter === 'ignored'}
            disabled={busy || queryBusy}
            onClick={() => {
              if (!busy && !queryBusy) void refresh({ ...committedQuery, filter: 'ignored', page: 1 }, 'memory', 'user-action').catch(() => undefined)
            }}
          >
            <span>已忽略</span>
            <span className="chipCount">{phase === 'initial-loading' ? '—' : data.summary.ignored}</span>
          </Chip>
          <Chip
            active={filter === 'rejected'}
            disabled={busy || queryBusy}
            onClick={() => {
              if (!busy && !queryBusy) void refresh({ ...committedQuery, filter: 'rejected', page: 1 }, 'memory', 'user-action').catch(() => undefined)
            }}
          >
            <span>已拒绝</span>
            <span className="chipCount">{phase === 'initial-loading' ? '—' : data.summary.rejected}</span>
          </Chip>
        </div>

        <div className="ghcrInboxSearchForm">
          <Input
            className="input ghcrInboxSearch"
            onChange={(event) => setSearchInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== 'Enter') return
              event.preventDefault()
              void refresh({ ...committedQuery, page: 1, query: searchInput.trim() }, 'memory', 'user-action').catch(() => undefined)
            }}
            placeholder="搜索仓库 / 原因 / 任务"
            value={searchInput}
          />
          <Button
            variant="ghost"
            disabled={busy || queryBusy}
            onClick={() => {
              void refresh({ ...committedQuery, page: 1, query: searchInput.trim() }, 'memory', 'user-action').catch(() => undefined)
            }}
          >
            搜索
          </Button>
          <Button
            variant="ghost"
            disabled={queryBusy || (!query && !searchInput)}
            onClick={() => {
              setSearchInput('')
              void refresh({ ...committedQuery, page: 1, query: '' }, 'memory', 'user-action').catch(() => undefined)
            }}
          >
            清除
          </Button>
        </div>

        <div className="ghcrInboxPager">
          <label className="label" htmlFor="ghcr-inbox-per-page">
            每页
          </label>
          <SelectField
            className="select"
            disabled={queryBusy}
            id="ghcr-inbox-per-page"
            onChange={(value) => {
              const next = Number.parseInt(value, 10)
              void refresh({ ...committedQuery, page: 1, perPage: Number.isFinite(next) && next > 0 ? next : 50 }, 'memory', 'user-action').catch(() => undefined)
            }}
            options={PER_PAGE_OPTIONS.map((option) => ({ value: String(option), label: String(option) }))}
            value={String(perPage)}
          />
          <span className="muted">
            {phase === 'initial-loading' ? '正在加载记录…' : `第 ${page} / ${maxPage} 页（筛选后 ${data.filteredTotal} / 总计 ${data.total}）`}
          </span>
          <Button variant="ghost" disabled={queryBusy || busy || page <= 1} onClick={() => void refresh({ ...committedQuery, page: Math.max(1, page - 1) }, 'memory', 'user-action').catch(() => undefined)}>
            上一页
          </Button>
          <Button
            variant="ghost"
            disabled={queryBusy || busy || page >= maxPage}
            onClick={() => void refresh({ ...committedQuery, page: Math.min(maxPage, page + 1) }, 'memory', 'user-action').catch(() => undefined)}
          >
            下一页
          </Button>
        </div>

        <div className="chipRow ghcrInboxQuickLinks">
          <Button variant="ghost" onClick={() => navigate({ name: 'settings' })}>
            返回设置
          </Button>
          <Button variant="ghost" onClick={() => navigate({ name: 'ghcr-webhooks' })}>
            GHCR 状态
          </Button>
        </div>
      </div>

      <div className="ghcrInboxTable" role="table" aria-label="Webhook delivery 记录">
        <div className="ghcrInboxTableHeader" role="row">
          <div>接收时间</div>
          <div>事件</div>
          <div>仓库</div>
          <div>处理结果</div>
          <div>响应</div>
          <div>任务</div>
        </div>

        {phase === 'ready-empty' ? (
          <div className="ghcrInboxEmpty muted">
            {query || filter !== 'all' ? '当前筛选条件下没有记录' : '还没有收到 GHCR Webhook 请求'}
          </div>
        ) : null}

        {data.deliveries.map((delivery, index) => (
          <div key={delivery.deliveryId} className="ghcrInboxRow" role="row">
            <div className="ghcrInboxCell">
              <div>{formatShort(delivery.receivedAt)}</div>
              {delivery.attemptCount > 1 ? <div className="muted">尝试 {delivery.attemptCount} 次</div> : null}
            </div>
            <div className="ghcrInboxCell">
              <div>
                <Mono>{delivery.event ?? '-'}</Mono>
                <span className="muted"> / </span>
                <Mono>{delivery.action ?? '-'}</Mono>
              </div>
              <div className="muted">首次接收：{formatShort(delivery.firstReceivedAt)}</div>
            </div>
            <div className="ghcrInboxCell">
              <Mono>{formatRepo(delivery.owner, delivery.repo, delivery.fullName)}</Mono>
            </div>
            <div className="ghcrInboxCell ghcrInboxCellStatus">
              {delivery.reason ? (
                (() => {
                  const tooltipId = `ghcr-inbox-reason-${page}-${index}`
                  const tooltipAbove = data.deliveries.length > 3 && index >= data.deliveries.length - 3
                  const open = openReasonDeliveryId === delivery.deliveryId
                  const decisionText = decisionLabel(delivery.decision)
                  const className = [
                    'ghcrInboxDecisionBadge',
                    'ghcrInboxDecisionBadgeInteractive',
                    tooltipAbove ? 'ghcrInboxDecisionBadgeTop' : '',
                    open ? 'ghcrInboxDecisionBadgeOpen' : '',
                  ]
                    .filter(Boolean)
                    .join(' ')
                  return (
                    <button
                      type="button"
                      className={className}
                      onClick={() =>
                        setOpenReasonDeliveryId((current) => (current === delivery.deliveryId ? null : delivery.deliveryId))
                      }
                      aria-expanded={open}
                      aria-describedby={tooltipId}
                    >
                      <Pill tone={decisionTone(delivery.decision)}>{decisionText}</Pill>
                      <span className="ghcrInboxReasonPreview">{delivery.reason}</span>
                      <span className="ghcrInboxReasonHint" aria-hidden="true">
                        !
                      </span>
                      <span id={tooltipId} className="ghcrInboxReasonTooltip" role="tooltip">
                        {delivery.reason}
                      </span>
                    </button>
                  )
                })()
              ) : (
                <span className="ghcrInboxDecisionBadge">
                  <Pill tone={decisionTone(delivery.decision)}>{decisionLabel(delivery.decision)}</Pill>
                </span>
              )}
            </div>
            <div className="ghcrInboxCell ghcrInboxCellStatus">
              <Pill tone={responseTone(delivery.responseStatus)}>
                {typeof delivery.responseStatus === 'number' ? `HTTP ${delivery.responseStatus}` : '-'}
              </Pill>
            </div>
            <div className="ghcrInboxCell ghcrInboxCellTask">
              {(() => {
                const jobIds = deliveryJobIds(delivery.jobId, delivery.jobIds)
                if (jobIds.length === 0) {
                  return <div className="muted">{taskLabel(jobIds)}</div>
                }
                if (jobIds.length === 1) {
                  return (
                    <button
                      type="button"
                      className="linkButton"
                      title={`任务 ID: ${jobIds[0]}`}
                      onClick={() => navigate({ name: 'job', jobId: jobIds[0] })}
                    >
                      {taskLabel(jobIds)}
                    </button>
                  )
                }
                return (
                  <div className="ghcrInboxTaskList" aria-label={`关联任务 ${jobIds.length} 个`}>
                    {jobIds.map((jobId) => (
                      <button
                        key={jobId}
                        type="button"
                        className="linkButton ghcrInboxTaskLink"
                        title={`任务 ID: ${jobId}`}
                        onClick={() => navigate({ name: 'job', jobId })}
                      >
                        <span>{jobId}</span>
                      </button>
                    ))}
                  </div>
                )
              })()}
            </div>
          </div>
        ))}
      </div>

      </AsyncDataRegion>
    </div>
  )
}
