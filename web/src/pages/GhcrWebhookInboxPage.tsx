import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { listGitHubPackagesWebhookInbox, type GitHubPackagesWebhookInboxItem } from '../api'
import { navigate } from '../routes'
import { Button, Mono, Pill } from '../ui'

function formatShort(ts?: string | null): string {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function outcomeTone(outcome: string): 'ok' | 'warn' | 'bad' | 'muted' {
  const v = (outcome ?? '').trim().toLowerCase()
  if (v === 'triggered') return 'ok'
  if (v === 'ignored') return 'muted'
  if (v === 'error' || v === 'failed') return 'bad'
  if (v) return 'warn'
  return 'muted'
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function fullName(item: GitHubPackagesWebhookInboxItem): string {
  const owner = (item.owner ?? '').trim()
  const repo = (item.repo ?? '').trim()
  if (owner && repo) return `${owner}/${repo}`
  return '-'
}

export function GhcrWebhookInboxPage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const [items, setItems] = useState<GitHubPackagesWebhookInboxItem[]>([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const refreshRequestIdRef = useRef(0)

  const refresh = useCallback(async () => {
    const requestId = ++refreshRequestIdRef.current
    const resp = await listGitHubPackagesWebhookInbox()
    if (requestId !== refreshRequestIdRef.current) return
    setItems(resp.items ?? [])
  }, [])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(errorMessage(e)))
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
              setError(null)
              await refresh()
            } catch (e: unknown) {
              setError(errorMessage(e))
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

  const sorted = useMemo(() => items.slice(0, 2000), [items])

  return (
    <div className="page">
      <div className="card">
        <div className="sectionRow">
          <div className="title">GHCR Webhook Inbox</div>
          <div className="chipRow" style={{ marginLeft: 'auto' }}>
            <Button variant="ghost" onClick={() => navigate({ name: 'ghcr-webhooks' })}>
              返回 GHCR Webhook
            </Button>
          </div>
        </div>

        <div className="muted" style={{ marginTop: 10 }}>
          仅记录验签通过且 <Mono>event=package/action=published</Mono> 的推送 · 展示最近 7 天 · DB 保留最近 30 天或最多 2000 条
        </div>

        <div className="queueList" style={{ marginTop: 12 }}>
          {sorted.length === 0 ? <div className="muted">暂无记录</div> : null}
          {sorted.map((it) => {
            const target = fullName(it)
            const canOpenJob = Boolean((it.jobId ?? '').trim())
            const row = (
              <>
                <div className="queueMain">
                  <div className="queueTitle">
                    <Mono>{target}</Mono>
                  </div>
                  <div className="queueMeta">
                    <span>
                      received <Mono>{formatShort(it.receivedAt)}</Mono>
                    </span>
                    <span>
                      delivery <Mono>{it.deliveryId}</Mono>
                    </span>
                    <span>
                      outcome <Mono>{it.outcome}</Mono>
                    </span>
                    {it.reason ? (
                      <span>
                        reason <Mono>{it.reason}</Mono>
                      </span>
                    ) : null}
                    {it.jobId ? (
                      <span>
                        job <Mono>{it.jobId}</Mono>
                      </span>
                    ) : null}
                  </div>
                </div>
                <div className="queueStatus">
                  <Pill tone={outcomeTone(it.outcome)}>{(it.outcome ?? '').trim() || 'unknown'}</Pill>
                </div>
              </>
            )

            if (canOpenJob) {
              return (
                <button key={it.deliveryId} className="queueItem" onClick={() => navigate({ name: 'job', jobId: it.jobId! })}>
                  {row}
                </button>
              )
            }

            return (
              <div key={it.deliveryId} className="queueItem" style={{ cursor: 'default' }}>
                {row}
              </div>
            )
          })}
        </div>

        {error ? <div className="error">{error}</div> : null}
      </div>
    </div>
  )
}

