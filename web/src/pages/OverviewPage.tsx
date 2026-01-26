import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  triggerDiscoveryScan,
  listDiscoveryProjects,
  listJobs,
  getStack,
  listStacks,
  triggerCheck,
  triggerUpdate,
  ApiError,
  type DiscoveredProject,
  type DiscoveryScanResponse,
  type JobListItem,
  type Service,
  type StackDetail,
  type StackListItem,
} from '../api'
import { navigate } from '../routes'
import { Button, Mono, StatusRemark } from '../ui'
import { isDockrevImageRef, selfUpgradeBaseUrl } from '../runtimeConfig'
import { useSupervisorHealth } from '../useSupervisorHealth'
import { serviceRowStatus, type RowStatus } from '../updateStatus'
import { UpdateCandidateFilters, type UpdateCandidateFilter } from '../components/UpdateCandidateFilters'
import { useConfirm } from '../confirm'

function formatShort(ts?: string | null) {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function formatDigestShort(digest: string | null | undefined): string | null {
  if (!digest) return null
  const m = digest.includes(':') ? digest : `sha256:${digest}`
  const last2 = m.slice(-2)
  return `${m.split(':')[0]}:…${last2}`
}

function formatTagDigest(tag: string, digest: string | null | undefined): string {
  const d = formatDigestShort(digest)
  return d ? `${tag}@${d}` : tag
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

function formatGroupSummary(services: number, counts: Record<Exclude<RowStatus, 'ok'>, number>) {
  const parts: string[] = [`${services} services`]
  if (counts.updatable > 0) parts.push(`${counts.updatable} 可更新`)
  if (counts.crossTag > 0) parts.push(`${counts.crossTag} 跨 tag 版本`)
  if (counts.hint > 0) parts.push(`${counts.hint} 需确认`)
  if (counts.archMismatch > 0) parts.push(`${counts.archMismatch} 架构不匹配`)
  if (counts.blocked > 0) parts.push(`${counts.blocked} 被阻止`)
  return parts.join(' · ')
}

function GroupGuide(props: { rows: number }) {
  if (props.rows <= 0) return null
  const rowHeight = 52
  const gap = 4
  const bulletGap = 10
  const topSeg = rowHeight / 2 - bulletGap / 2
  const midSeg = rowHeight + gap - bulletGap

  const segments: Array<{ top: number; height: number }> = []
  let y = 36 + gap // group head + gap == first row top
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

export function OverviewPage(props: {
  onComposeHint: (hint: { path?: string; profile?: string; lastScan?: string }) => void
  onTopActions: (node: React.ReactNode) => void
}) {
  const { onComposeHint, onTopActions } = props
  const confirm = useConfirm()
  const [filter, setFilter] = useState<UpdateCandidateFilter>('all')
  const [stacks, setStacks] = useState<StackListItem[]>([])
  const [details, setDetails] = useState<Record<string, StackDetail | undefined>>({})
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({})
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [discoveredProjects, setDiscoveredProjects] = useState<DiscoveredProject[]>([])
  const [lastDiscoveryScan, setLastDiscoveryScan] = useState<DiscoveryScanResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [noticeJobId, setNoticeJobId] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const supervisor = useSupervisorHealth()
  const selfUpgradeUrl = useMemo(() => selfUpgradeBaseUrl(), [])

  const refresh = useCallback(async () => {
    const errors: string[] = []
    setError(null)

    const stacksPromise = listStacks()
    const jobsPromise = listJobs()
    const projectsPromise = listDiscoveryProjects('exclude')

    const [stacksRes, jobsRes, projectsRes] = await Promise.allSettled([stacksPromise, jobsPromise, projectsPromise])

    if (jobsRes.status === 'fulfilled') setJobs(jobsRes.value)
    else errors.push('jobs unavailable')

    if (projectsRes.status === 'fulfilled') setDiscoveredProjects(projectsRes.value)
    else errors.push('discovery projects unavailable')

    if (stacksRes.status === 'rejected') {
      throw stacksRes.reason
    }

    const s = stacksRes.value
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

    if (errors.length > 0) setError(errors.join(' · '))
  }, [onComposeHint])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
  }, [refresh])

  const countsAll = useMemo(() => {
    const c: Record<Exclude<RowStatus, 'ok'>, number> = {
      updatable: 0,
      hint: 0,
      crossTag: 0,
      archMismatch: 0,
      blocked: 0,
    }
    for (const st of stacks) {
      const d = details[st.id]
      if (!d) continue
      for (const svc of d.services) {
        if (svc.archived) continue
        const stt = serviceRowStatus(svc)
        if (stt === 'ok') continue
        c[stt] += 1
      }
    }
    return c
  }, [details, stacks])

  const totalServicesAll = useMemo(() => {
    let total = 0
    for (const st of stacks) {
      const d = details[st.id]
      if (!d) continue
      total += d.services.filter((svc) => !svc.archived).length
    }
    return total
  }, [details, stacks])

  const allApply = useMemo(() => {
    if (countsAll.updatable > 0) return { enabled: true, note: null as string | null, title: null as string | null }
    if (countsAll.hint > 0 || countsAll.crossTag > 0) {
      return {
        enabled: true,
        note: '存在需确认/跨 tag 的候选；将由服务端计算是否实际变更',
        title: '存在需确认/跨 tag 的候选；将由服务端计算是否实际变更',
      }
    }
    return { enabled: false, note: null as string | null, title: '无可更新服务' }
  }, [countsAll.crossTag, countsAll.hint, countsAll.updatable])

  const jobsSummary = useMemo(() => {
    const total = jobs.length
    const running = jobs.filter((j) => j.status === 'running').length
    const failed = jobs.filter((j) => j.status === 'failed').length
    const rolled = jobs.filter((j) => j.status === 'rolled_back').length
    const success = jobs.filter((j) => j.status === 'success').length
    const other = total - running - failed - rolled - success
    const latest = [...jobs]
      .sort((a, b) => String(b.createdAt).localeCompare(String(a.createdAt)))
      .at(0)
    return { total, running, failed, rolled, success, other, latest }
  }, [jobs])

  const discoverySummary = useMemo(() => {
    const active = discoveredProjects.filter((p) => p.status === 'active' && !p.archived)
    const missing = discoveredProjects.filter((p) => p.status === 'missing' && !p.archived)
    const invalid = discoveredProjects.filter((p) => p.status === 'invalid' && !p.archived)
    const issues = [...missing, ...invalid]
      .sort((a, b) => String(b.lastSeenAt ?? '').localeCompare(String(a.lastSeenAt ?? '')))
      .slice(0, 4)
    return { active, missing, invalid, issues }
  }, [discoveredProjects])

  const runDiscoveryScan = useCallback(async () => {
    const ok = await confirm({
      title: '确认执行发现扫描？',
      body: ['即将执行发现扫描（discovery scan）', '', '提示：可能创建/标记 stacks；用于发现 missing/invalid。'].join('\n'),
      confirmText: '开始扫描',
      cancelText: '取消',
      confirmVariant: 'primary',
    })
    if (!ok) return
    setBusy(true)
    setError(null)
    try {
      const resp = await triggerDiscoveryScan()
      setLastDiscoveryScan(resp)
      setDiscoveredProjects(await listDiscoveryProjects('exclude'))
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }, [confirm])

  const triggerApply = useCallback(
    async (input: { scope: 'all' | 'stack' | 'service'; stackId?: string; serviceId?: string; targetLabel: string }) => {
      const scopeLabel = input.scope === 'all' ? 'all' : input.scope === 'stack' ? 'stack' : 'service'
      const ok = await confirm({
        title: '确认执行更新？',
        body: [
          `即将执行更新（mode=apply）`,
          `scope=${scopeLabel}`,
          `target=${input.targetLabel}`,
          '',
          '提示：将拉取镜像并重启容器；失败可能触发回滚。',
        ].join('\n'),
        confirmText: '执行更新',
        cancelText: '取消',
        confirmVariant: 'danger',
      })
      if (!ok) return

      setBusy(true)
      setError(null)
      setNoticeJobId(null)
      try {
        const resp = await triggerUpdate({
          scope: input.scope,
          stackId: input.stackId,
          serviceId: input.serviceId,
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
        </Button>
        <Button
          variant="danger"
          disabled={busy || !allApply.enabled}
          title={allApply.title ?? undefined}
          onClick={() => {
            void triggerApply({ scope: 'all', targetLabel: '全部服务' })
          }}
        >
          更新全部
        </Button>
      </>,
    )
  }, [allApply.enabled, allApply.title, busy, onTopActions, refresh, triggerApply])

  return (
    <div className="page">
      <div className="twoCol">
        <div className="card">
          <div className="sectionRow">
            <div>
              <div className="title">运行态与结果</div>
              <div className="muted">更新任务（队列）摘要</div>
            </div>
            <div style={{ marginLeft: 'auto', display: 'flex', gap: 10 }}>
              <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: 'queue' })}>
                查看队列
              </Button>
            </div>
          </div>
          <div className="chipRow" style={{ marginTop: 14 }}>
            <div className="chipStatic">{`运行中: ${jobsSummary.running}`}</div>
            <div className="chipStatic">{`失败: ${jobsSummary.failed}`}</div>
            <div className="chipStatic">{`回滚: ${jobsSummary.rolled}`}</div>
            <div className="chipStatic">{`成功: ${jobsSummary.success}`}</div>
            {jobsSummary.other > 0 ? <div className="chipStatic">{`其他: ${jobsSummary.other}`}</div> : null}
          </div>
          <div className="muted" style={{ marginTop: 12 }}>
            最近: {jobsSummary.latest ? <Mono>{`${jobsSummary.latest.status} · ${formatShort(jobsSummary.latest.createdAt)} · ${jobsSummary.latest.scope}`}</Mono> : <Mono>-</Mono>}
          </div>
        </div>

        <div className="card">
          <div className="sectionRow">
            <div>
              <div className="title">扫描与发现异常</div>
              <div className="muted">discovery projects（missing/invalid）</div>
            </div>
            <div style={{ marginLeft: 'auto', display: 'flex', gap: 10 }}>
              <Button variant="ghost" disabled={busy} onClick={runDiscoveryScan}>
                发现扫描
              </Button>
              <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: 'services' })}>
                查看服务
              </Button>
            </div>
          </div>
          <div className="chipRow" style={{ marginTop: 14 }}>
            <div className="chipStatic">{`active: ${discoverySummary.active.length}`}</div>
            <div className="chipStatic">{`missing: ${discoverySummary.missing.length}`}</div>
            <div className="chipStatic">{`invalid: ${discoverySummary.invalid.length}`}</div>
            {lastDiscoveryScan ? <div className="chipStatic">{`last scan: ${formatShort(lastDiscoveryScan.startedAt)}`}</div> : null}
          </div>
          {discoverySummary.issues.length > 0 ? (
            <div style={{ marginTop: 12, display: 'flex', flexDirection: 'column', gap: 8 }}>
              {discoverySummary.issues.map((p) => (
                <div key={p.project} className="muted" title={p.lastError ?? undefined}>
                  <Mono>{p.project}</Mono>
                  {p.status === 'missing' ? ' · missing' : ' · invalid'}
                  {p.lastError ? ` · ${p.lastError}` : ''}
                </div>
              ))}
            </div>
          ) : (
            <div className="muted" style={{ marginTop: 12 }}>
              暂无异常
            </div>
          )}
        </div>
      </div>

      <div className="overviewIndent">
        <div className="title">更新候选</div>

        <div style={{ marginTop: 14 }}>
          <UpdateCandidateFilters value={filter} onChange={setFilter} total={totalServicesAll} counts={countsAll} />
        </div>

        <div className="table" style={{ marginTop: 14 }}>
          <div className="tableHeader">
            <div>Service</div>
            <div>Image</div>
            <div>Current → Candidate</div>
            <div>状态 / 备注</div>
            <div>操作</div>
          </div>

	          {stacks.map((st) => {
	            const d = details[st.id]
	            if (!d) return null
	
	            const rows = d.services
	              .filter((svc) => !svc.archived)
	              .map((svc) => ({ svc, stt: serviceRowStatus(svc) }))
	              .filter((x) => filter === 'all' || x.stt === filter)

	            if (rows.length === 0) return null

	            const counts: Record<Exclude<RowStatus, 'ok'>, number> = {
	              updatable: 0,
	              hint: 0,
	              crossTag: 0,
	              archMismatch: 0,
              blocked: 0,
            }
            for (const svc of d.services) {
              if (svc.archived) continue
              const stt = serviceRowStatus(svc)
              if (stt === 'ok') continue
              counts[stt] += 1
            }

            const isCollapsed = collapsed[st.id] ?? false
            const totalServices = d.services.filter((svc) => !svc.archived).length
            const groupSummary = formatGroupSummary(totalServices, counts)
            const stackApply =
              counts.updatable > 0
                ? { enabled: true, title: null as string | null }
                : counts.hint > 0 || counts.crossTag > 0
                  ? { enabled: true, title: '存在需确认/跨 tag 的候选；将由服务端计算是否实际变更' }
                  : { enabled: false, title: '无可更新服务' }

            return (
              <div key={st.id} className={isCollapsed ? 'tableGroup' : 'tableGroup tableGroupExpanded'}>
                {!isCollapsed ? <GroupGuide rows={rows.length} /> : null}
                <div
                  className="groupHead"
                  role="button"
                  tabIndex={0}
                  onClick={() => setCollapsed((prev) => ({ ...prev, [st.id]: !isCollapsed }))}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault()
                      setCollapsed((prev) => ({ ...prev, [st.id]: !isCollapsed }))
                    }
                  }}
                >
                  <div className="cellService cellServiceGroup">
                    <StackIcon variant={isCollapsed ? 'collapsed' : 'expanded'} />
                    <div className="groupTitle">{d.name}</div>
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
                        void triggerApply({ scope: 'stack', stackId: st.id, targetLabel: `stack:${d.name}` })
                      }}
                    >
                      更新此 stack
                    </Button>
                  </div>
                </div>

                {!isCollapsed
                  ? rows.map(({ svc, stt }) => {
                      const current = formatTagDigest(svc.image.tag, svc.image.digest)
                      const candidate = svc.candidate ? formatTagDigest(svc.candidate.tag, svc.candidate.digest) : '-'
                      const isDockrev = isDockrevService(svc)
                      const svcApply =
                        stt === 'updatable'
                          ? { enabled: true, title: null as string | null, note: null as string | null }
                          : stt === 'crossTag'
                            ? { enabled: true, title: '跨 tag 版本更新；请确认风险后执行', note: '跨 tag' }
                          : stt === 'hint'
                            ? { enabled: true, title: '未确认是否有更新；将由服务端计算是否实际变更', note: '未确认' }
                            : stt === 'ok'
                              ? { enabled: false, title: '无候选版本', note: null }
                              : stt === 'archMismatch'
                                ? { enabled: false, title: '架构不匹配（仅提示，不允许更新）', note: null }
                                : { enabled: false, title: svc.ignore?.reason ?? '被阻止', note: null }
                      return (
                        <div
                          key={svc.id}
                          className="rowLine"
                          onClick={() => navigate({ name: 'service', stackId: st.id, serviceId: svc.id })}
                          role="button"
                          tabIndex={0}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter' || e.key === ' ') {
                              e.preventDefault()
                              navigate({ name: 'service', stackId: st.id, serviceId: svc.id })
                            }
                          }}
                        >
                          <div className="cellService">
                            <span className="svcBullet" aria-hidden="true" />
                            <span className="svcName">{svc.name}</span>
                          </div>
                          <div className="mono cellMono">{svc.image.ref}</div>
                          <div className="cellTwoLine">
                            <div className="mono">{current}</div>
                            <div className="mono">{candidate}</div>
                          </div>
                          <StatusRemark service={svc} status={stt} />
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
                                  void triggerApply({
                                    scope: 'service',
                                    stackId: st.id,
                                    serviceId: svc.id,
                                    targetLabel: `service:${d.name}/${svc.name}`,
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
        </div>

        <div className="muted" style={{ marginTop: 24 }}>
          按 compose 分组显示（可折叠）；点击 service 行进入详情。
        </div>
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
