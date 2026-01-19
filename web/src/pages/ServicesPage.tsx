import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  getStack,
  listIgnores,
  listStacks,
  registerStack,
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

  const [regName, setRegName] = useState('')
  const [regComposeFiles, setRegComposeFiles] = useState('')
  const [regEnvFile, setRegEnvFile] = useState('')

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
      for (const svc of d.services) out.push({ stackId: st.id, stackName: d.name, lastCheckAt: st.lastCheckAt, svc })
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

  async function doRegister() {
    setBusy(true)
    setError(null)
    try {
      const files = regComposeFiles
        .split('\n')
        .map((s) => s.trim())
        .filter(Boolean)
      if (!regName.trim() || files.length === 0) {
        throw new Error('请填写 name 与至少 1 个 composeFiles（容器内绝对路径）')
      }
      await registerStack({ name: regName.trim(), composeFiles: files, envFile: regEnvFile.trim() || null })
      setRegName('')
      setRegComposeFiles('')
      setRegEnvFile('')
      await refresh()
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }

  const ignoreByService = useMemo(() => {
    const m = new Map<string, number>()
    for (const r of ignores) {
      m.set(r.scope.serviceId, (m.get(r.scope.serviceId) ?? 0) + 1)
    }
    return m
  }, [ignores])

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
          <div className="title">注册 Stack</div>
          <div className="muted" style={{ marginLeft: 'auto' }}>
            运行在容器内时，这里填写“容器视角的绝对路径”
          </div>
        </div>
        <div className="formGrid">
          <label className="formField">
            <span className="label">Name</span>
            <input className="input" value={regName} onChange={(e) => setRegName(e.target.value)} placeholder="prod" />
          </label>
          <label className="formField">
            <span className="label">Env file（可选）</span>
            <input
              className="input"
              value={regEnvFile}
              onChange={(e) => setRegEnvFile(e.target.value)}
              placeholder="/srv/app/.env"
            />
          </label>
          <label className="formField formSpan2">
            <span className="label">Compose files（每行 1 个）</span>
            <textarea
              className="input textarea"
              value={regComposeFiles}
              onChange={(e) => setRegComposeFiles(e.target.value)}
              placeholder="/srv/app/compose.yml"
            />
          </label>
          <div className="formActions formSpan2">
            <Button variant="primary" disabled={busy} onClick={() => void doRegister()}>
              注册
            </Button>
          </div>
        </div>
      </div>

      {error ? <div className="error">{error}</div> : null}
    </div>
  )
}
