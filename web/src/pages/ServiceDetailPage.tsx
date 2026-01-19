import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  createIgnore,
  deleteIgnore,
  getServiceSettings,
  getStack,
  listIgnores,
  putServiceSettings,
  triggerUpdate,
  type IgnoreRule,
  type Service,
  type ServiceSettings,
  type StackDetail,
} from '../api'
import { Button, Mono, Pill, Switch } from '../ui'

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function svcTone(svc: Service): 'ok' | 'warn' | 'bad' | 'muted' {
  if (svc.ignore?.matched) return 'bad'
  if (!svc.candidate) return 'muted'
  if (svc.candidate.archMatch === 'mismatch') return 'warn'
  if (svc.candidate.archMatch === 'unknown') return 'warn'
  return 'ok'
}

function svcBadge(svc: Service): string {
  if (svc.ignore?.matched) return 'blocked'
  if (!svc.candidate) return 'no candidate'
  if (svc.candidate.archMatch === 'mismatch') return 'arch mismatch'
  if (svc.candidate.archMatch === 'unknown') return 'hint'
  return 'updatable'
}

function formatMap(map: Record<string, string>) {
  const keys = Object.keys(map)
  if (keys.length === 0) return []
  return keys.map((k) => ({ key: k, value: map[k] }))
}

function shortDigest(digest: string) {
  if (digest.length <= 24) return digest
  return `${digest.slice(0, 12)}…${digest.slice(-8)}`
}

