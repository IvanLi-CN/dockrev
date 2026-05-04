import { useCallback, useEffect, useState, type ReactNode } from 'react'
import {
  createIgnore,
  deleteIgnore,
  inferServiceRepoLink,
  listJobs,
  putServiceSettings,
  type JobListItem,
  type Service,
} from '../api'
import { navigate } from '../routes'
import { Button, IconButton, Input, Mono, Pill, RefreshIcon, SelectField, Switch } from '../ui'
import { isDockrevImageRef } from '../runtimeConfig'
import { serviceRowStatus } from '../updateStatus'
import { ServiceResourcePanel } from '../components/ServiceResourcePanel'
import { AutoUpdatePolicyEditor, createDefaultAutoUpdatePolicy } from '../components/AutoUpdatePolicyEditor'
import { AutoUpdatePolicyResultCard } from '../components/AutoUpdatePolicyResultCard'
import { RecentUpdateRecords, selectRecentServiceUpdateJobs } from '../components/RecentUpdateRecords'
import { ResponsiveSettingsDrawer } from '../components/ResponsiveSettingsDrawer'
import {
  ImageLinkIcons,
  RepositoryLinkIcon,
  splitImageNameForDisplay,
  splitImageRef,
} from '../imageLinks'
import { useServiceDetailPageState } from './useServiceDetailPageState'

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function svcBadge(svc: Service): string {
  const st = serviceRowStatus(svc)
  if (st === 'blocked') return '被阻止'
  if (st === 'archMismatch') return '架构不匹配'
  if (st === 'hint') return '需确认'
  if (st === 'updatable') return '可更新'
  return '无候选'
}

function isDockrevService(svc: Service): boolean {
  return isDockrevImageRef(svc.image.ref)
}

