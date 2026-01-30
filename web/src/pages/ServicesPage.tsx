import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  getStack,
  listStacks,
  listStacksArchived,
  restoreService,
  restoreStack,
  triggerCheck,
  triggerUpdate,
  ApiError,
  type Service,
  type StackDetail,
  type StackListItem,
} from '../api'
import { navigate } from '../routes'
import { Button, Mono, Pill, StatusRemark } from '../ui'
import { isDockrevImageRef, selfUpgradeBaseUrl } from '../runtimeConfig'
import { useSupervisorHealth } from '../useSupervisorHealth'
import { serviceRowStatus, type RowStatus } from '../updateStatus'
import { UpdateCandidateFilters, type UpdateCandidateFilter } from '../components/UpdateCandidateFilters'
import { UpdateTargetSelect } from '../components/UpdateTargetSelect'
import { useConfirm } from '../confirm'

function formatShort(ts: string) {
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

// RowStatus is shared via ../updateStatus

function formatGroupSummary(services: number, counts: Record<Exclude<RowStatus, 'ok'>, number>) {
  const parts: string[] = [`${services} services`]
  if (counts.updatable > 0) parts.push(`${counts.updatable} 可更新`)
  if (counts.crossTag > 0) parts.push(`${counts.crossTag} 跨标签版本`)
  if (counts.hint > 0) parts.push(`${counts.hint} 需确认`)
  if (counts.archMismatch > 0) parts.push(`${counts.archMismatch} 架构不匹配`)
  if (counts.blocked > 0) parts.push(`${counts.blocked} 被阻止`)
  return parts.join(' · ')
}

function splitImageRef(ref: string): { registry: string; name: string } {
  const s = ref.trim()
  const withoutDigest = s.includes('@') ? s.split('@', 1)[0] : s
  const firstSlash = withoutDigest.indexOf('/')
  if (firstSlash < 0) {
    return { registry: 'docker.io', name: withoutDigest }
  }
  const firstSeg = withoutDigest.slice(0, firstSlash)
  const rest = withoutDigest.slice(firstSlash + 1)
  const isRegistry = firstSeg.includes('.') || firstSeg.includes(':') || firstSeg === 'localhost'
  if (isRegistry) return { registry: firstSeg, name: rest }
  return { registry: 'docker.io', name: withoutDigest }
}

function formatImageName(name: string, tag: string | null | undefined): string {
  const t = (tag ?? '').trim()
  if (!t) return name
  if (name.includes('@')) return name
  const lastSlash = name.lastIndexOf('/')
  const lastColon = name.lastIndexOf(':')
  if (lastColon > lastSlash) return name
  if (t.startsWith('sha256:')) return `${name}@${t}`
  return `${name}:${t}`
}

function splitImageNameForDisplay(
  name: string,
  tag: string | null | undefined,
): { base: string; suffix: string } {
  const n = name.trim()
  if (!n) return { base: '', suffix: '' }

  const at = n.indexOf('@')
  if (at >= 0) return { base: n.slice(0, at), suffix: n.slice(at) }

  const lastSlash = n.lastIndexOf('/')
  const lastColon = n.lastIndexOf(':')
  if (lastColon > lastSlash) return { base: n.slice(0, lastColon), suffix: n.slice(lastColon) }

  const t = (tag ?? '').trim()
  if (!t) return { base: n, suffix: '' }
  if (t.startsWith('sha256:')) return { base: n, suffix: `@${t}` }
  return { base: n, suffix: `:${t}` }
}

function formatTagDisplay(tag: string, resolvedTag: string | null | undefined): string {
  const r = (resolvedTag ?? '').trim()
  return r && r !== tag ? r : tag
}

function formatTagTooltip(
  tag: string,
  digest: string | null | undefined,
  resolvedTag: string | null | undefined,
  resolvedTags: string[] | null | undefined,
): string | undefined {
  const inferred = (resolvedTag ?? '').trim()
  const lines: string[] = []

  const digestSuffix = digest ? (digest.includes(':') ? digest : `sha256:${digest}`) : null

  if (inferred && inferred !== tag) {
    lines.push(digestSuffix ? `${inferred}@${digestSuffix}` : inferred)
    lines.push(`原始标签: ${tag}`)
  } else {
    lines.push(digestSuffix ? `${tag}@${digestSuffix}` : tag)
  }

  if (resolvedTags && resolvedTags.length > 1) {
    lines.push(`resolvedTags: ${resolvedTags.join(', ')}`)
  }

  return lines.join('\n')
}

function isDockrevService(svc: Service): boolean {
  return isDockrevImageRef(svc.image.ref)
}

function StackIcon(props: { variant: 'collapsed' | 'expanded' }) {
  return (
    <svg className="stackIcon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {props.variant === 'expanded' ? (
        <path d="m5 19l2.757-7.351A1 1 0 0 1 8.693 11H21a1 1 0 0 1 .986 1.164l-.996 5.211A2 2 0 0 1 19.026 19za2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h4l3 3h7a2 2 0 0 1 2 2v2" />
      ) : (
        <path d="M5 4h4l3 3h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2" />
      )}
    </svg>
  )
}

