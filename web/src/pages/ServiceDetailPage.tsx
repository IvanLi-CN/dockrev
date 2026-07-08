import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import {
  ApiError,
  createIgnore,
  deleteIgnore,
  getServiceResourceUsageHistory,
  inferServiceRepoLink,
  listJobs,
  putServiceBackupTargets,
  putServiceSettings,
  type BackupTargetPolicy,
  type JobListItem,
  type Service,
  type ServiceBackupRecordItem,
  type ServiceBackupTargetItem,
  type ServiceBackupTargetsResponse,
  type ServiceResourceUsageWindow,
  type ServiceSettings,
  type StackDetail,
} from '../api'
import { BackupPolicySegmentedControl } from '../components/BackupPolicySegmentedControl'
import { BackupRecordList } from '../components/ServiceBackupRecords'
import { ReadonlySnapshotNotice } from '../components/ReadonlySnapshotNotice'
import { navigate } from '../routes'
import { Button, IconButton, Input, Mono, Pill, RefreshIcon, SelectField, Switch, Tabs, TabsList, TabsTrigger } from '../ui'
import { usePwaStatus } from '../pwaStatus'
import { buildReadonlySnapshotKey, readReadonlySnapshot, writeReadonlySnapshot } from '../readonlySnapshotCache'
import { isDockrevImageRef } from '../runtimeConfig'
import { serviceRowStatus } from '../updateStatus'
import { ServiceResourcePanel, type ServiceResourceSnapshot } from '../components/ServiceResourcePanel'
import { ServiceLogsPanel } from '../components/ServiceLogsPanel'
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
import { ServiceComposeTagField } from './ServiceComposeTagField'
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

type BackupTargetDraftItem = {
  key: string
  policy: BackupTargetPolicy
  relatedServiceCount: number
  relatedServiceIds: string[]
}

type BackupTargetsDraft = {
  bindPaths: BackupTargetDraftItem[]
  volumeNames: BackupTargetDraftItem[]
}

function createBackupTargetsDraft(data: ServiceBackupTargetsResponse | null): BackupTargetsDraft {
  const normalize = (items: ServiceBackupTargetItem[]): BackupTargetDraftItem[] =>
    items.map((item) => ({
      key: item.key,
      policy: item.policy,
      relatedServiceCount: item.relatedServiceCount,
      relatedServiceIds: item.relatedServiceIds,
    }))
  return {
    bindPaths: normalize(data?.bindPaths ?? []),
    volumeNames: normalize(data?.volumeNames ?? []),
  }
}

function backupTargetRequestItems(items: BackupTargetDraftItem[]) {
  return items.map((item) => ({
    key: item.key,
    policy: item.policy,
  }))
}

function backupTargetRequestFromDraft(draft: BackupTargetsDraft) {
  return {
    bindPaths: backupTargetRequestItems(draft.bindPaths),
    volumeNames: backupTargetRequestItems(draft.volumeNames),
  }
}

function formatBackupRetentionSummary(storage: ServiceBackupTargetsResponse['storage']): string {
  const hours = Math.round(storage.deleteAfterStableSeconds / 3600)
  return `目录 ${storage.baseDir} / 产物 .tar.gz / 最近 ${storage.keepLast} 份保留 / 其余稳定 ${hours}h 后清理`
}

function backupPolicyHint(item: BackupTargetDraftItem): string {
  if (item.policy === 'disabled') return '当前服务不会为这个 target 触发自动备份'
  if (item.policy === 'stop_related_services') {
    return item.relatedServiceCount > 1
      ? `备份前会协调停掉这 ${item.relatedServiceCount} 个关联服务，再恢复`
      : '备份前会先停掉当前服务，再恢复'
  }
  return item.relatedServiceCount > 1
    ? `保持这 ${item.relatedServiceCount} 个关联服务运行，直接备份`
    : '保持当前服务运行，直接备份'
}

function backupRelationshipLabel(item: BackupTargetDraftItem): string {
  if (item.relatedServiceCount <= 1) return '关联 1 个服务'
  return `关联 ${item.relatedServiceCount} 个服务`
}

const SERVICE_DETAIL_SNAPSHOT_STALE_MS = 60_000
const SERVICE_DETAIL_MONITORING_WINDOW: ServiceResourceUsageWindow = '1h'