export function ServiceDetailPage(props: {
  stackId: string
  serviceId: string
  onLastScanHint: (lastScan?: string) => void
  onTopActions: (node: ReactNode) => void
}) {
  const {
    anomalyCandidateTag,
    anomalyCurrentTag,
    bannerClass,
    bannerDetail,
    bannerTitle,
    bindTargets,
    busy,
    composeEnvFile,
    composeFiles,
    composeType,
    dotClass,
    draftRepoUrl,
    error,
    newRuleKind,
    newRuleNote,
    newRuleValue,
    notice,
    repoInferBusy,
    requestRefresh,
    rules,
    semverDowngradeAnomaly,
    service,
    serviceId,
    setBusy,
    setError,
    setNewRuleKind,
    setNewRuleNote,
    setNewRuleValue,
    setRepoInferBusy,
    setSettings,
    settings,
    settingsBusy,
    stack,
    stackSettings,
    supervisorErrorAt,
    supervisorState,
    tone,
    volTargets,
  } = useServiceDetailPageState(props)
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [settingsDrawerOpen, setSettingsDrawerOpen] = useState(false)

  const refreshRecentJobs = useCallback(async () => {
    setJobs(await listJobs())
  }, [])

  useEffect(() => {
    void refreshRecentJobs().catch(() => undefined)
  }, [props.serviceId, refreshRecentJobs])

  useEffect(() => {
    if (!notice?.jobId) return
    void refreshRecentJobs().catch(() => undefined)
  }, [notice?.jobId, refreshRecentJobs])

  if (!stack || !service || !settings) {
    return <div className="muted">加载中…</div>
  }

  const policy = settings.autoUpdatePolicy ?? createDefaultAutoUpdatePolicy('inherit')
  const recentUpdateJobs = selectRecentServiceUpdateJobs(jobs, service.id)

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
          {(() => {
            const img = splitImageRef(service.image.ref)
            const dn = splitImageNameForDisplay(img.name, service.image.tag)
            return (
              <div className="cellTwoLine">
                <div
                  className="mono monoPrimary monoSplit imageLinkRow"
                  title={dn.suffix ? `${dn.base}${dn.suffix}` : dn.base}
                >
                  <span className="monoSplitBase">{dn.base}</span>
                  <ImageLinkIcons imageRef={service.image.ref} repoUrl={draftRepoUrl} />
                </div>
                <div className="mono monoSecondary">{img.registry}</div>
              </div>
            )
          })()}
          <div className="muted">
            id <Mono>{service.id}</Mono> · stack <Mono>{stack.id}</Mono>
          </div>
        </div>
      </div>

      <div className="card svcComposeCard">
        <div className="title">Compose 信息</div>
        <div className="kv">
          <div className="kvRow">
            <div className="muted">type</div>
            <div className="mono">{composeType}</div>
          </div>
          <div className="kvRow">
            <div className="muted">compose files</div>
            {composeFiles.length > 0 ? (
              <div>
                {composeFiles.map((item, index) => (
                  <div key={`${item}-${index}`} className="mono">
                    {item}
                  </div>
                ))}
              </div>
            ) : (
              <div className="mono">-</div>
            )}
          </div>
          <div className="kvRow">
            <div className="muted">env file</div>
            <div className="mono">{composeEnvFile}</div>
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

      {semverDowngradeAnomaly ? (
        <div className="svcAnomalyAlert" role="alert">
          <div className="svcAnomalyAlertTitle">
            <span className="svcAnomalyAlertIcon" aria-hidden="true">
              ⚠
            </span>
            <span>版本异常：候选版本低于当前版本</span>
          </div>
          <div className="svcAnomalyAlertText">
            当前 <Mono>{anomalyCurrentTag}</Mono> → 候选 <Mono>{anomalyCandidateTag}</Mono>。手动更新仍可继续，请确认这是预期降级。
          </div>
        </div>
      ) : null}

      {isDockrevService(service) && supervisorState.status === 'offline' ? (
        <div className="muted" style={{ marginTop: 10 }}>
          supervisor offline · {supervisorErrorAt ?? '-'}
        </div>
      ) : null}

      <ServiceResourcePanel serviceId={service.id} />

      <div className="settingsSummaryGrid" style={{ marginTop: 16 }}>
        <AutoUpdatePolicyResultCard
          busy={settingsBusy}
          onOpenSettings={() => setSettingsDrawerOpen(true)}
          policy={policy}
          scope="service"
          stackPolicy={stackSettings?.autoUpdatePolicy ?? null}
        />
        <RecentUpdateRecords jobs={recentUpdateJobs} />
      </div>

      <div className="card" style={{ marginTop: 16 }}>
          <div className="title">忽略规则</div>

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
                        await requestRefresh()
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
              <SelectField
                className="input"
                onChange={(value) => setNewRuleKind(value as 'exact' | 'prefix' | 'regex' | 'semver')}
                options={[
                  { value: 'exact', label: 'exact' },
                  { value: 'prefix', label: 'prefix' },
                  { value: 'regex', label: 'regex' },
                  { value: 'semver', label: 'semver' },
                ]}
                value={newRuleKind}
              />
            </label>
            <label className="formField formSpan2">
              <span className="label">Value</span>
              <Input className="input" onChange={(e) => setNewRuleValue(e.target.value)} value={newRuleValue} />
            </label>
            <label className="formField formSpan2">
              <span className="label">Note</span>
              <Input className="input" onChange={(e) => setNewRuleNote(e.target.value)} value={newRuleNote} />
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
                      await requestRefresh()
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

      <ResponsiveSettingsDrawer
        description="配置自动更新策略、失败回滚和服务级备份目标。"
        onOpenChange={setSettingsDrawerOpen}
        open={settingsDrawerOpen}
        title="服务设置"
      >
        <AutoUpdatePolicyEditor
          busy={settingsBusy}
          onChange={(autoUpdatePolicy) => setSettings({ ...settings, autoUpdatePolicy })}
          onSave={() => {
            void (async () => {
              setBusy(true)
              setError(null)
              try {
                await putServiceSettings(props.serviceId, {
                  ...settings,
                  repoUrl: undefined,
                })
                await requestRefresh()
              } catch (e: unknown) {
                setError(errorMessage(e))
              } finally {
                setBusy(false)
              }
            })()
          }}
          policy={policy}
          scope="service"
          stackPolicy={stackSettings?.autoUpdatePolicy ?? null}
        />
        <div className="settingsDrawerDivider" />
        <div className="settingsDrawerSection">
          <div className="title">更新前备份 / 回滚</div>
          <div className="muted">服务级策略（失败回滚 + 备份 targets 三态选择）</div>

          <div className="kv">
            <div className="kvRow">
              <div className="label">失败回滚（autoRollback）</div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                <Switch checked={settings.autoRollback} disabled={settingsBusy} onChange={(v) => setSettings({ ...settings, autoRollback: v })} />
                <div className="muted">{settings.autoRollback ? 'on' : 'off'}</div>
              </div>
            </div>
            <div className="kvRow">
              <div className="label">代码仓库</div>
              <div>
                <div className="serviceRepoField">
                  <Input
                    className="input"
                    disabled={settingsBusy}
                    onChange={(e) => setSettings({ ...settings, repoUrl: e.target.value })}
                    placeholder="https://github.com/owner/repo"
                    value={settings.repoUrl ?? ''}
                  />
                  <RepositoryLinkIcon repoUrl={draftRepoUrl} />
                  <IconButton
                    disabled={settingsBusy}
                    hint={repoInferBusy ? '正在重新推断代码仓库…' : '根据镜像 OCI source / GHCR 重新推断'}
                    onClick={() => {
                      void (async () => {
                        setRepoInferBusy(true)
                        setError(null)
                        try {
                          const result = await inferServiceRepoLink(props.serviceId)
                          if (result.repoUrl) {
                            setSettings((prev) => (prev ? { ...prev, repoUrl: result.repoUrl } : prev))
                          } else {
                            setError(result.reason?.trim() || '未识别到代码仓库入口')
                          }
                        } catch (e: unknown) {
                          setError(errorMessage(e))
                        } finally {
                          setRepoInferBusy(false)
                        }
                      })()
                    }}
                    title="重新推断代码仓库"
                  >
                    <RefreshIcon className={repoInferBusy ? 'inlineIcon inlineIconLoading' : 'inlineIcon'} />
                  </IconButton>
                </div>
                <div className="muted">清空并保存会禁用后续自动补齐；再次手动推断并保存可恢复。</div>
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
                <SelectField
                  className="input"
                  disabled={settingsBusy}
                  onChange={(value) =>
                    setSettings({
                      ...settings,
                      backupTargets: {
                        ...settings.backupTargets,
                        bindPaths: {
                          ...settings.backupTargets.bindPaths,
                          [t.key]: value as 'inherit' | 'skip' | 'force',
                        },
                      },
                    })
                  }
                  options={[
                    { value: 'inherit', label: 'inherit' },
                    { value: 'skip', label: 'skip' },
                    { value: 'force', label: 'force' },
                  ]}
                  value={t.value}
                />
              </div>
            ))}

            <div className="label" style={{ marginTop: 10 }}>
              Volume names
            </div>
            {volTargets.length === 0 ? <div className="muted">暂无</div> : null}
            {volTargets.map((t) => (
              <div key={t.key} className="kvRow">
                <div className="mono">{t.key}</div>
                <SelectField
                  className="input"
                  disabled={settingsBusy}
                  onChange={(value) =>
                    setSettings({
                      ...settings,
                      backupTargets: {
                        ...settings.backupTargets,
                        volumeNames: {
                          ...settings.backupTargets.volumeNames,
                          [t.key]: value as 'inherit' | 'skip' | 'force',
                        },
                      },
                    })
                  }
                  options={[
                    { value: 'inherit', label: 'inherit' },
                    { value: 'skip', label: 'skip' },
                    { value: 'force', label: 'force' },
                  ]}
                  value={t.value}
                />
              </div>
            ))}

            <div className="formActions">
              <Button
                variant="primary"
                disabled={settingsBusy}
                onClick={() => {
                  void (async () => {
                    setBusy(true)
                    setError(null)
                    try {
                      await putServiceSettings(props.serviceId, {
                        ...settings,
                        repoUrl: (settings.repoUrl ?? '').trim() || null,
                      })
                      await requestRefresh()
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
      </ResponsiveSettingsDrawer>

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
      {notice ? (
        <div className="success">
          已创建{notice.kind === 'rollback' ? '回滚' : '更新'}任务 <Mono>{notice.jobId}</Mono> ·{' '}
          <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: 'queue' })}>
            查看队列
          </Button>
        </div>
      ) : null}
    </div>
  )
}
