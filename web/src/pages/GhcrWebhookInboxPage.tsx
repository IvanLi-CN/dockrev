import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  listGitHubPackagesWebhookDeliveries,
  type ListGitHubPackagesWebhookDeliveriesResponse,
} from '../api'
import { navigate } from '../routes'
import { Button, Chip, Mono, Pill } from '../ui'

type DeliveryFilter = 'all' | 'processed' | 'ignored' | 'rejected'

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

function taskLabel(decision: string): string {
  if (decision === 'processed') return 'Webhook 扫描任务'
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
  const [page, setPage] = useState(1)
  const [perPage, setPerPage] = useState(50)
  const [filter, setFilter] = useState<DeliveryFilter>('all')
  const [searchInput, setSearchInput] = useState('')
  const [query, setQuery] = useState('')
  const [data, setData] = useState<ListGitHubPackagesWebhookDeliveriesResponse>(EMPTY_DELIVERIES)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const refreshRequestIdRef = useRef(0)

  const refresh = useCallback(async () => {
    const requestId = ++refreshRequestIdRef.current
    setError(null)
    try {
      const next = await listGitHubPackagesWebhookDeliveries({
        page,
        perPage,
        decision: filter,
        q: query,
      })
      if (requestId !== refreshRequestIdRef.current) return
      setData(next)
    } catch (e: unknown) {
      if (requestId !== refreshRequestIdRef.current) return
      setError(errorMessage(e))
    }
  }, [filter, page, perPage, query])

  useEffect(() => {
    void refresh()
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

  const maxPage = useMemo(() => Math.max(1, Math.ceil(data.filteredTotal / perPage)), [data.filteredTotal, perPage])

  useEffect(() => {
    if (page <= maxPage) return
    setPage(maxPage)
  }, [maxPage, page])

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
      <div className="ghcrInboxSummaryGrid">
        {summaryItems.map((item) => (
          <div key={item.label} className="ghcrInboxSummaryItem">
            <div className="muted">{item.label}</div>
            <div className="ghcrInboxSummaryValue">
              <Mono>{item.value}</Mono>
            </div>
          </div>
        ))}
      </div>

      <div className="ghcrInboxToolbar">
        <div className="chipRow ghcrInboxFilterRow">
          <Chip
            active={filter === 'all'}
            onClick={() => {
              setFilter('all')
              setPage(1)
            }}
          >
            <span>全部</span>
            <span className="chipCount">{data.total}</span>
          </Chip>
          <Chip
            active={filter === 'processed'}
            onClick={() => {
              setFilter('processed')
              setPage(1)
            }}
          >
            <span>已处理</span>
            <span className="chipCount">{data.summary.processed}</span>
          </Chip>
          <Chip
            active={filter === 'ignored'}
            onClick={() => {
              setFilter('ignored')
              setPage(1)
            }}
          >
            <span>已忽略</span>
            <span className="chipCount">{data.summary.ignored}</span>
          </Chip>
          <Chip
            active={filter === 'rejected'}
            onClick={() => {
              setFilter('rejected')
              setPage(1)
            }}
          >
            <span>已拒绝</span>
            <span className="chipCount">{data.summary.rejected}</span>
          </Chip>
        </div>

        <div className="ghcrInboxSearchForm">
          <input
            className="input ghcrInboxSearch"
            placeholder="搜索仓库 / 原因 / 任务"
            value={searchInput}
            onChange={(event) => setSearchInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== 'Enter') return
              event.preventDefault()
              setPage(1)
              setQuery(searchInput.trim())
            }}
          />
          <Button
            variant="ghost"
            onClick={() => {
              setPage(1)
              setQuery(searchInput.trim())
            }}
          >
            搜索
          </Button>
          <Button
            variant="ghost"
            disabled={!query && !searchInput}
            onClick={() => {
              setSearchInput('')
              setQuery('')
              setPage(1)
            }}
          >
            清除
          </Button>
        </div>

        <div className="ghcrInboxPager">
          <label className="label" htmlFor="ghcr-inbox-per-page">
            每页
          </label>
          <select
            id="ghcr-inbox-per-page"
            className="select"
            value={perPage}
            onChange={(event) => {
              const next = Number.parseInt(event.target.value, 10)
              setPerPage(Number.isFinite(next) && next > 0 ? next : 50)
              setPage(1)
            }}
          >
            {PER_PAGE_OPTIONS.map((option) => (
              <option key={option} value={option}>
                {option}
              </option>
            ))}
          </select>
          <span className="muted">
            第 {page} / {maxPage} 页（筛选后 {data.filteredTotal} / 总计 {data.total}）
          </span>
          <Button variant="ghost" disabled={busy || page <= 1} onClick={() => setPage((value) => Math.max(1, value - 1))}>
            上一页
          </Button>
          <Button
            variant="ghost"
            disabled={busy || page >= maxPage}
            onClick={() => setPage((value) => Math.min(maxPage, value + 1))}
          >
            下一页
          </Button>
        </div>

        <div className="chipRow" style={{ marginLeft: 'auto' }}>
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

        {data.deliveries.length === 0 ? (
          <div className="ghcrInboxEmpty muted">
            {query || filter !== 'all' ? '当前筛选条件下没有记录' : '还没有收到 GHCR Webhook 请求'}
          </div>
        ) : null}

        {data.deliveries.map((delivery) => (
          <div key={delivery.deliveryId} className="ghcrInboxRow" role="row">
            <div className="ghcrInboxCell">
              <div>{formatShort(delivery.receivedAt)}</div>
              {delivery.attemptCount > 1 ? <div className="muted">重试 {delivery.attemptCount} 次</div> : null}
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
            <div className="ghcrInboxCell">
              <Pill tone={decisionTone(delivery.decision)}>{decisionLabel(delivery.decision)}</Pill>
              {delivery.reason ? <div className="muted">{delivery.reason}</div> : null}
            </div>
            <div className="ghcrInboxCell">
              <Pill tone={responseTone(delivery.responseStatus)}>
                {typeof delivery.responseStatus === 'number' ? `HTTP ${delivery.responseStatus}` : '-'}
              </Pill>
            </div>
            <div className="ghcrInboxCell ghcrInboxCellTask">
              {delivery.jobId ? (
                <button
                  type="button"
                  className="linkButton"
                  title={`任务 ID: ${delivery.jobId}`}
                  onClick={() => navigate({ name: 'job', jobId: delivery.jobId! })}
                >
                  {taskLabel(delivery.decision)}
                </button>
              ) : (
                <div className="muted">{taskLabel(delivery.decision)}</div>
              )}
              {delivery.jobId ? <div className="muted">状态页可查看执行细节</div> : null}
            </div>
          </div>
        ))}
      </div>

      {error ? <div className="error">{error}</div> : null}
    </div>
  )
}
