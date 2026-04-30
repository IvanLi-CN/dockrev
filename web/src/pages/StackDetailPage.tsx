import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { getStack, getStackSettings, putStackSettings, type StackDetail, type StackSettings } from '../api'
import { AutoUpdatePolicyEditor, createDefaultAutoUpdatePolicy } from '../components/AutoUpdatePolicyEditor'
import { navigate } from '../routes'
import { Button, Mono, Pill } from '../ui'
import { serviceRowStatus } from '../updateStatus'

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function statusLabel(status: ReturnType<typeof serviceRowStatus>): string {
  if (status === 'updatable') return '可更新'
  if (status === 'archMismatch') return '架构不匹配'
  if (status === 'blocked') return '被阻止'
  if (status === 'hint') return '需确认'
  return '无候选'
}

function statusTone(status: ReturnType<typeof serviceRowStatus>): 'ok' | 'warn' | 'bad' | 'muted' | 'info' {
  if (status === 'updatable') return 'ok'
  if (status === 'archMismatch' || status === 'hint') return 'warn'
  if (status === 'blocked') return 'bad'
  return 'muted'
}

export function StackDetailPage(props: {
  stackId: string
  onLastScanHint: (lastScan?: string) => void
  onTopActions: (node: ReactNode) => void
}) {
  const { stackId, onLastScanHint, onTopActions } = props
  const [stack, setStack] = useState<StackDetail | null>(null)
  const [settings, setSettings] = useState<StackSettings | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setError(null)
    onLastScanHint(undefined)
    const [stackRes, settingsRes] = await Promise.all([getStack(stackId), getStackSettings(stackId)])
    setStack(stackRes)
    setSettings(settingsRes)
  }, [onLastScanHint, stackId])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(errorMessage(e)))
  }, [refresh])

  useEffect(() => {
    onTopActions(
      <>
        <Button disabled={busy} onClick={() => navigate({ name: 'services' })}>
          返回服务
        </Button>
        <Button disabled={busy} onClick={() => void refresh()}>
          刷新
        </Button>
      </>,
    )
    return () => onTopActions(null)
  }, [busy, onTopActions, refresh])

  if (!stack || !settings) return <div className="muted">加载中…</div>

  const policy = settings.autoUpdatePolicy ?? createDefaultAutoUpdatePolicy('override')
  const updatable = stack.services.filter((service) => serviceRowStatus(service) !== 'ok').length

  return (
    <div className="page">
      <div className="stackDetailHero">
        <div>
          <div className="svcTitleName">Stack: <Mono>{stack.name}</Mono></div>
          <div className="muted">id <Mono>{stack.id}</Mono> · 服务 {stack.services.length} · 候选 {updatable}</div>
        </div>
        <Pill tone={stack.archived ? 'warn' : 'info'}>{stack.archived ? 'archived' : stack.compose.type}</Pill>
      </div>

      <div className="card">
        <AutoUpdatePolicyEditor
          busy={busy}
          onChange={(autoUpdatePolicy) => setSettings({ ...settings, autoUpdatePolicy })}
          onSave={() => {
            void (async () => {
              setBusy(true)
              setError(null)
              try {
                await putStackSettings(stack.id, { autoUpdatePolicy: policy })
                await refresh()
              } catch (e: unknown) {
                setError(errorMessage(e))
              } finally {
                setBusy(false)
              }
            })()
          }}
          policy={policy}
          scope="stack"
        />
      </div>

      <div className="card" style={{ marginTop: 16 }}>
        <div className="title">服务</div>
        <div className="stackServiceList">
          {stack.services.map((service) => {
            const status = serviceRowStatus(service)
            return (
              <div className="stackServiceRow" key={service.id}>
                <div>
                  <div className="mono">{service.name}</div>
                  <div className="muted">{service.image.ref}</div>
                </div>
                <Pill tone={statusTone(status)}>{statusLabel(status)}</Pill>
                <Button onClick={() => navigate({ name: 'service', stackId: stack.id, serviceId: service.id })}>
                  详情
                </Button>
              </div>
            )
          })}
        </div>
      </div>
      {error ? <div className="error">{error}</div> : null}
    </div>
  )
}
