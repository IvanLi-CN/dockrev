import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  listGitHubPackagesWebhookDeliveries,
  type GitHubPackagesWebhookDelivery,
  type ListGitHubPackagesWebhookDeliveriesResponse,
} from '../api'
import { navigate } from '../routes'
import { Button, Mono } from '../ui'

function formatShort(ts?: string | null): string {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function formatRepo(delivery: GitHubPackagesWebhookDelivery): string {
  if (delivery.fullName) return delivery.fullName
  if (delivery.owner && delivery.repo) return `${delivery.owner}/${delivery.repo}`
  return '-'
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

const EMPTY_DELIVERIES: ListGitHubPackagesWebhookDeliveriesResponse = {
  page: 1,
  perPage: 50,
  total: 0,
  deliveries: [],
}

export function GhcrWebhookInboxPage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const [page, setPage] = useState(1)
  const [perPage] = useState(50)
  const [data, setData] = useState<ListGitHubPackagesWebhookDeliveriesResponse>(EMPTY_DELIVERIES)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const refreshRequestIdRef = useRef(0)

  const refresh = useCallback(async () => {
    const requestId = ++refreshRequestIdRef.current
    setError(null)
    try {
      const next = await listGitHubPackagesWebhookDeliveries({ page, perPage })
      if (requestId !== refreshRequestIdRef.current) return
      setData(next)
    } catch (e: unknown) {
      if (requestId !== refreshRequestIdRef.current) return
      setError(errorMessage(e))
    }
  }, [page, perPage])

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

  const maxPage = useMemo(() => Math.max(1, Math.ceil(data.total / perPage)), [data.total, perPage])

  return (
    <div className="page">
      <div className="card">
        <div className="sectionRow">
          <div className="title">Webhook 收件箱</div>
          <div className="chipRow" style={{ marginLeft: 'auto' }}>
            <Button variant="ghost" onClick={() => navigate({ name: 'settings' })}>
              返回设置
            </Button>
            <Button variant="ghost" onClick={() => navigate({ name: 'ghcr-webhooks' })}>
              GHCR 状态
            </Button>
          </div>
        </div>

        <div className="queueMeta" style={{ marginTop: 10 }}>
          <span>
            总计 <Mono>{data.total}</Mono>
          </span>
          <span>
            页码 <Mono>{page}</Mono>
          </span>
          <span>
            每页 <Mono>{perPage}</Mono>
          </span>
        </div>

        <div className="queueList" style={{ marginTop: 12 }}>
          {data.deliveries.length === 0 ? <div className="muted">暂无 webhook 触发记录</div> : null}
          {data.deliveries.map((delivery) => (
            <div key={delivery.deliveryId} className="queueItem" style={{ cursor: 'default' }}>
              <div className="queueMain">
                <div className="queueTitle">
                  <Mono>{formatRepo(delivery)}</Mono>
                </div>
                <div className="queueMeta">
                  <span>
                    接收时间 <Mono>{formatShort(delivery.receivedAt)}</Mono>
                  </span>
                  <span>
                    投递 ID <Mono>{delivery.deliveryId}</Mono>
                  </span>
                </div>
              </div>
            </div>
          ))}
        </div>

        <div className="formActions" style={{ marginTop: 12, justifyContent: 'space-between' }}>
          <div className="muted">
            第 {page} 页（每页 {perPage}）
          </div>
          <div style={{ display: 'flex', gap: 10 }}>
            <Button variant="ghost" disabled={busy || page <= 1} onClick={() => setPage((p) => Math.max(1, p - 1))}>
              上一页
            </Button>
            <Button
              variant="ghost"
              disabled={busy || page >= maxPage}
              onClick={() => setPage((p) => Math.min(maxPage, p + 1))}
            >
              下一页
            </Button>
          </div>
        </div>

        {error ? <div className="error">{error}</div> : null}
      </div>
    </div>
  )
}