function GroupGuide(props: { rows: number }) {
  if (props.rows <= 0) return null
  const rowHeight = 52
  const gap = 4
  const bulletGap = 10
  const topSeg = rowHeight / 2 - bulletGap / 2
  const midSeg = rowHeight + gap - bulletGap

  const segments: Array<{ top: number; height: number }> = []
  let y = 36 + gap
  segments.push({ top: y, height: topSeg })
  y += topSeg + bulletGap
  for (let i = 0; i < props.rows - 1; i += 1) {
    segments.push({ top: y, height: midSeg })
    y += midSeg + bulletGap
  }
  segments.push({ top: y, height: topSeg })

  return (
    <div className="groupGuide" aria-hidden="true">
      {segments.map((s, idx) => (
        <span key={idx} className="groupGuideSeg" style={{ top: s.top, height: s.height }} />
      ))}
    </div>
  )
}

export function ServicesPage(props: {
  onComposeHint: (hint: { path?: string; profile?: string; lastScan?: string }) => void
  onTopActions: (node: React.ReactNode) => void
}) {
  const { onComposeHint, onTopActions } = props
  const confirm = useConfirm()
  const [stacks, setStacks] = useState<StackListItem[]>([])
  const [details, setDetails] = useState<Record<string, StackDetail | undefined>>({})
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({})
  const [search, setSearch] = useState('')
  const [filter, setFilter] = useState<UpdateCandidateFilter>('all')
  const [error, setError] = useState<string | null>(null)
  const [noticeJobId, setNoticeJobId] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const supervisor = useSupervisorHealth()
  const selfUpgradeUrl = useMemo(() => selfUpgradeBaseUrl(), [])

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

    const first = results.find(([, d]) => Boolean(d))?.[1]
    onComposeHint({
      path: first?.compose.composeFiles?.[0],
      profile: first?.name,
      lastScan: maxLastScan,
    })

    setCollapsed((prev) => {
      const next = { ...prev }
      for (const st of s) {
        if (next[st.id] == null) next[st.id] = false
      }
      return next
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
      <>
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

  const triggerApply = useCallback(
    async (input: {
      scope: 'stack' | 'service'
      stackId: string
      serviceId?: string
      targetLabel: string
      targetTag?: string
      targetDigest?: string | null
      getTarget?: () => { targetTag?: string; targetDigest?: string | null }
      confirmBody?: React.ReactNode
      confirmTitle?: string
    }) => {
      const scopeLabel = input.scope === 'stack' ? 'stack' : 'service'
      const confirmVariant = input.scope === 'service' ? 'primary' : 'danger'
      const badgeText = input.scope === 'stack' ? '批量更新' : '将更新并重启'
      const badgeTone = input.scope === 'service' ? 'warn' : 'bad'
      const ok = await confirm({
        title: input.confirmTitle ?? '确认执行更新？',
        body:
          input.confirmBody ?? (
            <>
              <div className="modalLead">将拉取镜像并重启容器；失败可能触发回滚。</div>
              <div className="modalKvGrid">
                <div className="modalKvLabel">模式</div>
                <div className="modalKvValue">
                  <Mono>apply</Mono>
                </div>
                <div className="modalKvLabel">范围</div>
                <div className="modalKvValue">
                  <Mono>{scopeLabel}</Mono>
                </div>
                <div className="modalKvLabel">目标</div>
                <div className="modalKvValue">
                  <Mono>{input.targetLabel}</Mono>
                </div>
                <div className="modalKvLabel">备份</div>
                <div className="modalKvValue">
                  <Mono>inherit</Mono>
                </div>
                <div className="modalKvLabel">架构不匹配</div>
                <div className="modalKvValue">
                  <Mono>disallow</Mono>
                </div>
              </div>
            </>
          ),
        confirmText: '执行更新',
        cancelText: '取消',
        confirmVariant,
        badgeText,
        badgeTone,
      })
      if (!ok) return

      const finalTarget = input.getTarget
        ? input.getTarget()
        : { targetTag: input.targetTag, targetDigest: input.targetDigest }

      setBusy(true)
      setError(null)
      setNoticeJobId(null)
      try {
        const resp = await triggerUpdate({
          scope: input.scope,
          stackId: input.stackId,
          serviceId: input.serviceId,
          targetTag: finalTarget.targetTag,
          targetDigest: finalTarget.targetDigest ?? undefined,
          mode: 'apply',
          allowArchMismatch: false,
          backupMode: 'inherit',
        })
        setNoticeJobId(resp.jobId)
      } catch (e: unknown) {
        if (e instanceof ApiError) {
          if (e.status === 401) setError('需要登录/鉴权（forward header）')
          else if (e.status === 409) setError('该 stack 正在更新（禁止并发）')
          else setError(e.message)
        } else {
          setError(e instanceof Error ? e.message : String(e))
        }
      } finally {
        setBusy(false)
      }
    },
    [confirm],
  )

  const groupsAll = useMemo(() => {
    const q = search.trim().toLowerCase()
    const out: Array<{
      stackId: string
      stackName: string
      lastCheckAt: string
      servicesAll: Array<{ svc: Service; status: RowStatus }>
      servicesSearch: Array<{ svc: Service; status: RowStatus }>
      countsAll: Record<Exclude<RowStatus, 'ok'>, number>
      countsSearch: Record<Exclude<RowStatus, 'ok'>, number>
      totalServices: number
    }> = []

    for (const st of stacks) {
      const d = details[st.id]
      if (!d) continue

      const servicesAll = d.services
        .filter((svc) => !svc.archived)
        .map((svc) => ({ svc, status: serviceRowStatus(svc) }))

      const servicesSearch = q
        ? servicesAll.filter((x) => {
            const hay = `${d.name} ${x.svc.name} ${x.svc.image.ref} ${x.svc.image.tag}`.toLowerCase()
            return hay.includes(q)
          })
        : servicesAll

      if (q && servicesSearch.length === 0) continue

      const countsAll: Record<Exclude<RowStatus, 'ok'>, number> = {
        updatable: 0,
        hint: 0,
        crossTag: 0,
        archMismatch: 0,
        blocked: 0,
      }
      for (const x of servicesAll) {
        if (x.status === 'ok') continue
        countsAll[x.status] += 1
      }

      const countsSearch: Record<Exclude<RowStatus, 'ok'>, number> = {
        updatable: 0,
        hint: 0,
        crossTag: 0,
        archMismatch: 0,
        blocked: 0,
      }
      for (const x of servicesSearch) {
        if (x.status === 'ok') continue
        countsSearch[x.status] += 1
      }

      const totalServices = servicesAll.length
      out.push({
        stackId: st.id,
        stackName: d.name,
        lastCheckAt: st.lastCheckAt,
        servicesAll,
        servicesSearch,
        countsAll,
        countsSearch,
        totalServices,
      })
    }

    return out
  }, [details, search, stacks])

  const filterSummary = useMemo(() => {
    let total = 0
    const counts: Record<Exclude<RowStatus, 'ok'>, number> = {
      updatable: 0,
      hint: 0,
      crossTag: 0,
      archMismatch: 0,
      blocked: 0,
    }
    for (const g of groupsAll) {
      total += g.servicesSearch.length
      for (const k of Object.keys(counts) as Array<Exclude<RowStatus, 'ok'>>) {
        counts[k] += g.countsSearch[k]
      }
    }
    return { total, counts }
  }, [groupsAll])

  const groups = useMemo(() => {
    const out: Array<{
      stackId: string
      stackName: string
      lastCheckAt: string
      services: Array<{ svc: Service; status: RowStatus }>
      countsAll: Record<Exclude<RowStatus, 'ok'>, number>
      totalServices: number
    }> = []

    for (const g of groupsAll) {
      const services = filter === 'all' ? g.servicesSearch : g.servicesSearch.filter((x) => x.status === filter)
      if (filter !== 'all' && services.length === 0) continue
      out.push({
        stackId: g.stackId,
        stackName: g.stackName,
        lastCheckAt: g.lastCheckAt,
        services,
        countsAll: g.countsAll,
        totalServices: g.totalServices,
      })
    }
    return out
  }, [filter, groupsAll])

  const totals = useMemo(() => {
    let total = 0
    for (const st of stacks) {
      const d = details[st.id]
      if (!d) continue
      total += d.services.filter((svc) => !svc.archived).length
    }
    const filtered = groups.reduce((acc, g) => acc + g.services.length, 0)
    return { total, filtered }
  }, [details, groups, stacks])

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
	              {totals.filtered}/{totals.total}
	            </div>
	          </div>
	        </div>

	        <div style={{ marginTop: 12 }}>
	          <UpdateCandidateFilters value={filter} onChange={setFilter} total={filterSummary.total} counts={filterSummary.counts} />
	        </div>

	        <div className="table" style={{ marginTop: 12 }}>
	          <div className="tableHeader">
	            <div>Service</div>
	            <div>Image</div>
	            <div>Current → Candidate</div>
            <div>状态 / 备注</div>
            <div>操作</div>
          </div>

	          {groups.map((g) => {
	            const isCollapsed = collapsed[g.stackId] ?? false
	            const groupSummary = formatGroupSummary(g.totalServices, g.countsAll)
	            const stackApply =
	              g.countsAll.updatable > 0
	                ? { enabled: true, title: null as string | null }
	                : g.countsAll.hint > 0 || g.countsAll.crossTag > 0
	                  ? { enabled: true, title: '存在需确认/跨标签的候选；将由服务端计算是否实际变更' }
	                  : { enabled: false, title: '无可更新服务' }
	            return (
	              <div key={g.stackId} className={isCollapsed ? 'tableGroup' : 'tableGroup tableGroupExpanded'}>
	                {!isCollapsed ? <GroupGuide rows={g.services.length} /> : null}
                <div
                  className="groupHead"
                  onClick={() => setCollapsed((prev) => ({ ...prev, [g.stackId]: !isCollapsed }))}
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault()
                      setCollapsed((prev) => ({ ...prev, [g.stackId]: !isCollapsed }))
                    }
                  }}
                >
                  <div className="cellService cellServiceGroup">
                    <StackIcon variant={isCollapsed ? 'collapsed' : 'expanded'} />
                    <div className="groupTitle">{g.stackName}</div>
                  </div>
                  <div className="groupMeta">{groupSummary}</div>
                  <div />
                  <div />
                  <div
                    className="actionCell"
                    onClick={(e) => e.stopPropagation()}
                    onKeyDown={(e) => e.stopPropagation()}
                  >
	                    <Button
	                      variant="ghost"
		                      disabled={busy || !stackApply.enabled}
		                      title={stackApply.title ?? undefined}
		                      onClick={() => {
		                        const d = details[g.stackId]
		                        const candidateServices = d
		                          ? d.services
		                              .filter((svc) => !svc.archived)
		                              .map((svc) => ({ svc, status: serviceRowStatus(svc) }))
		                              .filter((x) => x.status === 'updatable' || x.status === 'hint' || x.status === 'crossTag')
		                          : []
		                        const totalCandidates = g.countsAll.updatable + g.countsAll.hint + g.countsAll.crossTag
		                        const body = (
		                          <>
		                            <div className="modalLead">将为该 stack 内服务创建更新任务（服务端会计算是否实际变更）。</div>
		                            <div className="modalKvGrid">
	                              <div className="modalKvLabel">范围</div>
	                              <div className="modalKvValue">
	                                <Mono>stack</Mono>
	                              </div>
	                              <div className="modalKvLabel">目标</div>
	                              <div className="modalKvValue">
	                                <Mono>{g.stackName}</Mono>
	                              </div>
	                              <div className="modalKvLabel">候选服务</div>
	                              <div className="modalKvValue">{totalCandidates} 个（可更新/需确认/跨标签）</div>
	                              <div className="modalKvLabel">其中</div>
	                              <div className="modalKvValue">
	                                可更新 {g.countsAll.updatable} · 需确认 {g.countsAll.hint} · 跨标签 {g.countsAll.crossTag}
	                              </div>
	                              <div className="modalKvLabel">将跳过</div>
		                              <div className="modalKvValue">
		                                架构不匹配 {g.countsAll.archMismatch} · 被阻止 {g.countsAll.blocked}
		                              </div>
		                            </div>
		                            <div className="modalDivider" />
		                            <div className="modalLead">将更新的服务（预览）</div>
		                            <div className="modalList">
		                              {candidateServices.map((item) => {
		                                const current = formatTagDisplay(item.svc.image.tag, item.svc.image.resolvedTag)
		                                const candidate = item.svc.candidate ? formatTagDisplay(item.svc.candidate.tag, undefined) : '-'
		                                const title = `${formatTagTooltip(item.svc.image.tag, item.svc.image.digest, item.svc.image.resolvedTag, item.svc.image.resolvedTags) ?? current} → ${
		                                  item.svc.candidate
		                                    ? formatTagTooltip(item.svc.candidate.tag, item.svc.candidate.digest, undefined, undefined) ?? item.svc.candidate.tag
		                                    : '-'
		                                }`
		                                return (
		                                  <div key={item.svc.id} className="modalListItem">
		                                    <div className="modalListLeft">
		                                      <div className="modalListTitle">
		                                        <span className="mono">{item.svc.name}</span>
		                                        <span className="muted">{` · ${item.status}`}</span>
		                                      </div>
		                                      {(() => {
		                                        const img = splitImageRef(item.svc.image.ref)
		                                        const dn = splitImageNameForDisplay(img.name, item.svc.image.tag)
		                                        return (
		                                          <div className="cellTwoLine">
		                                            <div className="mono monoPrimary monoSplit">
		                                              <span className="monoSplitBase">{dn.base}</span>
		                                              {dn.suffix ? <span className="monoSplitTail">{dn.suffix}</span> : null}
		                                            </div>
		                                            <div className="mono monoSecondary">{img.registry}</div>
		                                          </div>
		                                        )
		                                      })()}
		                                    </div>
		                                    <div className="modalListRight">
		                                      <span className="mono" title={title}>{`${current} → ${candidate}`}</span>
		                                    </div>
		                                  </div>
		                                )
		                              })}
		                            </div>
		                            <div className="modalDivider" />
		                            <div className="muted">提示：将拉取镜像并重启容器；失败可能触发回滚。</div>
		                          </>
		                        )
	                        void triggerApply({
	                          scope: 'stack',
	                          stackId: g.stackId,
	                          targetLabel: `stack:${g.stackName}`,
	                          confirmBody: body,
	                          confirmTitle: '确认更新此 stack？',
	                        })
	                      }}
	                    >
	                      更新此 stack
	                    </Button>
                  </div>
                </div>

                {!isCollapsed
                  ? g.services.map(({ svc, status }) => {
	                      const current = formatTagDisplay(svc.image.tag, svc.image.resolvedTag)
	                      const currentTitle = formatTagTooltip(svc.image.tag, svc.image.digest, svc.image.resolvedTag, svc.image.resolvedTags)
	                      const candidate = svc.candidate ? formatTagDisplay(svc.candidate.tag, undefined) : '-'
	                      const candidateTitle = svc.candidate ? formatTagTooltip(svc.candidate.tag, svc.candidate.digest, undefined, undefined) : undefined
                      const isDockrev = isDockrevService(svc)
                      const svcApply =
                        status === 'updatable'
                          ? { enabled: true, title: null as string | null }
                          : status === 'crossTag'
                            ? { enabled: true, title: '跨标签版本更新；请确认风险后执行' }
                          : status === 'hint'
                            ? { enabled: true, title: '未确认是否有更新；将由服务端计算是否实际变更' }
                          : status === 'ok'
                            ? { enabled: false, title: '无候选版本' }
                              : status === 'archMismatch'
                                ? { enabled: false, title: '架构不匹配（仅提示，不允许更新）' }
                                : { enabled: false, title: svc.ignore?.reason ?? '被阻止' }
                      return (
                        <div
                          key={svc.id}
                          className="rowLine"
                          onClick={(e) => {
                            const t = e.target as unknown
                            const el =
                              t instanceof Element
                                ? t
                                : t && (t as { parentElement?: unknown }).parentElement instanceof Element
                                  ? (t as { parentElement: Element }).parentElement
                                  : null
                            if (el?.closest('button, a, input, select, textarea')) return
                            navigate({ name: 'service', stackId: g.stackId, serviceId: svc.id })
                          }}
                          role="button"
                          tabIndex={0}
                          onKeyDown={(e) => {
                            const t = e.target as unknown
                            const el =
                              t instanceof Element
                                ? t
                                : t && (t as { parentElement?: unknown }).parentElement instanceof Element
                                  ? (t as { parentElement: Element }).parentElement
                                  : null
                            if (el?.closest('button, a, input, select, textarea')) return
                            if (e.key === 'Enter' || e.key === ' ') {
                              e.preventDefault()
                              navigate({ name: 'service', stackId: g.stackId, serviceId: svc.id })
                            }
                          }}
                        >
	                          <div className="cellService">
	                            <span className="svcBullet" aria-hidden="true" />
	                            <span className="svcName">{svc.name}</span>
	                          </div>
	                          {(() => {
	                            const img = splitImageRef(svc.image.ref)
	                            const dn = splitImageNameForDisplay(img.name, svc.image.tag)
	                            return (
	                              <div className="cellTwoLine">
	                                <div className="mono monoPrimary monoSplit">
	                                  <span className="monoSplitBase">{dn.base}</span>
	                                  {dn.suffix ? <span className="monoSplitTail">{dn.suffix}</span> : null}
	                                </div>
	                                <div className="mono monoSecondary">{img.registry}</div>
	                              </div>
	                            )
	                          })()}
	                          <div className="cellTwoLine">
	                            <div className="mono monoPrimary" title={currentTitle}>{current}</div>
	                            <div className="mono monoPrimary" title={candidateTitle}>{candidate}</div>
	                          </div>
                          <StatusRemark service={svc} status={status} />
	                          <div
	                            className="actionCell"
	                            onClick={(e) => e.stopPropagation()}
	                            onKeyDown={(e) => e.stopPropagation()}
	                          >
                            {isDockrev ? (
                              <div className="actionStack">
                                <Button
                                  variant="ghost"
                                  disabled={busy || supervisor.state.status !== 'ok'}
                                  title={
                                    supervisor.state.status === 'offline'
                                      ? `自我升级不可用（supervisor offline） · ${supervisor.state.errorAt}`
                                      : supervisor.state.status === 'checking'
                                        ? '检查 supervisor 中…'
                                        : undefined
                                  }
                                  onClick={() => {
                                    window.location.href = selfUpgradeUrl
                                  }}
                                >
                                  升级 Dockrev
                                </Button>
                                {supervisor.state.status !== 'ok' ? (
                                  <Button
                                    variant="ghost"
                                    disabled={busy || supervisor.state.status === 'checking'}
                                    onClick={() => {
                                      void supervisor.check()
                                    }}
                                  >
                                    重试
                                  </Button>
                                ) : null}
                                {supervisor.state.status === 'offline' ? (
                                  <div className="muted">supervisor offline · {supervisor.state.errorAt}</div>
                                ) : null}
                              </div>
	                            ) : (
	                              <Button
	                                variant="ghost"
	                                disabled={busy || !svcApply.enabled}
	                                title={svcApply.title ?? undefined}
	                                onClick={() => {
	                                  const selected = { tag: svc.candidate?.tag ?? '-', digest: svc.candidate?.digest ?? null }
	                                  const body = (
	                                    <>
	                                      <div className="modalLead">将对该服务执行更新（apply）。</div>
	                                      <div className="modalKvGrid">
	                                        <div className="modalKvLabel">范围</div>
	                                        <div className="modalKvValue">
	                                          <Mono>service</Mono>
	                                        </div>
	                                        <div className="modalKvLabel">目标</div>
	                                        <div className="modalKvValue">
	                                          <Mono>{`${g.stackName}/${svc.name}`}</Mono>
		                                        </div>
		                                        <div className="modalKvLabel">镜像</div>
		                                        <div className="modalKvValue">
		                                          {(() => {
		                                        const img = splitImageRef(svc.image.ref)
		                                        const dn = splitImageNameForDisplay(img.name, svc.image.tag)
		                                        return (
		                                          <div className="cellTwoLine">
		                                            <div className="mono monoPrimary monoSplit">
		                                              <span className="monoSplitBase">{dn.base}</span>
		                                              {dn.suffix ? <span className="monoSplitTail">{dn.suffix}</span> : null}
		                                            </div>
		                                            <div className="mono monoSecondary">{img.registry}</div>
		                                          </div>
		                                        )
		                                      })()}
		                                        </div>
		                                        <div className="modalKvLabel">目标版本</div>
		                                        <div className="modalKvValue">
                                            <span className="mono">{formatTagDisplay(svc.image.tag, svc.image.resolvedTag)}</span>
                                            <span className="mono" style={{ opacity: 0.8 }}>
                                              {' '}
                                              →{' '}
                                            </span>
                                              <UpdateTargetSelect
                                                serviceId={svc.id}
                                                currentTag={svc.image.resolvedTag ?? svc.image.tag}
                                                initialTag={svc.candidate?.tag ?? null}
                                                initialDigest={svc.candidate?.digest ?? null}
                                                variant="inline"
                                                showLabel={false}
                                                showComparison={false}
                                                onChange={(next) => {
                                                  selected.tag = next.tag
                                                  selected.digest = next.digest ?? null
                                                }}
                                              />
	                                        </div>
		                                        <div className="modalKvLabel">状态</div>
		                                        <div className="modalKvValue">
		                                          <Mono>{status}</Mono>
		                                        </div>
		                                      </div>
		                                      <div className="modalDivider" />
		                                      <div className="muted">提示：将拉取镜像并重启容器；失败可能触发回滚。</div>
		                                    </>
	                                  )
	                                  void triggerApply({
	                                    scope: 'service',
	                                    stackId: g.stackId,
	                                    serviceId: svc.id,
	                                    targetLabel: `service:${g.stackName}/${svc.name}`,
	                                    getTarget: () => ({
	                                      targetTag: selected.tag !== '-' ? selected.tag : undefined,
	                                      targetDigest: selected.digest,
	                                    }),
	                                    confirmBody: body,
	                                    confirmTitle: `确认更新服务 ${svc.name}？`,
	                                  })
	                                }}
	                              >
	                                执行更新
	                              </Button>
	                            )}
	                          </div>
                        </div>
                      )
                    })
                  : null}
              </div>
            )
          })}

          {groups.length === 0 ? <div className="muted">无匹配结果</div> : null}
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
	                    {(() => {
	                      const img = splitImageRef(x.svc.image.ref)
	                      return (
	                        <div className="muted">
	                          image <Mono>{formatImageName(img.name, x.svc.image.tag)}</Mono> · registry <Mono>{img.registry}</Mono> · current{' '}
	                          <Mono>{formatTagDisplay(x.svc.image.tag, x.svc.image.resolvedTag)}</Mono>
	                        </div>
	                      )
	                    })()}
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
      {noticeJobId ? (
        <div className="success">
          已创建更新任务 <Mono>{noticeJobId}</Mono> ·{' '}
          <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: 'queue' })}>
            查看队列
          </Button>
        </div>
      ) : null}
      {busy ? <div className="muted">处理中…</div> : null}
    </div>
  )
}
