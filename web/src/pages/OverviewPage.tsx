import { useCallback, useEffect, useMemo, useState } from 'react'
import { getStack, listStacks, triggerCheck, type Service, type StackDetail, type StackListItem } from '../api'
import { navigate } from '../routes'
import { Button, Mono, Pill } from '../ui'
import { FilterChips } from '../Shell'

type RowStatus = 'updatable' | 'hint' | 'archMismatch' | 'blocked'
type Filter = 'all' | RowStatus

function serviceStatus(svc: Service): RowStatus | null {
  if (svc.ignore?.matched) return 'blocked'
  if (!svc.candidate) return null
  if (svc.candidate.archMatch === 'mismatch') return 'archMismatch'
  if (svc.candidate.archMatch === 'unknown') return 'hint'
  return 'updatable'
}

function statusTone(st: RowStatus): 'ok' | 'warn' | 'bad' | 'muted' {
  if (st === 'updatable') return 'ok'
  if (st === 'hint') return 'warn'
  if (st === 'archMismatch') return 'warn'
  return 'bad'
}

function statusLabel(st: RowStatus): string {
  if (st === 'updatable') return '可更新'
  if (st === 'hint') return '新版本提示'
  if (st === 'archMismatch') return '架构不匹配'
  return '被阻止'
}

function noteFor(svc: Service, st: RowStatus): string {
  if (st === 'blocked') return svc.ignore?.reason ?? '被阻止'
  if (st === 'archMismatch') return '仅提示，不允许更新'
  if (st === 'hint') return '同级别/新 minor'
  return '按当前 tag 前缀'
}

