import { useCallback, useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react'
import {
  createIgnore,
  deleteIgnore,
  inferServiceRepoLink,
  listServiceTagSuggestions,
  listJobs,
  putServiceComposeTag,
  putServiceSettings,
  type JobListItem,
  type Service,
  type ServiceSettings,
  type ServiceTagSuggestionItem,
} from '../api'
import { navigate } from '../routes'
import { Button, IconButton, Input, Mono, Pill, RefreshIcon, SelectField, Switch } from '../ui'
import { isDockrevImageRef } from '../runtimeConfig'
import { serviceRowStatus } from '../updateStatus'
import { ServiceResourcePanel } from '../components/ServiceResourcePanel'
import { createDefaultAutoUpdatePolicy } from '../components/AutoUpdatePolicyEditor'
import { AutoUpdatePolicyDrawer } from '../components/AutoUpdatePolicyDrawer'
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

function formatSuggestionTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value || '-'
  return date.toLocaleString()
}

function ServiceComposeTagField(props: {
  busy: boolean
  currentTag: string
  serviceId: string
  onError: (message: string | null) => void
  onSaved: () => Promise<void>
}) {
  const [value, setValue] = useState(props.currentTag)
  const [open, setOpen] = useState(false)
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [loaded, setLoaded] = useState(false)
  const [items, setItems] = useState<ServiceTagSuggestionItem[]>([])
  const [fieldError, setFieldError] = useState<string | null>(null)
  const [activeIndex, setActiveIndex] = useState(0)
  const [filterSuggestions, setFilterSuggestions] = useState(false)
  const comboId = useId()
  const closeTimerRef = useRef<number | null>(null)
  const listboxId = `${comboId}-tag-suggestions`

  const filteredItems = useMemo(() => {
    const query = filterSuggestions ? value.trim().toLowerCase() : ''
    if (!query) return items
    return items.filter((item) => item.tag.toLowerCase().includes(query))
  }, [filterSuggestions, items, value])

  useEffect(() => {
    setValue(props.currentTag)
    setOpen(false)
    setLoaded(false)
    setItems([])
    setFieldError(null)
    setActiveIndex(0)
    setFilterSuggestions(false)
  }, [props.currentTag, props.serviceId])

  useEffect(() => {
    if (!open) return
    setActiveIndex((current) => Math.min(Math.max(current, 0), Math.max(filteredItems.length - 1, 0)))
  }, [filteredItems.length, open])

  const loadSuggestions = useCallback(async () => {
    if (loaded || loading) return
    setLoading(true)
    setFieldError(null)
    try {
      const resp = await listServiceTagSuggestions(props.serviceId)
      setItems(resp.items)
      setLoaded(true)
    } catch (e: unknown) {
      setFieldError(errorMessage(e))
    } finally {
      setLoading(false)
    }
  }, [loaded, loading, props.serviceId])

  const openSuggestions = useCallback(() => {
    if (closeTimerRef.current != null) {
      window.clearTimeout(closeTimerRef.current)
      closeTimerRef.current = null
    }
    setOpen(true)
    void loadSuggestions()
  }, [loadSuggestions])

  const scheduleClose = useCallback(() => {
    closeTimerRef.current = window.setTimeout(() => {
      setOpen(false)
    }, 120)
  }, [])

  const selectSuggestion = useCallback((item: ServiceTagSuggestionItem) => {
    setValue(item.tag)
    setOpen(false)
    setActiveIndex(0)
    setFilterSuggestions(false)
  }, [])

  const save = useCallback(async () => {
    const next = value.trim()
    setFieldError(null)
    props.onError(null)
    if (!next) {
      setFieldError('tag 不能为空')
      return
    }
    setSaving(true)
    try {
      await putServiceComposeTag(props.serviceId, next)
      setLoaded(false)
      setItems([])
      setOpen(false)
      await props.onSaved()
    } catch (e: unknown) {
      setFieldError(errorMessage(e))
    } finally {
      setSaving(false)
    }
  }, [props, value])

  const disabled = props.busy || saving
  return (
    <div className="serviceTagEditor">
      <div className="serviceTagEditorHeader">
        <div>
          <div className="label">部署 tag</div>
          <div className="muted">写回原始 Compose 文件；保存后不会自动执行 compose up。</div>
        </div>
        <div className="chipStatic">
          当前 <Mono>{props.currentTag || '-'}</Mono>
        </div>
      </div>
      <div className="serviceTagEditorControls">
        <div className="serviceTagInputWrap">
          <Input
            aria-activedescendant={
              open && filteredItems[activeIndex] ? `${listboxId}-option-${activeIndex}` : undefined
            }
            aria-autocomplete="list"
            aria-controls={listboxId}
            aria-expanded={open}
            autoComplete="off"
            className="input"
            disabled={disabled}
            onBlur={scheduleClose}
            onChange={(e) => {
              setValue(e.target.value)
              setOpen(true)
              setActiveIndex(0)
              setFilterSuggestions(true)
              void loadSuggestions()
            }}
            onFocus={openSuggestions}
            onKeyDown={(e) => {
              if (e.key === 'ArrowDown') {
                e.preventDefault()
                setOpen(true)
                void loadSuggestions()
                setActiveIndex((current) => Math.min(current + 1, Math.max(filteredItems.length - 1, 0)))
                return
              }
              if (e.key === 'ArrowUp') {
                e.preventDefault()
                setOpen(true)
                setActiveIndex((current) => Math.max(current - 1, 0))
                return
              }
              if (e.key === 'Enter') {
                if (open && filteredItems[activeIndex]) {
                  e.preventDefault()
                  selectSuggestion(filteredItems[activeIndex])
                  return
                }
                e.preventDefault()
                void save()
                return
              }
              if (e.key === 'Escape') setOpen(false)
            }}
            placeholder="例如 5.2.3 或 stable"
            role="combobox"
            value={value}
          />
          {open ? (
            <div className="serviceTagSuggestionMenu" id={listboxId} role="listbox">
              {loading ? <div className="serviceTagSuggestionEmpty">加载历史 tag…</div> : null}
              {!loading && fieldError ? <div className="serviceTagSuggestionEmpty">{fieldError}</div> : null}
              {!loading && !fieldError && filteredItems.length === 0 ? (
                <div className="serviceTagSuggestionEmpty">{items.length === 0 ? '暂无历史 tag' : '没有匹配的历史 tag'}</div>
              ) : null}
              {!loading && !fieldError
                ? filteredItems.map((item, index) => (
                    <button
                      aria-selected={index === activeIndex}
                      className={`serviceTagSuggestionItem${index === activeIndex ? ' active' : ''}`}
                      id={`${listboxId}-option-${index}`}
                      key={`${item.tag}-${item.lastUsedAt}`}
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => selectSuggestion(item)}
                      onMouseEnter={() => setActiveIndex(index)}
                      role="option"
                      type="button"
                    >
                      <span className="mono monoPrimary">{item.tag}</span>
                      <span className="muted">{formatSuggestionTime(item.lastUsedAt)}</span>
                    </button>
                  ))
                : null}
            </div>
          ) : null}
        </div>
        <Button variant="primary" disabled={disabled || value.trim() === props.currentTag.trim()} onClick={() => void save()}>
          {saving ? '保存中…' : '保存 tag'}
        </Button>
      </div>
      {fieldError ? <div className="serviceTagFieldError">{fieldError}</div> : null}
    </div>
  )
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
    settings,
    settingsBusy,
    stack,
    stackSettings,
    supervisorErrorAt,
    supervisorState,
    tone,
  } = useServiceDetailPageState(props)
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [settingsDrawerOpen, setSettingsDrawerOpen] = useState(false)
  const [tagDrawerOpen, setTagDrawerOpen] = useState(false)
  const [serviceSettingsDrawerOpen, setServiceSettingsDrawerOpen] = useState(false)
  const [autoPolicyDraft, setAutoPolicyDraft] = useState(() => createDefaultAutoUpdatePolicy('inherit'))
  const [serviceSettingsDraft, setServiceSettingsDraft] = useState<ServiceSettings | null>(null)

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
  const serviceProtectionDraft = serviceSettingsDraft ?? settings
  const serviceProtectionBindTargets = Object.entries(serviceProtectionDraft.backupTargets.bindPaths).map(
    ([key, value]) => ({ key, value }),
  )
  const serviceProtectionVolTargets = Object.entries(serviceProtectionDraft.backupTargets.volumeNames).map(
    ([key, value]) => ({ key, value }),
  )
  const visibleRepoUrl = serviceSettingsDrawerOpen ? serviceProtectionDraft.repoUrl : draftRepoUrl
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
                  <ImageLinkIcons imageRef={service.image.ref} repoUrl={visibleRepoUrl} />
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
          onOpenSettings={() => {
            setAutoPolicyDraft(policy)
            setSettingsDrawerOpen(true)
          }}
          policy={policy}
          scope="service"
          stackPolicy={stackSettings?.autoUpdatePolicy ?? null}
        />
        <RecentUpdateRecords jobs={recentUpdateJobs} />
      </div>

      <div className="card serviceSafeguardCard">
        <div>
          <div className="title">部署 tag</div>
          <div className="muted">直接写回原始 Compose 文件里的镜像 tag，不自动执行 compose up。</div>
        </div>
        <div className="serviceTagCardActions">
          <div className="chipStatic">
            当前 <Mono>{service.image.tag || '-'}</Mono>
          </div>
          <Button disabled={settingsBusy} onClick={() => setTagDrawerOpen(true)}>
            编辑 tag
          </Button>
        </div>
      </div>

      <div className="card serviceSafeguardCard">
        <div>
          <div className="title">服务保护设置</div>
          <div className="muted">失败回滚、代码仓库和备份目标单独配置，不与自动更新策略混排。</div>
        </div>
        <Button
          disabled={settingsBusy}
          onClick={() => {
            setServiceSettingsDraft(settings)
            setServiceSettingsDrawerOpen(true)
          }}
        >
          打开
        </Button>
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

      <AutoUpdatePolicyDrawer
        busy={settingsBusy}
        onChange={setAutoPolicyDraft}
        onOpenChange={setSettingsDrawerOpen}
        onSave={() => {
          void (async () => {
            setBusy(true)
            setError(null)
            try {
              await putServiceSettings(props.serviceId, {
                ...settings,
                autoUpdatePolicy: autoPolicyDraft,
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
        open={settingsDrawerOpen}
        policy={autoPolicyDraft}
        previewServiceId={service.id}
        scope="service"
        stackPolicy={stackSettings?.autoUpdatePolicy ?? null}
      />

      <ResponsiveSettingsDrawer
        description="写回原始 Compose 文件里的镜像 tag；保存后不会自动执行 compose up。"
        onOpenChange={setTagDrawerOpen}
        open={tagDrawerOpen}
        title="部署 tag"
      >
        <div className="settingsDrawerSection">
          <ServiceComposeTagField
            busy={settingsBusy}
            currentTag={service.image.tag}
            onError={setError}
            onSaved={requestRefresh}
            serviceId={props.serviceId}
          />
        </div>
      </ResponsiveSettingsDrawer>

      <ResponsiveSettingsDrawer
        description="配置失败回滚、代码仓库和服务级备份目标。"
        onOpenChange={setServiceSettingsDrawerOpen}
        open={serviceSettingsDrawerOpen}
        title="服务保护设置"
      >
        <div className="settingsDrawerSection">
          <div className="title">更新前备份 / 回滚</div>
          <div className="muted">服务级策略（失败回滚 + 备份 targets 三态选择）</div>

          <div className="kv">
            <div className="kvRow">
              <div className="label">失败回滚（autoRollback）</div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                <Switch
                  checked={serviceProtectionDraft.autoRollback}
                  disabled={settingsBusy}
                  onChange={(autoRollback) =>
                    setServiceSettingsDraft({ ...serviceProtectionDraft, autoRollback })
                  }
                />
                <div className="muted">{serviceProtectionDraft.autoRollback ? 'on' : 'off'}</div>
              </div>
            </div>
            <div className="kvRow">
              <div className="label">代码仓库</div>
              <div>
                <div className="serviceRepoField">
                  <Input
                    className="input"
                    disabled={settingsBusy}
                    onChange={(e) => setServiceSettingsDraft({ ...serviceProtectionDraft, repoUrl: e.target.value })}
                    placeholder="https://github.com/owner/repo"
                    value={serviceProtectionDraft.repoUrl ?? ''}
                  />
                  <RepositoryLinkIcon repoUrl={serviceProtectionDraft.repoUrl ?? draftRepoUrl} />
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
                            setServiceSettingsDraft({ ...serviceProtectionDraft, repoUrl: result.repoUrl })
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
            {serviceProtectionBindTargets.length === 0 ? <div className="muted">暂无</div> : null}
            {serviceProtectionBindTargets.map((t) => (
              <div key={t.key} className="kvRow">
                <div className="mono">{t.key}</div>
                <SelectField
                  className="input"
                  disabled={settingsBusy}
                  onChange={(value) =>
                    setServiceSettingsDraft({
                      ...serviceProtectionDraft,
                      backupTargets: {
                        ...serviceProtectionDraft.backupTargets,
                        bindPaths: {
                          ...serviceProtectionDraft.backupTargets.bindPaths,
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
            {serviceProtectionVolTargets.length === 0 ? <div className="muted">暂无</div> : null}
            {serviceProtectionVolTargets.map((t) => (
              <div key={t.key} className="kvRow">
                <div className="mono">{t.key}</div>
                <SelectField
                  className="input"
                  disabled={settingsBusy}
                  onChange={(value) =>
                    setServiceSettingsDraft({
                      ...serviceProtectionDraft,
                      backupTargets: {
                        ...serviceProtectionDraft.backupTargets,
                        volumeNames: {
                          ...serviceProtectionDraft.backupTargets.volumeNames,
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
                      const draft = serviceSettingsDraft ?? settings
                      await putServiceSettings(props.serviceId, {
                        ...draft,
                        autoUpdatePolicy: settings.autoUpdatePolicy,
                        repoUrl: (draft.repoUrl ?? '').trim() || null,
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
