import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  archiveDiscoveredProject,
  getStack,
  listDiscoveryProjects,
  listStacks,
  restoreDiscoveredProject,
  triggerCheck,
  triggerDiscoveryScan,
  type DiscoveredProject,
  type Service,
  type StackDetail,
  type StackListItem,
} from '../api'
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
  const [discovered, setDiscovered] = useState<DiscoveredProject[]>([])
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

    setDiscovered(await listDiscoveryProjects('exclude').catch(() => []))
  }, [onComposeHint])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
  }, [refresh])

  useEffect(() => {
    onTopActions(
      <>
        <Button
          variant="primary"
          disabled={busy}
          onClick={() => {
            void (async () => {
              setBusy(true)
              try {
                await triggerDiscoveryScan()
                await refresh()
              } catch (e: unknown) {
                setError(e instanceof Error ? e.message : String(e))
              } finally {
                setBusy(false)
              }
            })()
          }}
        >
          立即发现
        </Button>
        <Button
          variant="ghost"
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
          立即扫描更新
        </Button>
      </>,
    )
  }, [busy, onTopActions, refresh])

  const allUpdateRows = useMemo(() => {
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

  const visibleUpdateRows = useMemo(() => {
    return allUpdateRows.filter((r) => !r.service.archived)
  }, [allUpdateRows])

  const countsAll = useMemo(() => {
    const c: Record<RowStatus, number> = { updatable: 0, hint: 0, archMismatch: 0, blocked: 0 }
    for (const r of allUpdateRows) c[r.status] += 1
    return c
  }, [allUpdateRows])

  const countsVisible = useMemo(() => {
    const c: Record<RowStatus, number> = { updatable: 0, hint: 0, archMismatch: 0, blocked: 0 }
    for (const r of visibleUpdateRows) c[r.status] += 1
    return c
  }, [visibleUpdateRows])

  const discoveredUnregistered = useMemo(() => {
    return discovered.filter((p) => !p.stackId || p.status !== 'active')
  }, [discovered])

  return (
    <div className="page">
      <div className="statGrid">
        <div className="card statCard">
          <div className="label">可更新</div>
          <div className="statNum">{countsAll.updatable}</div>
          <div className="muted">已匹配 host arch</div>
        </div>
        <div className="card statCard">
          <div className="label">新版本提示</div>
          <div className="statNum">{countsAll.hint}</div>
          <div className="muted">同级别/新 minor</div>
        </div>
        <div className="card statCard">
          <div className="label">架构不匹配</div>
          <div className="statNum">{countsAll.archMismatch}</div>
          <div className="muted">仅提示，不允许更新</div>
        </div>
        <div className="card statCard">
          <div className="label">被阻止</div>
          <div className="statNum">{countsAll.blocked}</div>
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

      {discoveredUnregistered.length > 0 ? (
        <div className="card" style={{ marginBottom: 14 }}>
          <div className="sectionRow">
            <div className="title">Discovered（无 Stack / 异常）</div>
            <div className="muted" style={{ marginLeft: 'auto' }}>
              {discoveredUnregistered.length} projects
            </div>
          </div>
          <div className="svcGrid">
            {discoveredUnregistered.map((p) => (
              <div key={p.project} className="svcCard" style={{ cursor: 'default' }}>
                <div className="svcCardTop">
                  <div className="svcCardName">
                    <Mono>{p.project}</Mono>
                  </div>
                  <Pill tone={p.status === 'invalid' ? 'bad' : p.status === 'missing' ? 'warn' : 'muted'}>{p.status}</Pill>
                </div>
                <div className="svcCardMeta">
                  {p.lastError ? (
                    <div className="muted">
                      error <Mono>{p.lastError}</Mono>
                    </div>
                  ) : null}
                  {p.configFiles?.length ? (
                    <div className="muted">
                      config <Mono>{p.configFiles.join(', ')}</Mono>
                    </div>
                  ) : null}
                  <div className="muted">
                    archived <Mono>{String(p.archived)}</Mono>
                    {p.stackId ? (
                      <>
                        {' '}
                        · stackId <Mono>{p.stackId}</Mono>
                      </>
                    ) : null}
                  </div>
                  <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
                    <Button
                      variant="ghost"
                      disabled={busy}
                      onClick={() => {
                        void (async () => {
                          setBusy(true)
                          setError(null)
                          try {
                            await archiveDiscoveredProject(p.project)
                            await refresh()
                          } catch (e: unknown) {
                            setError(e instanceof Error ? e.message : String(e))
                          } finally {
                            setBusy(false)
                          }
                        })()
                      }}
                    >
                      归档
                    </Button>
                    <Button
                      variant="primary"
                      disabled={busy}
                      onClick={() => {
                        void (async () => {
                          setBusy(true)
                          setError(null)
                          try {
                            await restoreDiscoveredProject(p.project)
                            await refresh()
                          } catch (e: unknown) {
                            setError(e instanceof Error ? e.message : String(e))
                          } finally {
                            setBusy(false)
                          }
                        })()
                      }}
                    >
                      恢复
                    </Button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      <FilterChips
        value={filter}
        onChange={setFilter}
        items={[
          { key: 'all', label: '全部', count: visibleUpdateRows.length },
          { key: 'updatable', label: '可更新', count: countsVisible.updatable },
          { key: 'hint', label: '新版本提示', count: countsVisible.hint },
          { key: 'archMismatch', label: '架构不匹配', count: countsVisible.archMismatch },
          { key: 'blocked', label: '被阻止', count: countsVisible.blocked },
        ]}
      />

      {allUpdateRows.length !== visibleUpdateRows.length ? (
        <div className="muted" style={{ marginTop: -6, marginBottom: 10 }}>
          已归档服务默认不在列表中展示，但仍会计入上方统计。
        </div>
      ) : null}

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
            .filter((svc) => !svc.archived)
            .map((svc) => ({ svc, stt: serviceStatus(svc) }))
            .filter((x): x is { svc: Service; stt: RowStatus } => Boolean(x.stt))
          const visibleServices = allServices.filter((x) => filter === 'all' || x.stt === filter)

          const groupCounts: Record<RowStatus, number> = { updatable: 0, hint: 0, archMismatch: 0, blocked: 0 }
          for (const x of allServices) groupCounts[x.stt] += 1

          const archivedUpdateCount = d.services.filter((svc) => svc.archived && serviceStatus(svc)).length

          const isCollapsed = collapsed[st.id] ?? false
          const groupSummary = `${st.services} services · ${groupCounts.updatable} 可更新 · ${groupCounts.blocked} 被阻止${
            archivedUpdateCount > 0 ? ` · ${archivedUpdateCount} 已归档更新（隐藏）` : ''
          }`

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