function formatShort(ts: string) {
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

export function OverviewPage(props: {
  onComposeHint: (hint: { path?: string; profile?: string; lastScan?: string }) => void
  onTopActions: (node: React.ReactNode) => void
}) {
  const { onComposeHint, onTopActions } = props
  const [filter, setFilter] = useState<Filter>('all')
  const [stacks, setStacks] = useState<StackListItem[]>([])
  const [details, setDetails] = useState<Record<string, StackDetail | undefined>>({})
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({})
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const refresh = useCallback(async () => {
    setError(null)
    const s = await listStacks()
    setStacks(s)
    const maxLastScan = s.map((x) => x.lastCheckAt).sort().at(-1)

    const ids = s.map((x) => x.id)
    const results = await Promise.all(
      ids.map(async (id) => {
        try {
          return [id, await getStack(id)] as const
        } catch {
          return [id, undefined] as const
        }
      }),
    )
    setDetails(Object.fromEntries(results))
    const first = results.find(([, d]) => Boolean(d))?.[1]
    onComposeHint({ path: first?.compose.composeFiles?.[0], profile: first?.name, lastScan: maxLastScan })

    setCollapsed((prev) => {
      const next = { ...prev }
      for (const st of s) {
        if (next[st.id] == null) next[st.id] = st.updates === 0
      }
      return next
    })
  }, [onComposeHint])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
  }, [refresh])

  useEffect(() => {
    onTopActions(
      <Button
        variant="primary"
        disabled={busy}
        onClick={() => {
          void (async () => {
            setBusy(true)
            try {
              await triggerCheck('all')
              await refresh()
            } catch (e: unknown) {
              setError(e instanceof Error ? e.message : String(e))
            } finally {
              setBusy(false)
            }
          })()
        }}
      >
        立即扫描
      </Button>,
    )
  }, [busy, onTopActions, refresh])

  const rows = useMemo(() => {
    const out: Array<{
      stackId: string
      stackName: string
      service: Service
      status: RowStatus
    }> = []
    for (const st of stacks) {
      const d = details[st.id]
      if (!d) continue
      for (const svc of d.services) {
        const stt = serviceStatus(svc)
        if (!stt) continue
        out.push({ stackId: st.id, stackName: d.name, service: svc, status: stt })
      }
    }
    return out
  }, [stacks, details])

  const counts = useMemo(() => {
    const c: Record<RowStatus, number> = { updatable: 0, hint: 0, archMismatch: 0, blocked: 0 }
    for (const r of rows) c[r.status] += 1
    return c
  }, [rows])

  return (
    <div className="page">
      <div className="statGrid">
        <div className="card statCard">
          <div className="label">可更新</div>
          <div className="statNum">{counts.updatable}</div>
          <div className="muted">已匹配 host arch</div>
        </div>
        <div className="card statCard">
          <div className="label">新版本提示</div>
          <div className="statNum">{counts.hint}</div>
          <div className="muted">同级别/新 minor</div>
        </div>
        <div className="card statCard">
          <div className="label">架构不匹配</div>
          <div className="statNum">{counts.archMismatch}</div>
          <div className="muted">仅提示，不允许更新</div>
        </div>
        <div className="card statCard">
          <div className="label">被阻止</div>
          <div className="statNum">{counts.blocked}</div>
          <div className="muted">忽略/策略阻止</div>
        </div>
      </div>

      <div className="sectionRow">
        <div className="title">更新候选</div>
        <div className="muted" style={{ marginLeft: 'auto' }}>
          {stacks.length > 0 ? (
            <span>
              stacks: <Mono>{stacks.length}</Mono> · last scan:{' '}
              <Mono>{stacks.map((s) => s.lastCheckAt).sort().at(-1) ?? '-'}</Mono>
            </span>
          ) : (
            <span>尚未注册 stack</span>
          )}
        </div>
      </div>

      <FilterChips
        value={filter}
        onChange={setFilter}
        items={[
          { key: 'all', label: '全部', count: rows.length },
          { key: 'updatable', label: '可更新', count: counts.updatable },
          { key: 'hint', label: '新版本提示', count: counts.hint },
          { key: 'archMismatch', label: '架构不匹配', count: counts.archMismatch },
          { key: 'blocked', label: '被阻止', count: counts.blocked },
        ]}
      />

      <div className="table">
        <div className="tableHeader">
          <div>Service</div>
          <div>Image</div>
          <div>Current → Candidate</div>
          <div>状态 / 备注</div>
        </div>

        {stacks.map((st) => {
          const d = details[st.id]
          if (!d) return null
          const allServices = d.services
            .map((svc) => ({ svc, stt: serviceStatus(svc) }))
            .filter((x): x is { svc: Service; stt: RowStatus } => Boolean(x.stt))
          const visibleServices = allServices.filter((x) => filter === 'all' || x.stt === filter)

          const groupCounts: Record<RowStatus, number> = { updatable: 0, hint: 0, archMismatch: 0, blocked: 0 }
          for (const x of allServices) groupCounts[x.stt] += 1

          const isCollapsed = collapsed[st.id] ?? false
          const groupSummary = `${allServices.length} services · ${groupCounts.updatable} 可更新 · ${groupCounts.blocked} 被阻止`

          return (
            <div key={st.id} className="tableGroup">
              <button
                className="groupHead"
                onClick={() => setCollapsed((prev) => ({ ...prev, [st.id]: !isCollapsed }))}
              >
                <div className="groupTitle">{d.name}</div>
                <div className="groupMeta">{groupSummary}</div>
                <div className="groupRight">
                  <span className="muted">{formatShort(st.lastCheckAt)}</span>
                  <span className="groupChevron">{isCollapsed ? '▸' : '▾'}</span>
                </div>
              </button>

              {!isCollapsed &&
                visibleServices.map(({ svc, stt }) => {
                  const current = `${svc.image.tag}${svc.image.digest ? `@${svc.image.digest}` : ''}`
                  const candidate = svc.candidate ? `${svc.candidate.tag}@${svc.candidate.digest}` : '-'
                  return (
                    <button
                      key={svc.id}
                      className="rowLine"
                      onClick={() => navigate({ name: 'service', stackId: st.id, serviceId: svc.id })}
                    >
                      <div className="svcName">{svc.name}</div>
                      <div className="mono">{svc.image.ref}</div>
                      <div className="mono">
                        <div>{current}</div>
                        <div>{candidate}</div>
                      </div>
                      <div className="statusCol">
                        <Pill tone={statusTone(stt)}>{statusLabel(stt)}</Pill>
                        <div className="muted">{noteFor(svc, stt)}</div>
                      </div>
                    </button>
                  )
                })}
            </div>
          )
        })}
      </div>

      {error ? <div className="error">{error}</div> : null}
      {busy ? <div className="muted">处理中…</div> : null}
    </div>
  )
}