type ServiceDetailSnapshotPayload = {
  stack: StackDetail
  jobs: JobListItem[]
  backupTargets: ServiceBackupTargetsResponse | null
  backupRecords: ServiceBackupRecordItem[]
  monitoring: ServiceResourceSnapshot | null
}

function readReason(details: unknown): string | null {
  if (!details || typeof details !== 'object') return null
  const reason = (details as Record<string, unknown>).reason
  return typeof reason === 'string' ? reason : null
}

function isMonitorDisabledError(error: unknown): boolean {
  return error instanceof ApiError && error.status === 409 && readReason(error.details) === 'resource_monitor_disabled'
}

function ServiceDetailReadonlyBlocked(props: {
  title: string
  detail: string
}) {
  return (
    <div className="card serviceDetailReadonlyBlock">
      <div className="title">{props.title}</div>
      <div className="muted">{props.detail}</div>
    </div>
  )
}

function sanitizeReadonlyStackSnapshot(stack: StackDetail): StackDetail {
  return {
    ...stack,
    services: stack.services.map((service) => ({
      ...service,
      settings: {
        autoRollback: false,
        backupTargets: {
          bindPaths: {},
          volumeNames: {},
        },
      },
    })),
  }
}

export function ServiceDetailPage(props: {
  stackId: string
  serviceId: string
  section?: 'overview' | 'monitoring' | 'backup' | 'logs' | 'settings'
  onLastScanHint: (lastScan?: string) => void
  onTopActions: (node: ReactNode) => void
}) {
  const { onTopActions } = props
  const section = props.section ?? 'overview'
  const { isOnline } = usePwaStatus()
  const snapshotKey = buildReadonlySnapshotKey('service-detail', `${props.stackId}:${props.serviceId}`)
  const {
    anomalyCandidateTag,
    anomalyCurrentTag,
    bannerClass,
    bannerDetail,
    bannerTitle,
    backupRecords,
    busy,
    composeEnvFile,
    composeFiles,
    composeType,
    dotClass,
    draftRepoUrl,
    error,
    lastSuccessfulRefreshAt,
    newRuleKind,
    newRuleNote,
    newRuleValue,
    notice,
    backupTargets,
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
    topActions,
    supervisorErrorAt,
    supervisorState,
    tone,
    dangerousActions,
  } = useServiceDetailPageState(props)
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [monitoringSnapshot, setMonitoringSnapshot] = useState<ServiceResourceSnapshot | null>(null)
  const [snapshotPayload, setSnapshotPayload] = useState<ServiceDetailSnapshotPayload | null>(null)
  const [, setSnapshotStatus] = useState<'missing' | 'fresh' | 'stale' | 'expired' | 'unsupported'>(
    'missing',
  )
  const [snapshotFetchedAt, setSnapshotFetchedAt] = useState<string | null>(null)
  const [snapshotAnchorFetchedAt, setSnapshotAnchorFetchedAt] = useState<string | null>(null)
  const [snapshotActive, setSnapshotActive] = useState(false)
  const [settingsDrawerOpen, setSettingsDrawerOpen] = useState(false)
  const [tagDrawerOpen, setTagDrawerOpen] = useState(false)
  const [serviceSettingsDrawerOpen, setServiceSettingsDrawerOpen] = useState(false)
  const [backupSettingsDrawerOpen, setBackupSettingsDrawerOpen] = useState(false)
  const [autoPolicyDraft, setAutoPolicyDraft] = useState(() => createDefaultAutoUpdatePolicy('inherit'))
  const [serviceSettingsDraft, setServiceSettingsDraft] = useState<ServiceSettings | null>(null)
  const [serviceBackupTargetsDraft, setServiceBackupTargetsDraft] = useState<BackupTargetsDraft>(() =>
    createBackupTargetsDraft(null),
  )

  const refreshRecentJobs = useCallback(async () => {
    setJobs(await listJobs())
  }, [])

  useEffect(() => {
    let cancelled = false
    void (async () => {
      const snapshot = await readReadonlySnapshot<ServiceDetailSnapshotPayload>(snapshotKey)
      if (cancelled) return
      setSnapshotStatus(snapshot.status)
      setSnapshotFetchedAt(snapshot.record?.fetchedAt ?? null)
      setSnapshotAnchorFetchedAt(snapshot.record?.fetchedAt ?? null)
      if (snapshot.status !== 'fresh') return
      setSnapshotPayload(snapshot.record.payload)
      setMonitoringSnapshot(snapshot.record.payload.monitoring ?? null)
      setSnapshotActive(true)
    })()
    return () => {
      cancelled = true
    }
  }, [snapshotKey])

  useEffect(() => {
    void refreshRecentJobs().catch(() => undefined)
  }, [props.serviceId, refreshRecentJobs])

  useEffect(() => {
    if (!notice?.jobId) return
    void refreshRecentJobs().catch(() => undefined)
  }, [notice?.jobId, refreshRecentJobs])

  useEffect(() => {
    let cancelled = false
    if (!isOnline) return undefined
    void (async () => {
      try {
        const response = await getServiceResourceUsageHistory(props.serviceId, SERVICE_DETAIL_MONITORING_WINDOW)
        if (cancelled) return
        setMonitoringSnapshot({
          fetchedAt:
            response.samples.length > 0
              ? response.samples[response.samples.length - 1]?.sampledAt ?? new Date().toISOString()
              : new Date().toISOString(),
          windowKey: SERVICE_DETAIL_MONITORING_WINDOW,
          samples: response.samples,
          monitorDisabled: false,
        })
      } catch (error: unknown) {
        if (cancelled) return
        if (isMonitorDisabledError(error)) {
          setMonitoringSnapshot({
            fetchedAt: new Date().toISOString(),
            windowKey: SERVICE_DETAIL_MONITORING_WINDOW,
            samples: [],
            monitorDisabled: true,
          })
        }
      }
    })()
    return () => {
      cancelled = true
    }
  }, [isOnline, props.serviceId])

  useEffect(() => {
    if (!lastSuccessfulRefreshAt) return
    setSnapshotActive(false)
    setSnapshotAnchorFetchedAt(null)
  }, [lastSuccessfulRefreshAt])

  useEffect(() => {
    if (!stack || !service) return
    void writeReadonlySnapshot(
      snapshotKey,
      {
        stack: sanitizeReadonlyStackSnapshot(stack),
        jobs,
        backupTargets,
        backupRecords,
        monitoring: monitoringSnapshot,
      },
      {
        staleAfterMs: SERVICE_DETAIL_SNAPSHOT_STALE_MS,
        fetchedAt: snapshotAnchorFetchedAt ? Date.parse(snapshotAnchorFetchedAt) || undefined : undefined,
      },
    )
  }, [
    backupRecords,
    backupTargets,
    jobs,
    monitoringSnapshot,
    service,
    snapshotAnchorFetchedAt,
    snapshotKey,
    stack,
  ])

  const snapshotService = useMemo(
    () => snapshotPayload?.stack.services.find((item) => item.id === props.serviceId) ?? null,
    [props.serviceId, snapshotPayload],
  )
  const effectiveStack = stack ?? snapshotPayload?.stack ?? null
  const effectiveService = service ?? snapshotService
  const effectiveJobs = snapshotActive ? snapshotPayload?.jobs ?? jobs : jobs
  const effectiveBackupTargets = snapshotActive ? snapshotPayload?.backupTargets ?? backupTargets : backupTargets
  const effectiveBackupRecords = snapshotActive ? snapshotPayload?.backupRecords ?? backupRecords : backupRecords
  const readonlyUi = !isOnline || snapshotActive

  useEffect(() => {
    if (readonlyUi) {
      onTopActions(
        <>
          <Button disabled={busy} onClick={() => navigate({ name: 'stack', stackId: props.stackId })}>
            Stack 详情
          </Button>
          <Button disabled={busy || !isOnline} onClick={() => void requestRefresh()}>
            刷新
          </Button>
        </>,
      )
      return () => onTopActions(null)
    }
    onTopActions(topActions)
    return () => onTopActions(null)
  }, [busy, isOnline, onTopActions, props.stackId, readonlyUi, requestRefresh, topActions])

  if (!effectiveStack || !effectiveService) {
    if (!isOnline) {
      return (
        <div className="page">
          <ReadonlySnapshotNotice
            tone="bad"
            title="当前没有可用的离线服务详情数据。"
            detail="请恢复联网后重新加载该页面。"
          />
        </div>
      )
    }
    return <div className="muted">加载中…</div>
  }

  const policy = settings?.autoUpdatePolicy ?? stackSettings?.autoUpdatePolicy ?? createDefaultAutoUpdatePolicy('inherit')
  const serviceProtectionDraft = serviceSettingsDraft ?? settings ?? effectiveService.settings
  const visibleRepoUrl = serviceSettingsDrawerOpen ? serviceProtectionDraft.repoUrl : draftRepoUrl
  const recentUpdateJobs = selectRecentServiceUpdateJobs(effectiveJobs, effectiveService.id)
  const sectionValue = section
  const effectiveBannerTitle =
    service != null
      ? bannerTitle
      : serviceRowStatus(effectiveService) === 'blocked'
        ? '已阻止（忽略规则命中）'
        : serviceRowStatus(effectiveService) === 'archMismatch'
          ? '架构不匹配（仅提示，不允许更新）'
          : serviceRowStatus(effectiveService) === 'hint'
            ? '需确认（arch 未知）'
            : serviceRowStatus(effectiveService) === 'updatable'
              ? '可更新'
              : '暂无候选版本'
  const effectiveBannerClass = service != null ? bannerClass : 'svcBanner svcBannerMuted'
  const effectiveDotClass = service != null ? dotClass : 'svcBannerDot'
  const effectiveBannerDetail =
    service != null ? bannerDetail : '当前展示本地快照；恢复联网后刷新可获取最新候选与实时状态。'

  const renderOverviewSection = () => (
    <div className="svcDetailSectionStack">
      <RecentUpdateRecords jobs={recentUpdateJobs} />
    </div>
  )

  const renderMonitoringSection = () => (
    <div className="svcDetailSectionStack">
      <ServiceResourcePanel
        initialSnapshot={readonlyUi ? monitoringSnapshot ?? snapshotPayload?.monitoring ?? null : undefined}
        readonly={readonlyUi}
        serviceId={effectiveService.id}
      />
    </div>
  )

  const renderBackupSection = () => (
    <div className="svcDetailSectionStack">
      <div className="card serviceBackupSummaryCard" data-service-detail-section-card="backup-summary">
        <div className="serviceBackupSummaryHead">
          <div>
            <div className="title">备份设置</div>
            <div className="muted">当前服务的备份策略、存储位置与默认保留摘要。</div>
          </div>
          <div data-service-detail-action="open-backup-settings">
            <Button
              disabled={settingsBusy || readonlyUi}
              onClick={() => {
                setServiceBackupTargetsDraft(createBackupTargetsDraft(effectiveBackupTargets))
                setBackupSettingsDrawerOpen(true)
              }}
            >
              编辑备份设置
            </Button>
          </div>
        </div>
        <div className="serviceBackupMetaCard">
          {effectiveBackupTargets?.storage ? (
            <>
              <div className="serviceBackupMetaSummary">{formatBackupRetentionSummary(effectiveBackupTargets.storage)}</div>
              <div className="serviceBackupMetaGrid">
                <div>
                  <div className="muted">目录</div>
                  <div className="mono">{effectiveBackupTargets.storage.baseDir}</div>
                </div>
                <div>
                  <div className="muted">产物</div>
                  <div className="mono">{effectiveBackupTargets.storage.artifactPattern}</div>
                </div>
                <div>
                  <div className="muted">压缩</div>
                  <div className="mono">{effectiveBackupTargets.storage.compression}</div>
                </div>
              </div>
            </>
          ) : (
            <div className="muted">加载备份说明中…</div>
          )}
        </div>
      </div>

      <div className="card" data-service-detail-section-card="backup-records">
        <div className="serviceBackupSummaryHead">
          <div>
            <div className="title">实际备份记录</div>
            <div className="muted">这里只显示当前服务实际产生过备份产物的记录。</div>
          </div>
        </div>
        <BackupRecordList records={effectiveBackupRecords} />
      </div>
    </div>
  )

  const renderLogsSection = () => (
    <div className="svcDetailSectionStack">
      {readonlyUi ? (
        <ServiceDetailReadonlyBlocked
          detail="日志流不做持久化。恢复联网后才能重新建立实时日志连接。"
          title="当前离线，日志页需要联网。"
        />
      ) : (
        <ServiceLogsPanel serviceId={effectiveService.id} />
      )}
    </div>
  )

  const renderSettingsSection = () => (
    <div className="svcDetailSectionStack">
      {readonlyUi || !settings ? (
        <ServiceDetailReadonlyBlocked
          detail="设置页包含敏感配置与写操作，不会持久化到本地；恢复联网后才可编辑。"
          title="当前离线，设置页需要联网。"
        />
      ) : null}
      {!readonlyUi && settings ? (
        <>
          <div data-service-detail-section-card="auto-policy">
            <div data-service-detail-action="open-auto-policy">
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

      <div className="card serviceSafeguardCard">
        <div>
          <div className="title">部署 tag</div>
          <div className="muted">直接写回原始 Compose 文件里的镜像 tag，不自动执行 compose up。</div>
        </div>
          <div className="serviceTagCardActions">
            <div className="chipStatic">
              当前 <Mono>{effectiveService.image.tag || '-'}</Mono>
            </div>
            <Button disabled={settingsBusy} onClick={() => setTagDrawerOpen(true)}>
              编辑 tag
          </Button>
        </div>
      </div>

      <div className="card serviceSafeguardCard">
        <div>
          <div className="title">服务保护设置</div>
          <div className="muted">失败回滚与代码仓库单独配置；备份目标已经迁到独立的备份分区。</div>
        </div>
        <div data-service-detail-action="open-service-settings">
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
      </div>

      <div className="card" data-service-detail-section-card="ignore-rules">
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

      <div className="card" data-service-detail-section-card="webhook">
        <div className="title">Webhook 触发（服务级）</div>
        <div className="muted">用于外部系统触发：更新此服务 / 更新 compose / 更新全部</div>

        <div className="webhookRow">
          <div className="label">POST</div>
          <div className="mono">/api/v1/update/service/{effectiveService.name}</div>
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

      <div className="card svcDangerZoneCard" data-service-detail-section-card="danger-zone">
        <div className="svcDangerZoneHead">
          <div>
            <div className="title">维护动作</div>
            <div className="muted">低频或高影响动作下沉到设置页，避免服务详情首屏过于拥挤。</div>
          </div>
        </div>
        <div className="svcDangerZoneActions">{dangerousActions}</div>
      </div>
        </>
      ) : null}
    </div>
  )

  const renderSection = () => {
    if (sectionValue === 'monitoring') return renderMonitoringSection()
    if (sectionValue === 'backup') return renderBackupSection()
    if (sectionValue === 'logs') return renderLogsSection()
    if (sectionValue === 'settings') return renderSettingsSection()
    return renderOverviewSection()
  }

  return (
    <div className="page">
      {snapshotActive ? (
        <ReadonlySnapshotNotice
          tone={!isOnline ? 'warn' : 'info'}
          title={!isOnline ? '当前离线，显示已缓存的服务详情数据。' : '先显示已缓存的服务详情数据，后台会继续刷新。'}
          detail="仅保留概览、监控摘要与备份摘要；日志和设置会继续要求联网。"
          fetchedAt={snapshotFetchedAt}
          actionLabel="重试刷新"
          actionDisabled={!isOnline || busy}
          onAction={() => void requestRefresh()}
        />
      ) : !isOnline ? (
        <ReadonlySnapshotNotice
          tone="warn"
          title="当前离线。"
          detail="仅在存在可用缓存时显示只读内容；日志与设置需要联网。"
        />
      ) : null}
      <div className="svcTitleRow">
        <div className="svcTitleMain">
          <div className="svcTitleNameRow">
            <div className="svcTitleName">
              服务: <Mono>{effectiveService.name}</Mono>
            </div>
            <Pill tone="muted">{effectiveStack.name}</Pill>
          </div>
          {(() => {
            const img = splitImageRef(effectiveService.image.ref)
            const dn = splitImageNameForDisplay(img.name, effectiveService.image.tag)
            return (
              <div className="cellTwoLine">
                <div
                  className="mono monoPrimary monoSplit imageLinkRow"
                  title={dn.suffix ? `${dn.base}${dn.suffix}` : dn.base}
                >
                  <span className="monoSplitBase">{dn.base}</span>
                  <ImageLinkIcons imageRef={effectiveService.image.ref} repoUrl={visibleRepoUrl} />
                </div>
                <div className="mono monoSecondary">{img.registry}</div>
              </div>
            )
          })()}
          <div className="muted">
            id <Mono>{effectiveService.id}</Mono> · stack <Mono>{effectiveStack.id}</Mono>
          </div>
        </div>
      </div>

      <div className="svcDetailContextSummary" data-service-detail-context="status-summary">
        <div className={effectiveBannerClass}>
          <div className="svcBannerTitleRow">
            <span className={effectiveDotClass} />
            <div className="svcBannerTitle">{effectiveBannerTitle}</div>
            <div style={{ marginLeft: 'auto' }}>
              <Pill tone={tone}>{svcBadge(effectiveService)}</Pill>
            </div>
          </div>
          <div className="svcBannerDetail">{effectiveBannerDetail}</div>
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
      </div>

      <div className="svcDetailTabsShell" data-service-detail-tabs-shell="true">
        <Tabs
          onValueChange={(value) => {
            const nextSection = value as 'overview' | 'monitoring' | 'backup' | 'logs' | 'settings'
            navigate({
              name: 'service',
              stackId: props.stackId,
              serviceId: props.serviceId,
              section: nextSection,
            })
          }}
          value={sectionValue}
        >
          <TabsList className="svcDetailTabsList" aria-label="服务详情分区">
            <TabsTrigger
              className={sectionValue === 'overview' ? 'svcDetailTab active' : 'svcDetailTab'}
              data-service-detail-tab="overview"
              value="overview"
            >
              概览
            </TabsTrigger>
            <TabsTrigger
              className={sectionValue === 'monitoring' ? 'svcDetailTab active' : 'svcDetailTab'}
              data-service-detail-tab="monitoring"
              value="monitoring"
            >
              监控
            </TabsTrigger>
            <TabsTrigger
              className={sectionValue === 'backup' ? 'svcDetailTab active' : 'svcDetailTab'}
              data-service-detail-tab="backup"
              value="backup"
            >
              备份
            </TabsTrigger>
            <TabsTrigger
              className={sectionValue === 'logs' ? 'svcDetailTab active' : 'svcDetailTab'}
              data-service-detail-tab="logs"
              value="logs"
            >
              日志
            </TabsTrigger>
            <TabsTrigger
              className={sectionValue === 'settings' ? 'svcDetailTab active' : 'svcDetailTab'}
              data-service-detail-tab="settings"
              value="settings"
            >
              设置
            </TabsTrigger>
          </TabsList>
        </Tabs>
      </div>

      {isDockrevService(effectiveService) && supervisorState.status === 'offline' ? (
        <div className="muted" style={{ marginTop: 10 }}>
          supervisor offline · {supervisorErrorAt ?? '-'}
        </div>
      ) : null}

      {renderSection()}

      {settings ? (
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
          previewServiceId={effectiveService.id}
          scope="service"
          stackPolicy={stackSettings?.autoUpdatePolicy ?? null}
        />
      ) : null}

      <ResponsiveSettingsDrawer
        description="写回原始 Compose 文件里的镜像 tag；保存后不会自动执行 compose up。"
        onOpenChange={setTagDrawerOpen}
        open={tagDrawerOpen}
        title="部署 tag"
      >
        <div className="settingsDrawerSection">
          <ServiceComposeTagField
            busy={settingsBusy}
            currentTag={effectiveService.image.tag}
            onError={setError}
            onSaved={requestRefresh}
            serviceId={props.serviceId}
          />
        </div>
      </ResponsiveSettingsDrawer>

      <ResponsiveSettingsDrawer
        description="配置失败回滚与代码仓库。"
        onOpenChange={(open) => {
          setServiceSettingsDrawerOpen(open)
          if (!open) {
            setServiceSettingsDraft(null)
          }
        }}
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
                      ...serviceProtectionDraft,
                      autoUpdatePolicy: settings?.autoUpdatePolicy,
                      repoUrl: (serviceProtectionDraft.repoUrl ?? '').trim() || null,
                    })
                    await requestRefresh()
                    setServiceSettingsDrawerOpen(false)
                    setServiceSettingsDraft(null)
                  } catch (e: unknown) {
                    setError(errorMessage(e))
                  } finally {
                    setBusy(false)
                  }
                })()
              }}
            >
              保存服务保护设置
            </Button>
          </div>
        </div>
      </ResponsiveSettingsDrawer>

      <ResponsiveSettingsDrawer
        description="配置当前服务升级前的备份 targets 与默认存储说明。"
        onOpenChange={setBackupSettingsDrawerOpen}
        open={backupSettingsDrawerOpen}
        title="备份设置"
      >
        <div className="settingsDrawerSection">
          <div className="sectionTitle">备份项（服务级）</div>
          <div className="muted">每个 target 单独选择一个策略；数字表示关联服务数，停机备份会一起协调这些服务。</div>

          <div className="serviceBackupPicker">
            <div className="serviceBackupMetaCard">
              <div className="label">备份说明</div>
              {effectiveBackupTargets?.storage ? (
                <>
                  <div className="serviceBackupMetaSummary">{formatBackupRetentionSummary(effectiveBackupTargets.storage)}</div>
                  <div className="serviceBackupMetaGrid">
                    <div>
                      <div className="muted">目录</div>
                      <div className="mono">{effectiveBackupTargets.storage.baseDir}</div>
                    </div>
                    <div>
                      <div className="muted">产物</div>
                      <div className="mono">{effectiveBackupTargets.storage.artifactPattern}</div>
                    </div>
                    <div>
                      <div className="muted">压缩</div>
                      <div className="mono">{effectiveBackupTargets.storage.compression}</div>
                    </div>
                  </div>
                </>
              ) : (
                <div className="muted">加载备份说明中…</div>
              )}
            </div>

            {serviceBackupTargetsDraft.volumeNames.length === 0 && serviceBackupTargetsDraft.bindPaths.length === 0 ? (
              <div className="serviceBackupEmptyState">
                当前服务在 Compose 中未发现可备份 volume 或 bind path。
              </div>
            ) : (
              <>
                <div className="serviceBackupGroup">
                  <div className="label">Volumes</div>
                  {serviceBackupTargetsDraft.volumeNames.length === 0 ? (
                    <div className="muted">当前服务未声明可备份 volume。</div>
                  ) : null}
                  {serviceBackupTargetsDraft.volumeNames.map((item) => (
                    <div key={item.key} className="serviceBackupRow">
                      <div className="serviceBackupRowHead">
                        <div>
                          <div className="mono">{item.key}</div>
                          <div className="muted">{backupRelationshipLabel(item)}</div>
                        </div>
                        <div className="serviceBackupCountBadge">{item.relatedServiceCount}</div>
                      </div>
                      <div className="serviceBackupRowControls">
                        <div className="muted">{backupPolicyHint(item)}</div>
                        <BackupPolicySegmentedControl
                          disabled={settingsBusy}
                          itemLabel={item.key}
                          onChange={(value) => {
                            setServiceBackupTargetsDraft((prev) => ({
                              ...prev,
                              volumeNames: prev.volumeNames.map((entry) =>
                                entry.key === item.key ? { ...entry, policy: value } : entry,
                              ),
                            }))
                          }}
                          value={item.policy}
                        />
                      </div>
                    </div>
                  ))}
                </div>

                <div className="serviceBackupGroup">
                  <div className="label">Bind paths</div>
                  {serviceBackupTargetsDraft.bindPaths.length === 0 ? (
                    <div className="muted">当前服务未声明可备份 bind path。</div>
                  ) : null}
                  {serviceBackupTargetsDraft.bindPaths.map((item) => (
                    <div key={item.key} className="serviceBackupRow">
                      <div className="serviceBackupRowHead">
                        <div>
                          <div className="mono">{item.key}</div>
                          <div className="muted">{backupRelationshipLabel(item)}</div>
                        </div>
                        <div className="serviceBackupCountBadge">{item.relatedServiceCount}</div>
                      </div>
                      <div className="serviceBackupRowControls">
                        <div className="muted">{backupPolicyHint(item)}</div>
                        <BackupPolicySegmentedControl
                          disabled={settingsBusy}
                          itemLabel={item.key}
                          onChange={(value) => {
                            setServiceBackupTargetsDraft((prev) => ({
                              ...prev,
                              bindPaths: prev.bindPaths.map((entry) =>
                                entry.key === item.key ? { ...entry, policy: value } : entry,
                              ),
                            }))
                          }}
                          value={item.policy}
                        />
                      </div>
                    </div>
                  ))}
                </div>
              </>
            )}

            <div className="formActions">
              <Button
                variant="primary"
                disabled={settingsBusy}
                onClick={() => {
                  void (async () => {
                    setBusy(true)
                    setError(null)
                    try {
                      await putServiceBackupTargets(
                        props.serviceId,
                        backupTargetRequestFromDraft(serviceBackupTargetsDraft),
                      )
                      await requestRefresh()
                    } catch (e: unknown) {
                      setError(errorMessage(e))
                    } finally {
                      setBusy(false)
                    }
                  })()
                }}
              >
                保存备份设置
              </Button>
            </div>
          </div>
        </div>
      </ResponsiveSettingsDrawer>

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