export function ServiceDetailPage(props: {
  stackId: string
  serviceId: string
  onComposeHint: (hint: { path?: string; profile?: string; lastScan?: string }) => void
  onTopActions: (node: React.ReactNode) => void
}) {
  const { stackId, serviceId, onComposeHint, onTopActions } = props
  const [stack, setStack] = useState<StackDetail | null>(null)
  const [service, setService] = useState<Service | null>(null)
  const [settings, setSettings] = useState<ServiceSettings | null>(null)
  const [rules, setRules] = useState<IgnoreRule[]>([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const [newRuleKind, setNewRuleKind] = useState<'exact' | 'prefix' | 'regex' | 'semver'>('regex')
  const [newRuleValue, setNewRuleValue] = useState('.*')
  const [newRuleNote, setNewRuleNote] = useState('blocked via UI')

  const refresh = useCallback(async () => {
    setError(null)
    const st = await getStack(stackId)
    setStack(st)
    const svc = st.services.find((s) => s.id === serviceId) ?? null
    setService(svc)
    onComposeHint({
      path: st.compose.composeFiles[0],
      profile: st.name,
      lastScan: undefined,
    })
    setSettings(await getServiceSettings(serviceId))
    const allRules = await listIgnores()
    setRules(allRules.filter((r) => r.scope.serviceId === serviceId))
  }, [onComposeHint, serviceId, stackId])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(errorMessage(e)))
  }, [refresh])

  useEffect(() => {
    onTopActions(
      <>
        <Button
          variant="primary"
          disabled={busy || !service}
          onClick={() => {
            void (async () => {
              if (!service) return
              setBusy(true)
              setError(null)
              try {
                await triggerUpdate({
                  scope: 'service',
                  stackId,
                  serviceId,
                  mode: 'dry-run',
                  allowArchMismatch: false,
                  backupMode: 'inherit',
                })
              } catch (e: unknown) {
                setError(errorMessage(e))
              } finally {
                setBusy(false)
              }
            })()
          }}
        >
          预览更新
        </Button>
        <Button
          variant="danger"
          disabled={busy}
          onClick={() => {
            void (async () => {
              setBusy(true)
              setError(null)
              try {
                await createIgnore({
                  enabled: true,
                  serviceId,
                  kind: 'regex',
                  value: '.*',
                  note: 'blocked via UI',
                })
                await refresh()
              } catch (e: unknown) {
                setError(errorMessage(e))
              } finally {
                setBusy(false)
              }
            })()
          }}
        >
          阻止此服务更新
        </Button>
      </>,
    )
  }, [busy, onTopActions, refresh, service, serviceId, stackId])

  const bindTargets = useMemo(() => (settings ? formatMap(settings.backupTargets.bindPaths) : []), [settings])
  const volTargets = useMemo(() => (settings ? formatMap(settings.backupTargets.volumeNames) : []), [settings])

  const tone = useMemo(() => (service ? svcTone(service) : 'muted'), [service])
  const bannerClass =
    tone === 'ok' ? 'svcBanner svcBannerOk' : tone === 'warn' ? 'svcBanner svcBannerWarn' : tone === 'bad' ? 'svcBanner svcBannerBad' : 'svcBanner svcBannerMuted'
  const dotClass =
    tone === 'ok'
      ? 'svcBannerDot svcBannerDotOk'
      : tone === 'warn'
        ? 'svcBannerDot svcBannerDotWarn'
        : tone === 'bad'
          ? 'svcBannerDot svcBannerDotBad'
          : 'svcBannerDot'

  const bannerTitle = useMemo(() => {
    if (!service) return '加载中…'
    if (service.ignore?.matched) return '已阻止（忽略规则命中）'
    if (!service.candidate) return '暂无候选版本'
    if (service.candidate.archMatch === 'mismatch') return '架构不匹配（仅提示，不允许更新）'
    if (service.candidate.archMatch === 'unknown') return '新版本提示（同前缀/同 minor）'
    return '可更新（已发现新 digest）'
  }, [service])

  const bannerDetail = useMemo(() => {
    if (!service) return ''
    const current = `${service.image.tag}${service.image.digest ? `@${shortDigest(service.image.digest)}` : ''}`
    if (service.ignore?.matched) {
      const why = service.ignore.reason ? ` · reason: ${service.ignore.reason}` : ''
      return `当前: ${current} · rule: ${service.ignore.ruleId}${why}`
    }
    if (!service.candidate) return `当前: ${current}`
    const cand = `${service.candidate.tag}@${shortDigest(service.candidate.digest)}`
    const arch = service.candidate.arch.length ? ` · arch=${service.candidate.arch.join(',')}` : ''
    return `当前: ${current} → 候选: ${cand}${arch}`
  }, [service])

  if (!stack || !service || !settings) {
    return <div className="muted">加载中…</div>
  }

  return (
    <div className="page">
      <div className="svcTitleRow">
        <div className="svcTitleMain">
          <div className="svcTitleNameRow">
            <div className="svcTitleName">
              服务: <Mono>{service.name}</Mono>
            </div>
            <Pill tone="muted">{stack.name}</Pill>
          </div>
          <div className="mono">{service.image.ref}</div>
          <div className="muted">
            id <Mono>{service.id}</Mono> · stack <Mono>{stack.id}</Mono>
          </div>
        </div>
      </div>

      <div className={bannerClass}>
        <div className="svcBannerTitleRow">
          <span className={dotClass} />
          <div className="svcBannerTitle">{bannerTitle}</div>
          <div style={{ marginLeft: 'auto' }}>
            <Pill tone={tone}>{svcBadge(service)}</Pill>
          </div>
        </div>
        <div className="svcBannerDetail">{bannerDetail}</div>
      </div>

      <div className="twoCol">
        <div className="card">
          <div className="title">更新策略</div>
          <div className="muted">忽略后不计入“可更新”，但保留可追溯记录</div>

          <div className="ruleList">
            {rules.map((r) => (
              <div key={r.id} className="ruleRow" style={{ display: 'flex', gap: 12, alignItems: 'flex-start' }}>
                <div style={{ flex: 1 }}>
                  <div className="mono">
                    {r.match.kind}={r.match.value}
                  </div>
                  <div className="muted">
                    id <Mono>{r.id}</Mono> · enabled <Mono>{String(r.enabled)}</Mono>
                    {r.note ? (
                      <>
                        {' '}
                        · note <Mono>{r.note}</Mono>
                      </>
                    ) : null}
                  </div>
                </div>
                <Button
                  variant="ghost"
                  disabled={busy}
                  onClick={() => {
                    void (async () => {
                      setBusy(true)
                      setError(null)
                      try {
                        await deleteIgnore(r.id)
                        await refresh()
                      } catch (e: unknown) {
                        setError(errorMessage(e))
                      } finally {
                        setBusy(false)
                      }
                    })()
                  }}
                >
                  删除
                </Button>
              </div>
            ))}
            {rules.length === 0 ? <div className="muted">暂无规则</div> : null}
          </div>

          <div className="sectionTitle" style={{ marginTop: 14 }}>
            添加规则
          </div>
          <div className="formGrid">
            <label className="formField">
              <span className="label">Kind</span>
              <select
                className="input"
                value={newRuleKind}
                onChange={(e) => setNewRuleKind(e.target.value as 'exact' | 'prefix' | 'regex' | 'semver')}
              >
                <option value="exact">exact</option>
                <option value="prefix">prefix</option>
                <option value="regex">regex</option>
                <option value="semver">semver</option>
              </select>
            </label>
            <label className="formField formSpan2">
              <span className="label">Value</span>
              <input className="input" value={newRuleValue} onChange={(e) => setNewRuleValue(e.target.value)} />
            </label>
            <label className="formField formSpan2">
              <span className="label">Note</span>
              <input className="input" value={newRuleNote} onChange={(e) => setNewRuleNote(e.target.value)} />
            </label>
            <div className="formActions formSpan2">
              <Button
                variant="primary"
                disabled={busy}
                onClick={() => {
                  void (async () => {
                    setBusy(true)
                    setError(null)
                    try {
                      await createIgnore({
                        enabled: true,
                        serviceId,
                        kind: newRuleKind,
                        value: newRuleValue,
                        note: newRuleNote,
                      })
                      await refresh()
                    } catch (e: unknown) {
                      setError(errorMessage(e))
                    } finally {
                      setBusy(false)
                    }
                  })()
                }}
              >
                添加
              </Button>
            </div>
          </div>
        </div>

        <div className="card">
          <div className="title">更新前备份 / 回滚</div>
          <div className="muted">服务级策略（失败回滚 + 备份 targets 三态选择）</div>

          <div className="kv">
            <div className="kvRow">
              <div className="label">失败回滚（autoRollback）</div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                <Switch checked={settings.autoRollback} disabled={busy} onChange={(v) => setSettings({ ...settings, autoRollback: v })} />
                <div className="muted">{settings.autoRollback ? 'on' : 'off'}</div>
              </div>
            </div>
          </div>

          <div className="sectionTitle" style={{ marginTop: 14 }}>
            备份项（服务级）
          </div>
          <div className="muted">三态：inherit / skip / force</div>

          <div className="kv" style={{ marginTop: 10 }}>
            <div className="label">Bind paths</div>
            {bindTargets.length === 0 ? <div className="muted">暂无</div> : null}
            {bindTargets.map((t) => (
              <div key={t.key} className="kvRow">
                <div className="mono">{t.key}</div>
                <select
                  className="input"
                  value={t.value}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      backupTargets: {
                        ...settings.backupTargets,
                        bindPaths: {
                          ...settings.backupTargets.bindPaths,
                          [t.key]: e.target.value as 'inherit' | 'skip' | 'force',
                        },
                      },
                    })
                  }
                >
                  <option value="inherit">inherit</option>
                  <option value="skip">skip</option>
                  <option value="force">force</option>
                </select>
              </div>
            ))}

            <div className="label" style={{ marginTop: 10 }}>
              Volume names
            </div>
            {volTargets.length === 0 ? <div className="muted">暂无</div> : null}
            {volTargets.map((t) => (
              <div key={t.key} className="kvRow">
                <div className="mono">{t.key}</div>
                <select
                  className="input"
                  value={t.value}
                  onChange={(e) =>
                    setSettings({
                      ...settings,
                      backupTargets: {
                        ...settings.backupTargets,
                        volumeNames: {
                          ...settings.backupTargets.volumeNames,
                          [t.key]: e.target.value as 'inherit' | 'skip' | 'force',
                        },
                      },
                    })
                  }
                >
                  <option value="inherit">inherit</option>
                  <option value="skip">skip</option>
                  <option value="force">force</option>
                </select>
              </div>
            ))}

            <div className="formActions">
              <Button
                variant="primary"
                disabled={busy}
                onClick={() => {
                  void (async () => {
                    setBusy(true)
                    setError(null)
                    try {
                      await putServiceSettings(props.serviceId, settings)
                      await refresh()
                    } catch (e: unknown) {
                      setError(errorMessage(e))
                    } finally {
                      setBusy(false)
                    }
                  })()
                }}
              >
                保存服务设置
              </Button>
            </div>
          </div>
        </div>
      </div>

      <div className="card" style={{ marginTop: 16 }}>
        <div className="title">Webhook 触发（服务级）</div>
        <div className="muted">用于外部系统触发：更新此服务 / 更新 compose / 更新全部</div>

        <div className="webhookRow">
          <div className="label">POST</div>
          <div className="mono">/api/v1/update/service/{service.name}</div>
          <div style={{ marginLeft: 'auto' }} className="chipStatic">
            需要鉴权
          </div>
        </div>
        <div className="webhookBody">
          <div className="label">Body（可选）</div>
          <div className="mono">{`{ "dryRun": true, "backup": "inherit" }`}</div>
          <div className="muted">dryRun=仅预览；backup=inherit/on/off；rollback=inherit/on/off</div>
        </div>
      </div>

      {error ? <div className="error">{error}</div> : null}
    </div>
  )
}
