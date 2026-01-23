import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  getStack,
  listStacks,
  triggerCheck,
  type Service,
  type StackDetail,
  type StackListItem,
} from '../api'
import { navigate } from '../routes'
import { Button } from '../ui'
import { FilterChips } from '../Shell'

type RowStatus = 'ok' | 'updatable' | 'hint' | 'archMismatch' | 'blocked'
type Filter = 'all' | Exclude<RowStatus, 'ok'>

function serviceStatus(svc: Service): RowStatus {
  if (svc.ignore?.matched) return 'blocked'
  if (!svc.candidate) return 'ok'
  if (svc.candidate.archMatch === 'mismatch') return 'archMismatch'
  if (svc.candidate.archMatch === 'unknown') return 'hint'
  return 'updatable'
}

function statusDotClass(st: RowStatus): string {
  if (st === 'updatable') return 'statusDot statusDotOk'
  if (st === 'hint') return 'statusDot statusDotWarn'
  if (st === 'archMismatch') return 'statusDot statusDotWarn'
  if (st === 'blocked') return 'statusDot statusDotBad'
  return 'statusDot'
}

function statusLabel(st: RowStatus): string {
  if (st === 'updatable') return '可更新'
  if (st === 'hint') return '新版本提示'
  if (st === 'archMismatch') return '架构不匹配'
  if (st === 'blocked') return '被阻止'
  return '无更新'
}

function noteFor(svc: Service, st: RowStatus): string {
  if (st === 'blocked') return svc.ignore?.reason ?? '被阻止'
  if (st === 'archMismatch') return '仅提示，不允许更新'
  if (st === 'hint') return '同级别/新 minor'
  if (st === 'updatable') {
    const hasForceBackup =
      Object.values(svc.settings.backupTargets.bindPaths).some((v) => v === 'force') ||
      Object.values(svc.settings.backupTargets.volumeNames).some((v) => v === 'force')
    return hasForceBackup ? '备份通过后执行' : '按当前 tag 前缀'
  }
  return '-'
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
  if (counts.hint > 0) parts.push(`${counts.hint} 新版本提示`)
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
      </>,
    )
  }, [busy, onTopActions, refresh])

  const countsAll = useMemo(() => {
    const c: Record<Exclude<RowStatus, 'ok'>, number> = { updatable: 0, hint: 0, archMismatch: 0, blocked: 0 }
    for (const st of stacks) {
      const d = details[st.id]
      if (!d) continue
      for (const svc of d.services) {
        if (svc.archived) continue
        const stt = serviceStatus(svc)
        if (stt === 'ok') continue
        c[stt] += 1
      }
    }
    return c
  }, [details, stacks])

  return (
    <div className="page">
      <div className="statGrid">
        <div className="card statCard">
          <div className="label">可更新</div>
          <div className="statNum">{countsAll.updatable}</div>
          <div className="muted">需要确认后执行</div>
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
          <div className="muted">备份失败/被禁用</div>
        </div>
      </div>

      <div className="overviewIndent">
        <div className="title">更新候选</div>

        <div style={{ marginTop: 14 }}>
          <FilterChips
            value={filter}
            onChange={setFilter}
            items={[
              { key: 'all', label: '全部' },
              { key: 'updatable', label: '可更新', count: countsAll.updatable },
              { key: 'hint', label: '新版本提示', count: countsAll.hint },
              { key: 'archMismatch', label: '架构不匹配', count: countsAll.archMismatch },
              { key: 'blocked', label: '被阻止', count: countsAll.blocked },
            ]}
          />
        </div>

        <div className="table" style={{ marginTop: 14 }}>
          <div className="tableHeader">
            <div>Service</div>
            <div>Image</div>
            <div>Current → Candidate</div>
            <div>状态 / 备注</div>
          </div>

          {stacks.map((st) => {
            const d = details[st.id]
            if (!d) return null

            const rows = d.services
              .filter((svc) => !svc.archived)
              .map((svc) => ({ svc, stt: serviceStatus(svc) }))
              .filter((x) => filter === 'all' || x.stt === filter)

            const counts: Record<Exclude<RowStatus, 'ok'>, number> = { updatable: 0, hint: 0, archMismatch: 0, blocked: 0 }
            for (const svc of d.services) {
              if (svc.archived) continue
              const stt = serviceStatus(svc)
              if (stt === 'ok') continue
              counts[stt] += 1
            }

            const isCollapsed = collapsed[st.id] ?? false
            const totalServices = d.services.filter((svc) => !svc.archived).length
            const groupSummary = formatGroupSummary(totalServices, counts)

            return (
              <div key={st.id} className={isCollapsed ? 'tableGroup' : 'tableGroup tableGroupExpanded'}>
                {!isCollapsed ? <GroupGuide rows={rows.length} /> : null}
                <button
                  className="groupHead"
                  onClick={() => setCollapsed((prev) => ({ ...prev, [st.id]: !isCollapsed }))}
                >
                  <div className="cellService cellServiceGroup">
                    <StackIcon variant={isCollapsed ? 'collapsed' : 'expanded'} />
                    <div className="groupTitle">{d.name}</div>
                  </div>
                  <div className="groupMeta">{groupSummary}</div>
                  <div />
                  <div />
                </button>

                {!isCollapsed
                  ? rows.map(({ svc, stt }) => {
                      const current = formatTagDigest(svc.image.tag, svc.image.digest)
                      const candidate = svc.candidate ? formatTagDigest(svc.candidate.tag, svc.candidate.digest) : '-'
                      return (
                        <button
                          key={svc.id}
                          className="rowLine"
                          onClick={() => navigate({ name: 'service', stackId: st.id, serviceId: svc.id })}
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
                          <div className="statusCol">
                            <div className="statusLine">
                              <span className={statusDotClass(stt)} aria-hidden="true" />
                              <span className="label">{statusLabel(stt)}</span>
                            </div>
                            <div className="muted statusNote">{noteFor(svc, stt)}</div>
                          </div>
                        </button>
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
      {busy ? <div className="muted">处理中…</div> : null}
    </div>
  )
}
