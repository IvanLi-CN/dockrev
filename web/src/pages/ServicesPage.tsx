import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  getStack,
  listIgnores,
  listStacks,
  listStacksArchived,
  restoreService,
  restoreStack,
  type IgnoreRule,
  type Service,
  type StackDetail,
  type StackListItem,
} from '../api'
import { navigate } from '../routes'
import { Button, Mono, Pill } from '../ui'

function formatShort(ts: string) {
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function svcTone(svc: Service): 'ok' | 'warn' | 'bad' | 'muted' {
  if (svc.ignore?.matched) return 'bad'
  if (!svc.candidate) return 'muted'
  if (svc.candidate.archMatch === 'mismatch') return 'warn'
  return 'ok'
}

function svcBadge(svc: Service): string {
  if (svc.ignore?.matched) return 'ignored'
  if (!svc.candidate) return 'ok'
  if (svc.candidate.archMatch === 'mismatch') return 'arch mismatch'
  return 'update'
}

export function ServicesPage(props: {
  onComposeHint: (hint: { path?: string; profile?: string; lastScan?: string }) => void
  onTopActions: (node: React.ReactNode) => void
}) {
  const { onComposeHint, onTopActions } = props
  const [stacks, setStacks] = useState<StackListItem[]>([])
  const [details, setDetails] = useState<Record<string, StackDetail | undefined>>({})
  const [ignores, setIgnores] = useState<IgnoreRule[]>([])
  const [search, setSearch] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const [archivedStacks, setArchivedStacks] = useState<StackListItem[]>([])
  const [archivedDetails, setArchivedDetails] = useState<Record<string, StackDetail | undefined>>({})

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
    setIgnores(await listIgnores())

    const first = results.find(([, d]) => Boolean(d))?.[1]
    onComposeHint({
      path: first?.compose.composeFiles?.[0],
      profile: first?.name,
      lastScan: maxLastScan,
    })

    const a = await listStacksArchived('only').catch(() => [])
    setArchivedStacks(a)
    const aIds = a.map((x) => x.id)
    const aResults = await Promise.all(
      aIds.map(async (id) => {
        try {
          return [id, await getStack(id)] as const
        } catch {
          return [id, undefined] as const
        }
      }),
    )
    setArchivedDetails(Object.fromEntries(aResults))
  }, [onComposeHint])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
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
            } catch (e: unknown) {
              setError(e instanceof Error ? e.message : String(e))
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

  const allServices = useMemo(() => {
    const out: Array<{ stackId: string; stackName: string; lastCheckAt: string; svc: Service }> = []
    for (const st of stacks) {
      const d = details[st.id]
      if (!d) continue
      for (const svc of d.services) {
        if (svc.archived) continue
        out.push({ stackId: st.id, stackName: d.name, lastCheckAt: st.lastCheckAt, svc })
      }
    }
    return out
  }, [stacks, details])

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    if (!q) return allServices
    return allServices.filter((x) => {
      const hay = `${x.stackName} ${x.svc.name} ${x.svc.image.ref} ${x.svc.image.tag}`.toLowerCase()
      return hay.includes(q)
    })
  }, [allServices, search])

  const ignoreByService = useMemo(() => {
    const m = new Map<string, number>()
    for (const r of ignores) {
      m.set(r.scope.serviceId, (m.get(r.scope.serviceId) ?? 0) + 1)
    }
    return m
  }, [ignores])

  const archivedServices = useMemo(() => {
    const out: Array<{ stackId: string; stackName: string; svc: Service }> = []
    for (const st of stacks) {
      const d = details[st.id]
      if (!d) continue
      for (const svc of d.services) {
        if (svc.archived) out.push({ stackId: st.id, stackName: d.name, svc })
      }
    }
    return out
  }, [details, stacks])

  return (
    <div className="page">
      <div className="card">
        <div className="sectionRow">
          <div className="title">服务</div>
          <div style={{ marginLeft: 'auto', display: 'flex', gap: 10, alignItems: 'center' }}>
            <input
              className="input"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="搜索 service / image / stack"
            />
            <div className="muted">
              {filtered.length}/{allServices.length}
            </div>
          </div>
        </div>

        <div className="svcGrid">
          {filtered.map((x) => (
            <button
              key={x.svc.id}
              className="svcCard"
              onClick={() => navigate({ name: 'service', stackId: x.stackId, serviceId: x.svc.id })}
            >
              <div className="svcCardTop">
                <div className="svcCardName">{x.svc.name}</div>
                <Pill tone={svcTone(x.svc)}>{svcBadge(x.svc)}</Pill>
              </div>
              <div className="svcCardMeta">
                <div className="muted">
                  stack <Mono>{x.stackName}</Mono>
                </div>
                <div className="muted">
                  image <Mono>{x.svc.image.ref}</Mono>
                </div>
                <div className="muted">
                  current <Mono>{x.svc.image.tag}</Mono>
                  {x.svc.candidate ? (
                    <>
                      {' '}
                      → candidate <Mono>{x.svc.candidate.tag}</Mono>
                    </>
                  ) : null}
                </div>
                <div className="muted">
                  last scan <Mono>{formatShort(x.lastCheckAt)}</Mono>
                  {ignoreByService.get(x.svc.id) ? (
                    <>
                      {' '}
                      · ignores <Mono>{ignoreByService.get(x.svc.id)}</Mono>
                    </>
                  ) : null}
                </div>
              </div>
            </button>
          ))}
          {filtered.length === 0 ? <div className="muted">无匹配结果</div> : null}
        </div>
      </div>

      <div className="card">
        <div className="sectionRow">
          <div className="title">已归档</div>
        </div>
        {archivedStacks.length === 0 && archivedServices.length === 0 ? <div className="muted">暂无归档对象</div> : null}

        {archivedStacks.length > 0 ? (
          <div style={{ marginTop: 10 }}>
            <div className="muted" style={{ marginBottom: 8 }}>
              已归档 stacks（按 stack 成组展示）
            </div>
            <div className="svcGrid">
              {archivedStacks.map((st) => {
                const d = archivedDetails[st.id]
                const title = d ? d.name : st.name
                return (
                  <div key={st.id} className="svcCard" style={{ cursor: 'default' }}>
                    <div className="svcCardTop">
                      <div className="svcCardName">{title}</div>
                      <Pill tone="muted">archived</Pill>
                    </div>
                    <div className="svcCardMeta">
                      <div className="muted">
                        id <Mono>{st.id}</Mono>
                      </div>
                      <div className="muted">
                        services <Mono>{st.services}</Mono> · archived services <Mono>{st.archivedServices ?? 0}</Mono> · updates{' '}
                        <Mono>{st.updates}</Mono>
                      </div>
                      <div className="muted">
                        last scan <Mono>{formatShort(st.lastCheckAt)}</Mono>
                      </div>
                      <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
                        <Button
                          variant="primary"
                          disabled={busy}
                          onClick={() => {
                            void (async () => {
                              setBusy(true)
                              setError(null)
                              try {
                                await restoreStack(st.id)
                                await refresh()
                              } catch (e: unknown) {
                                setError(e instanceof Error ? e.message : String(e))
                              } finally {
                                setBusy(false)
                              }
                            })()
                          }}
                        >
                          恢复 stack
                        </Button>
                      </div>
                    </div>
                  </div>
                )
              })}
            </div>
          </div>
        ) : null}

        {archivedServices.length > 0 ? (
          <div style={{ marginTop: 16 }}>
            <div className="muted" style={{ marginBottom: 8 }}>
              已归档 services（按所属 stack 聚合）
            </div>
            <div className="svcGrid">
              {archivedServices.map((x) => (
                <div key={x.svc.id} className="svcCard" style={{ cursor: 'default' }}>
                  <div className="svcCardTop">
                    <div className="svcCardName">{x.svc.name}</div>
                    <Pill tone="muted">archived</Pill>
                  </div>
                  <div className="svcCardMeta">
                    <div className="muted">
                      stack <Mono>{x.stackName}</Mono>
                    </div>
                    <div className="muted">
                      image <Mono>{x.svc.image.ref}</Mono> · current <Mono>{x.svc.image.tag}</Mono>
                    </div>
                    <div style={{ display: 'flex', gap: 8, marginTop: 8 }}>
                      <Button
                        variant="primary"
                        disabled={busy}
                        onClick={() => {
                          void (async () => {
                            setBusy(true)
                            setError(null)
                            try {
                              await restoreService(x.svc.id)
                              await refresh()
                            } catch (e: unknown) {
                              setError(e instanceof Error ? e.message : String(e))
                            } finally {
                              setBusy(false)
                            }
                          })()
                        }}
                      >
                        恢复 service
                      </Button>
                      <Button
                        variant="ghost"
                        disabled={busy}
                        onClick={() => navigate({ name: 'service', stackId: x.stackId, serviceId: x.svc.id })}
                      >
                        打开详情
                      </Button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        ) : null}
      </div>

      {error ? <div className="error">{error}</div> : null}
    </div>
  )
}
