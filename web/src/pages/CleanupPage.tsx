import { startTransition, useCallback, useEffect, useRef, useMemo, useState, type ReactNode } from 'react'
import { Box, HardDrive, Layers3, Package } from 'lucide-react'
import {
  ApiError,
  applyCleanups,
  cleanupScanRunEventsUrl,
  scanCleanups,
  startCleanupScanRun,
  type CleanupFingerprintMismatchError,
  type CleanupPreset,
  type CleanupResourceItem,
  type CleanupResourceKind,
  type CleanupScanRequest,
  type CleanupScanResponse,
  type CleanupScanRunEvent,
  type CleanupScanStage,
  type CleanupScope,
  type CleanupStackGroup,
} from '../api'
import { useManagementEventBatch } from '../managementEvents'
import { deployCheckOperationImpactReason, useDeployCheckReport } from '../deployCheck'
import { CleanupContextNavigation } from '../components/PageContextNavigation'
import { CleanupConfirmDialog, type CleanupDialogPhase } from './CleanupConfirmDialog'
import { navigate } from '../routes'
import { Button, Mono, Pill, RefreshIcon, SectionTitle, Tabs, TabsList, TabsTrigger, TrashIcon } from '../ui'
import {
  CLEANUP_SNAPSHOT_MAX_AGE_MS,
  KIND_LABEL,
  aggregateStackResources,
  buildUsageCards,
  cleanupResourceKey,
  cleanupResourceKeys,
  countUnknownResources,
  countVisibleResources,
  cleanupFailureBackoffMs,
  isSnapshotFresh,
  nextCleanupRefreshDelayMs,
  projectResponseForTarget,
  shouldStopCleanupRefresh,
  snapshotAgeNow,
  countRenderableResources,
  flattenAllResources,
  formatBytes,
  formatDiskUsage,
  formatEstimate,
  formatPercent,
  formatUnknownCount,
  itemHasUnknownSize,
  mergeCleanupResponses,
  kindSummary,
  projectResponseForPreset,
  filterCleanupResponseForView,
  staleBucketsForResponse,
  toErrorMessage,
  type CleanupUsageBucket,
  type CleanupUsageCard,
} from './cleanupPageModel'

const PRESET_META: Array<{
  key: CleanupPreset
  label: string
  description: string
}> = [
  {
    key: 'conservative',
    label: '保守',
    description: '仅清理已停止容器、dangling 镜像与未使用网络。',
  },
  {
    key: 'balanced',
    label: '均衡',
    description: '额外纳入项目旧镜像与全局 builder cache，适合作为默认策略。',
  },
  {
    key: 'project_deep_clean',
    label: '项目深清',
    description: '继续向下清理项目卷，适合做一次较彻底的空间回收。',
  },
  {
    key: 'aggressive',
    label: '激进',
    description: '连全局未归属镜像与卷也纳入候选，只建议明确核对后执行。',
  },
]

const CLEANUP_CONFIRM_POLL_FALLBACK_MS = 800
const CLEANUP_REFRESH_ERROR = '刷新失败，请重试。'
const CLEANUP_APPLY_ERROR = '清理失败，请重试。'

const CLEANUP_USAGE_CARD_META: Array<{
  key: CleanupUsageBucket
  icon: typeof Box
  toneClassName: string
}> = [
  {
    key: 'container',
    icon: Box,
    toneClassName: 'cleanupUsageCardContainer',
  },
  {
    key: 'image',
    icon: Package,
    toneClassName: 'cleanupUsageCardImage',
  },
  {
    key: 'volume',
    icon: HardDrive,
    toneClassName: 'cleanupUsageCardVolume',
  },
  {
    key: 'other',
    icon: Layers3,
    toneClassName: 'cleanupUsageCardOther',
  },
]

type PageScanOptions = { silent?: boolean; source?: 'initial' | 'manual' | 'foreground' | 'dialog' }

type CleanupDialogState = {
  open: boolean
  target: CleanupActionTarget
  phase: CleanupDialogPhase
  response: CleanupScanResponse | null
  fingerprint: string | null
  receivedAt: number
  stale: boolean
  stage: CleanupScanStage | null
  error: string | null
}

type CleanupActionTarget =
  | { actionKey: string; scope: 'all'; title: string; targetLabel: string }
  | { actionKey: string; scope: 'stack'; stackId: string; title: string; targetLabel: string }
  | {
      actionKey: string
      scope: 'service'
      stackId: string
      serviceId: string
      title: string
      targetLabel: string
    }

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function formatShort(ts?: string | null): string {
  if (!ts) return '-'
  const date = new Date(ts)
  if (Number.isNaN(date.valueOf())) return ts
  return date.toLocaleString('zh-CN', { hour12: false })
}

function parseFingerprintMismatch(details: unknown): CleanupFingerprintMismatchError | null {
  if (!isRecord(details) || !('latest' in details) || !isRecord(details.latest)) return null
  return details as CleanupFingerprintMismatchError
}

function presetLabel(preset: CleanupPreset): string {
  return PRESET_META.find((item) => item.key === preset)?.label ?? preset
}

function scopeLabel(scope: CleanupScope): string {
  if (scope === 'all') return '全部'
  if (scope === 'stack') return 'Stack'
  return '服务'
}

function actionHint(scope: 'all' | 'stack' | 'service'): string {
  if (scope === 'all') {
    return '清理当前规则下的全部候选；不会停止正在运行的容器。'
  }
  if (scope === 'stack') {
    return '清理这个 stack 当前规则下的全部候选，不只当前这一行；不会停止正在运行的容器。'
  }
  return '清理这个服务当前规则下的全部候选，不只当前这一行；不会停止正在运行的容器。'
}

function confirmSafetyNote(scope: CleanupScope): string {
  if (scope === 'service') {
    return '会清理该服务当前规则下的全部候选，不只是一行。正在运行的容器不会被停止；执行时若资源已经重新被占用，会自动跳过。'
  }
  if (scope === 'stack') {
    return '会清理该 stack 当前规则下的全部候选，包括 stack orphan。正在运行的容器不会被停止；执行时若资源已经重新被占用，会自动跳过。'
  }
  return '会清理当前规则下的全部候选。正在运行的容器不会被停止；执行时若资源已经重新被占用，会自动跳过。'
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

function resourceKindClassName(kind: CleanupResourceKind): string {
  switch (kind) {
    case 'image':
      return 'cleanupResourceKind cleanupResourceKindImage'
    case 'container':
      return 'cleanupResourceKind cleanupResourceKindContainer'
    case 'network':
      return 'cleanupResourceKind cleanupResourceKindNetwork'
    case 'volume':
      return 'cleanupResourceKind cleanupResourceKindVolume'
    case 'builder_cache':
      return 'cleanupResourceKind cleanupResourceKindBuilderCache'
  }
}

function CleanupSummaryCell(props: { resources: CleanupResourceItem[]; hint?: string }) {
  return (
    <div className="cleanupCellSummary">
      <div className="mono monoPrimary">{kindSummary(props.resources)}</div>
      {props.hint ? <div className="muted cleanupCellHint">{props.hint}</div> : null}
    </div>
  )
}

function CleanupEstimateCell(props: { bytes: number; hasUnknown?: boolean; count?: number }) {
  return (
    <div className="cleanupCellEstimate">
      <div className="mono monoPrimary">{formatEstimate(props.bytes, props.hasUnknown)}</div>
      {props.count != null ? <div className="muted cleanupCellHint">{props.count} 项</div> : null}
    </div>
  )
}

function CleanupUsageCardView(props: { card: CleanupUsageCard; refreshing?: boolean }) {
  const meta = CLEANUP_USAGE_CARD_META.find((item) => item.key === props.card.key) ?? CLEANUP_USAGE_CARD_META[3]
  const Icon = meta.icon
  const knownOnly = props.card.bytes > 0 ? formatBytes(props.card.bytes) : '0 B'
  const knownShareLabel = `已知候选占比 ${formatPercent(props.card.share)}`
  const barWidth = props.card.bytes > 0 && props.card.share > 0 ? Math.max(2, Math.round(props.card.share * 100)) : 0

  return (
    <article
      className={`cleanupUsageCard ${meta.toneClassName}${props.card.unknownCount > 0 ? ' cleanupUsageCardUnknown' : ''}${props.refreshing ? ' cleanupStaleLoading' : ''}`}
      data-refreshing={props.refreshing ? 'true' : undefined}
    >
      <div className="cleanupUsageCardHead">
        <div className="cleanupUsageCardIconWrap">
          <Icon aria-hidden="true" className="cleanupUsageCardIcon" size={18} strokeWidth={2} />
        </div>
        <div className="cleanupUsageCardHeading">
          <div className="cleanupUsageCardLabel">{props.card.label}</div>
          <div className="cleanupUsageCardDescription">{props.card.description}</div>
        </div>
        <Pill tone="muted">{props.card.count} 项</Pill>
      </div>

      <div className="cleanupUsageCardValue">{formatEstimate(props.card.bytes, props.card.unknownCount > 0)}</div>

      <div className="cleanupUsageCardMeta">
        <span>已识别 {knownOnly}</span>
        <span>{props.card.unknownCount > 0 ? `含 ${formatUnknownCount(props.card.unknownCount)}` : knownShareLabel}</span>
      </div>

      <div aria-hidden="true" className="cleanupUsageCardBar">
        <span style={{ width: `${barWidth}%` }} />
      </div>
    </article>
  )
}

type CleanupFlatRow = {
  key: string
  ownerLabel: string
  ownerDetail?: string
  ownerTone: 'muted' | 'warn'
  serviceId?: string
  serviceName?: string
  actionScope: 'stack' | 'service' | 'none'
  resource: CleanupResourceItem
}

function flattenStackRows(stack: CleanupStackGroup): CleanupFlatRow[] {
  const rows: CleanupFlatRow[] = []
  stack.stackOrphans.forEach((resource) => {
    rows.push({
      key: `orphan:${stack.stackId}:${resource.resourceId}`,
      ownerLabel: 'Stack 未归属',
      ownerDetail: '未归到单个服务',
      ownerTone: 'warn',
      actionScope: 'stack',
      resource,
    })
  })
  stack.services.forEach((service) => {
    service.resources.forEach((resource) => {
      rows.push({
        key: `service:${stack.stackId}:${service.serviceId}:${resource.resourceId}`,
        ownerLabel: service.serviceName,
        ownerTone: 'muted',
        serviceId: service.serviceId,
        serviceName: service.serviceName,
        actionScope: 'service',
        resource,
      })
    })
  })
  return rows
}

function flattenUnownedRows(response: CleanupScanResponse): CleanupFlatRow[] {
  return (response.unownedGroup?.resources ?? []).map((resource) => ({
    key: `unowned:${resource.resourceId}`,
    ownerLabel: '未归属资源',
    ownerDetail: '不属于受管 stack',
    ownerTone: 'warn',
    actionScope: 'none',
    resource,
  }))
}

function CleanupResponseView(props: {
  response: CleanupScanResponse
  compact?: boolean
  busyActionKey?: string | null
  staleResourceKeys?: Set<string>
  scanBusy?: boolean
  onStackAction?: (stack: CleanupStackGroup) => void
  onServiceAction?: (stack: CleanupStackGroup, serviceId: string, serviceName: string) => void
  deployCheckReport?: import('../api').DeployCheckReportResponse | null
}) {
  if (countRenderableResources(props.response) === 0) {
    return <div className="cleanupEmptyState">当前规则下没有可清理资源。</div>
  }

  return (
    <div className={props.compact ? 'table cleanupTable cleanupTableCompact' : 'table cleanupTable'}>
      {!props.compact ? (
        <div className="tableHeader cleanupTableHeader">
          <div>可清理内容</div>
          <div>归属</div>
          <div>可清理原因</div>
          <div>预计释放</div>
          <div>操作</div>
        </div>
      ) : null}

      {props.response.stackGroups.map((stack) => {
        const stackCleanupReason = deployCheckOperationImpactReason(props.deployCheckReport ?? null, 'cleanup', stack.stackId)
        return (
        <section key={stack.stackId} className="tableGroup cleanupTableGroup">
          <div
            className={`groupHead cleanupGroupHead${
              aggregateStackResources(stack).some((resource) => props.staleResourceKeys?.has(cleanupResourceKey(resource)))
                ? ' cleanupStaleLoading'
                : ''
            }`}
          >
            <div className="cellService cellServiceGroup">
              <StackIcon variant="expanded" />
              <div className="groupTitle">{stack.stackName}</div>
              <Pill tone="muted">Stack</Pill>
            </div>
            <div className="groupMeta">服务 {stack.services.length} · orphan {stack.stackOrphans.length}</div>
            <CleanupSummaryCell resources={aggregateStackResources(stack)} />
            <CleanupEstimateCell
              bytes={stack.estimatedReclaimableBytes}
              count={aggregateStackResources(stack).length}
              hasUnknown={stack.hasUnknownSize}
            />
            {props.onStackAction ? (
              <div
                className="actionCell"
                onClick={(event) => event.stopPropagation()}
                onKeyDown={(event) => event.stopPropagation()}
              >
                <Button
                  disabled={props.scanBusy || Boolean(stackCleanupReason) || stack.estimatedReclaimableBytes <= 0 && stack.hasUnknownSize !== true}
                  hint={stackCleanupReason ?? actionHint('stack')}
                  loading={props.busyActionKey === `stack:${stack.stackId}`}
                  onClick={() => props.onStackAction?.(stack)}
                  variant="ghost"
                >
                  清理此 stack
                </Button>
              </div>
            ) : (
              <div />
            )}
          </div>

          {flattenStackRows(stack).map((row) => (
            <div
              key={row.key}
              className={`${row.ownerTone === 'warn' ? 'rowLine cleanupRowLine cleanupRowLineMuted' : 'rowLine cleanupRowLine'}${
                props.staleResourceKeys?.has(cleanupResourceKey(row.resource)) ? ' cleanupStaleLoading' : ''
              }`}
            >
              <div className="cellService">
                <span className={row.ownerTone === 'warn' ? 'svcBullet cleanupSvcBulletWarn' : 'svcBullet'} aria-hidden="true" />
                <span className="svcName">{row.resource.label}</span>
              </div>
              <div className="cellTwoLine cleanupOwnerCell">
                <div className="mono monoPrimary">{row.ownerLabel || ' '}</div>
                {row.ownerDetail ? <div className="muted cleanupCellHint">{row.ownerDetail}</div> : null}
              </div>
              <div className="cellTwoLine cleanupReasonCell">
                <div>
                  <span className={resourceKindClassName(row.resource.kind)}>{KIND_LABEL[row.resource.kind]}</span>
                </div>
                <div className="cleanupReasonText">{row.resource.reason}</div>
              </div>
              <CleanupEstimateCell
                bytes={row.resource.estimatedReclaimableBytes ?? 0}
                count={1}
                hasUnknown={itemHasUnknownSize(row.resource)}
              />
              {row.actionScope === 'stack' && props.onStackAction ? (
                <div className="actionCell">
                  <Button
                    disabled={props.scanBusy || Boolean(stackCleanupReason) || stack.estimatedReclaimableBytes <= 0 && stack.hasUnknownSize !== true}
                    hint={stackCleanupReason ?? actionHint('stack')}
                  loading={props.busyActionKey === `stack:${stack.stackId}`}
                  onClick={() => props.onStackAction?.(stack)}
                  variant="ghost"
                >
                    清理
                  </Button>
                </div>
              ) : row.actionScope === 'service' && props.onServiceAction && row.serviceId && row.serviceName ? (
                <div className="actionCell">
                  <Button
                    disabled={props.scanBusy || Boolean(deployCheckOperationImpactReason(props.deployCheckReport ?? null, 'cleanup', [stack.stackId, row.serviceId]))}
                    hint={deployCheckOperationImpactReason(props.deployCheckReport ?? null, 'cleanup', [stack.stackId, row.serviceId]) ?? actionHint('service')}
                    loading={props.busyActionKey === `service:${stack.stackId}:${row.serviceId}`}
                    onClick={() => props.onServiceAction?.(stack, row.serviceId!, row.serviceName!)}
                    variant="ghost"
                  >
                    清理
                  </Button>
                </div>
              ) : (
                <div className="actionCell" />
              )}
            </div>
          ))}
        </section>
        )
      })}

      {props.response.unownedGroup?.resources.length ? (
        <section className="tableGroup cleanupTableGroup cleanupTableGroupUnowned">
          <div
            className={`groupHead cleanupGroupHead cleanupGroupHeadWarn${
              props.response.unownedGroup.resources.some((resource) =>
                props.staleResourceKeys?.has(cleanupResourceKey(resource)),
              )
                ? ' cleanupStaleLoading'
                : ''
            }`}
          >
            <div className="cellService cellServiceGroup">
              <StackIcon variant="expanded" />
              <div className="groupTitle">{props.response.unownedGroup.title}</div>
              <Pill tone="warn">仅全部</Pill>
            </div>
            <div className="groupMeta">全局未归属候选</div>
            <CleanupSummaryCell resources={props.response.unownedGroup.resources} />
            <CleanupEstimateCell
              bytes={props.response.unownedGroup.estimatedReclaimableBytes}
              count={props.response.unownedGroup.resources.length}
              hasUnknown={props.response.unownedGroup.hasUnknownSize}
            />
            <div />
          </div>

          {flattenUnownedRows(props.response).map((row) => (
            <div
              key={row.key}
              className={`rowLine cleanupRowLine cleanupRowLineMuted${
                props.staleResourceKeys?.has(cleanupResourceKey(row.resource)) ? ' cleanupStaleLoading' : ''
              }`}
            >
              <div className="cellService">
                <span className="svcBullet cleanupSvcBulletWarn" aria-hidden="true" />
                <span className="svcName">{row.resource.label}</span>
              </div>
              <div className="cellTwoLine cleanupOwnerCell">
                <div className="mono monoPrimary">{row.ownerLabel || ' '}</div>
                {row.ownerDetail ? <div className="muted cleanupCellHint">{row.ownerDetail}</div> : null}
              </div>
              <div className="cellTwoLine cleanupReasonCell">
                <div>
                  <span className={resourceKindClassName(row.resource.kind)}>{KIND_LABEL[row.resource.kind]}</span>
                </div>
                <div className="cleanupReasonText">{row.resource.reason}</div>
              </div>
              <CleanupEstimateCell
                bytes={row.resource.estimatedReclaimableBytes ?? 0}
                count={1}
                hasUnknown={itemHasUnknownSize(row.resource)}
              />
              <div className="actionCell" />
            </div>
          ))}
        </section>
      ) : null}
    </div>
  )
}

export function CleanupPage(props: {
  onLastScanHint?: (lastScan?: string) => void
  onTopActions: (node: ReactNode) => void
  onContextNavigation?: (node: ReactNode) => void
}) {
  const { onLastScanHint, onTopActions, onContextNavigation } = props
  const deployCheckReport = useDeployCheckReport()
  const activeScanIdRef = useRef<string | null>(null)
  const activeScanEventsRef = useRef<EventSource | null>(null)
  const [activePreset, setActivePreset] = useState<CleanupPreset>('balanced')
  const [pageScan, setPageScan] = useState<CleanupScanResponse | null>(null)
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [scanPhase, setScanPhase] = useState<'idle' | 'scanning' | 'ready' | 'failed'>('idle')
  const [staleResourceKeys, setStaleResourceKeys] = useState<Set<string>>(() => new Set())
  const [pageError, setPageError] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [busyActionKey, setBusyActionKey] = useState<string | null>(null)
  const [dialog, setDialog] = useState<CleanupDialogState | null>(null)
  const [viewScope, setViewScope] = useState<CleanupScope>('all')
  const [viewResourceKinds, setViewResourceKinds] = useState<CleanupResourceKind[]>([])
  const pageScanRequestVersionRef = useRef(0)
  const pageScanRef = useRef<CleanupScanResponse | null>(null)
  const pageScanReceivedAtRef = useRef(0)
  const activePresetRef = useRef<CleanupPreset>(activePreset)
  const refreshingRef = useRef(false)
  const dialogRef = useRef<CleanupDialogState | null>(null)
  const lastInteractionAtRef = useRef(Date.now())
  const lastScanDurationRef = useRef<number | null>(null)
  const scanStartedAtRef = useRef<number | null>(null)
  const scanSilentRef = useRef(false)
  const backoffAttemptRef = useRef(0)
  const backoffUntilRef = useRef(0)

  const setDialogState = useCallback((next: CleanupDialogState | null) => {
    dialogRef.current = next
    setDialog(next)
  }, [])

  const patchDialog = useCallback((patch: Partial<CleanupDialogState>) => {
    const current = dialogRef.current
    if (!current) return
    const next = { ...current, ...patch }
    dialogRef.current = next
    setDialog(next)
  }, [])

  const applyPageScan = useCallback((response: CleanupScanResponse) => {
    pageScanRef.current = response
    pageScanReceivedAtRef.current = Date.now()
    setPageScan(response)
  }, [])

  useEffect(() => {
    activePresetRef.current = activePreset
  }, [activePreset])

  useEffect(() => {
    refreshingRef.current = refreshing
  }, [refreshing])

  const projected = useMemo(
    () => (pageScan ? projectResponseForPreset(pageScan, activePreset) : null),
    [activePreset, pageScan],
  )
  const usageCards = useMemo(() => (pageScan ? buildUsageCards(pageScan) : []), [pageScan])
  const serverDiskUsage = useMemo(() => formatDiskUsage(pageScan?.serverDiskUsage), [pageScan?.serverDiskUsage])
  const pageUnknownCount = useMemo(() => (pageScan ? countUnknownResources(flattenAllResources(pageScan)) : 0), [pageScan])
  const projectedUnknownCount = useMemo(
    () => (projected ? countUnknownResources(flattenAllResources(projected)) : 0),
    [projected],
  )
  const activePresetMeta = useMemo(
    () => PRESET_META.find((item) => item.key === activePreset) ?? PRESET_META[1],
    [activePreset],
  )
  const initialScanPending = loading && !pageScan
  const staleUsageBuckets = useMemo(() => staleBucketsForResponse(pageScan, staleResourceKeys), [pageScan, staleResourceKeys])
  const pageSnapshotAge = pageScan
    ? snapshotAgeNow(pageScan, pageScanReceivedAtRef.current, Date.now())
    : null
  const pageSnapshotStale = pageScan != null && !isSnapshotFresh(pageSnapshotAge)
  const hasStaleResources = (staleResourceKeys.size > 0 && refreshing) || pageSnapshotStale
  const deployCheckCleanupReason = deployCheckOperationImpactReason(deployCheckReport, 'cleanup')

  const startPageScanRef = useRef<((options?: PageScanOptions) => void) | null>(null)

  useEffect(() => () => {
    pageScanRequestVersionRef.current += 1
    activeScanEventsRef.current?.close()
    activeScanEventsRef.current = null
  }, [])

  const scopedScanRequest = useCallback(
    (target: CleanupActionTarget, preset: CleanupPreset, reason: CleanupScanRequest['reason'], refresh: boolean): CleanupScanRequest => ({
      reason,
      refresh,
      preset,
      scope: target.scope,
      stackId: 'stackId' in target ? target.stackId : undefined,
      serviceId: 'serviceId' in target ? target.serviceId : undefined,
    }),
    [],
  )

  const finishDialogScan = useCallback(
    async (target: CleanupActionTarget) => {
      const current = dialogRef.current
      if (!current || current.target.actionKey !== target.actionKey) return
      try {
        const scoped = await scanCleanups(scopedScanRequest(target, activePresetRef.current, 'confirm', false))
        if (!dialogRef.current || dialogRef.current.target.actionKey !== target.actionKey) return
        if (scoped.status === 'ready' && scoped.confirmationFingerprint) {
          patchDialog({
            phase: countVisibleResources(scoped) > 0 ? 'confirm' : 'empty',
            response: scoped,
            fingerprint: scoped.confirmationFingerprint,
            receivedAt: Date.now(),
            stage: null,
          })
        } else {
          patchDialog({ phase: 'discovery' })
          startPageScanRef.current?.({ silent: false, source: 'dialog' })
        }
      } catch (error) {
        patchDialog({ phase: 'error', error: toErrorMessage(error), response: null })
      }
    },
    [patchDialog, scopedScanRequest],
  )

  const startPageScan = useCallback(
    (options?: PageScanOptions) => {
      const silent = Boolean(options?.silent)
      if (!silent) {
        backoffAttemptRef.current = 0
        backoffUntilRef.current = 0
      }
      if (activeScanEventsRef.current || refreshingRef.current) {
        if (!silent) scanSilentRef.current = false
        return
      }
      if (!pageScanRef.current) setLoading(true)
      const version = pageScanRequestVersionRef.current + 1
      pageScanRequestVersionRef.current = version
      scanSilentRef.current = silent
      scanStartedAtRef.current = Date.now()
      refreshingRef.current = true
      setRefreshing(true)
      setScanPhase('scanning')
      setPageError(null)
      setActionError(null)
      void (async () => {
        try {
          const started = await startCleanupScanRun({
            reason: 'page',
            refresh: true,
            preset: 'aggressive',
            scope: 'all',
          })
          if (pageScanRequestVersionRef.current !== version) return
          activeScanIdRef.current = started.scanId
          const source = new EventSource(cleanupScanRunEventsUrl(started.scanId), { withCredentials: true })
          activeScanEventsRef.current = source
          const closeSource = () => {
            source.close()
            if (activeScanEventsRef.current === source) activeScanEventsRef.current = null
            activeScanIdRef.current = null
          }
          const onScanEvent = (event: Event) => {
            if (!(event instanceof MessageEvent) || typeof event.data !== 'string') return
            let payload: CleanupScanRunEvent
            try {
              payload = JSON.parse(event.data) as CleanupScanRunEvent
            } catch {
              return
            }
            if (
              activeScanEventsRef.current !== source ||
              activeScanIdRef.current !== started.scanId ||
              payload.scanId !== started.scanId
            ) return

            if (payload.phase === 'scan_partial' && payload.response) {
              const partial = payload.response
              setScanPhase('scanning')
              const currentDialog = dialogRef.current
              if (currentDialog?.open && currentDialog.phase === 'discovery') {
                const projectedPartial = projectResponseForPreset(partial, activePresetRef.current)
                patchDialog({
                  response: projectResponseForTarget(projectedPartial, currentDialog.target),
                  stage: partial.scanStage ?? null,
                })
              }
              if (!scanSilentRef.current) {
                setPageScan((previous) => {
                  const merged = previous
                    ? mergeCleanupResponses(previous, partial)
                    : {
                        response: {
                          ...partial,
                          status: 'ready' as const,
                          refreshing: true,
                          confirmationFingerprint: null,
                        },
                        staleKeys: new Set<string>(),
                      }
                  setStaleResourceKeys(merged.staleKeys)
                  pageScanRef.current = merged.response
                  return merged.response
                })
                setLoading(false)
              }
              return
            }

            if (payload.phase === 'scan_ready' && payload.response) {
              const ready = payload.response
              lastScanDurationRef.current =
                scanStartedAtRef.current == null ? null : Date.now() - scanStartedAtRef.current
              applyPageScan(ready)
              setScanPhase('ready')
              setRefreshing(false)
              refreshingRef.current = false
              setLoading(false)
              setStaleResourceKeys(new Set())
              setPageError(null)
              setActionError(null)
              backoffAttemptRef.current = 0
              backoffUntilRef.current = 0
              onLastScanHint?.(ready.scannedAt ?? undefined)
              const currentDialog = dialogRef.current
              closeSource()
              if (currentDialog?.open && currentDialog.phase === 'discovery') {
                void finishDialogScan(currentDialog.target)
              }
              return
            }

            if (payload.phase === 'scan_failed') {
              setScanPhase('failed')
              setRefreshing(false)
              refreshingRef.current = false
              setLoading(false)
              const currentDialog = dialogRef.current
              if (currentDialog?.open && currentDialog.phase === 'discovery') {
                patchDialog({ phase: 'error', response: null, error: CLEANUP_REFRESH_ERROR })
              }
              if (scanSilentRef.current) {
                backoffAttemptRef.current += 1
                backoffUntilRef.current = Date.now() + cleanupFailureBackoffMs(backoffAttemptRef.current)
                setActionError(CLEANUP_REFRESH_ERROR)
              } else {
                if (!started.previousSnapshot) {
                  pageScanRef.current = null
                  setPageScan(null)
                }
                setPageError(CLEANUP_REFRESH_ERROR)
                setStaleResourceKeys(new Set())
              }
              closeSource()
            }
          }
          source.addEventListener('scan_partial', onScanEvent)
          source.addEventListener('scan_ready', onScanEvent)
          source.addEventListener('scan_failed', onScanEvent)

          if (started.previousSnapshot) {
            if (!pageScanRef.current) applyPageScan(started.previousSnapshot)
            if (!silent) setStaleResourceKeys(cleanupResourceKeys(started.previousSnapshot))
            onLastScanHint?.(started.previousSnapshot.scannedAt ?? undefined)
            setLoading(false)
          } else if (!pageScanRef.current) {
            setStaleResourceKeys(new Set())
          }
        } catch (error) {
          if (pageScanRequestVersionRef.current !== version) return
          const message = error instanceof ApiError && error.status >= 500 ? CLEANUP_REFRESH_ERROR : toErrorMessage(error)
          setRefreshing(false)
          refreshingRef.current = false
          setScanPhase('failed')
              setLoading(false)
          if (silent) {
            backoffAttemptRef.current += 1
            backoffUntilRef.current = Date.now() + cleanupFailureBackoffMs(backoffAttemptRef.current)
            setActionError(message)
          } else {
            setPageError(message)
            setStaleResourceKeys(new Set())
          }
          const currentDialog = dialogRef.current
          if (currentDialog?.open && currentDialog.phase === 'discovery') {
            patchDialog({ phase: 'error', response: null, error: message })
          }
        }
      })()
    },
    [applyPageScan, finishDialogScan, onLastScanHint, patchDialog],
  )
  startPageScanRef.current = startPageScan

  const pollPageSnapshot = useCallback(
    async (version: number) => {
      let delay = CLEANUP_CONFIRM_POLL_FALLBACK_MS
      while (pageScanRequestVersionRef.current === version) {
        await new Promise<void>((resolve) => window.setTimeout(resolve, delay))
        if (pageScanRequestVersionRef.current !== version) return
        try {
          const response = await scanCleanups({
            reason: 'page',
            refresh: false,
            preset: 'aggressive',
            scope: 'all',
          })
          if (pageScanRequestVersionRef.current !== version) return
          if (response.status === 'ready' && !response.refreshing) {
            applyPageScan(response)
            setScanPhase('ready')
            setRefreshing(false)
            refreshingRef.current = false
            setPageError(null)
            onLastScanHint?.(response.scannedAt ?? undefined)
            return
          }
          delay = Math.max(300, response.retryAfterMs ?? CLEANUP_CONFIRM_POLL_FALLBACK_MS)
        } catch (error) {
          if (pageScanRequestVersionRef.current !== version) return
          setPageError(error instanceof ApiError && error.status >= 500 ? CLEANUP_REFRESH_ERROR : toErrorMessage(error))
          setRefreshing(false)
          refreshingRef.current = false
          setScanPhase('failed')
          return
        }
      }
    },
    [applyPageScan, onLastScanHint],
  )

  useEffect(() => {
    onLastScanHint?.(undefined)
    const version = pageScanRequestVersionRef.current + 1
    pageScanRequestVersionRef.current = version
    void (async () => {
      try {
        const response = await scanCleanups({
          reason: 'page',
          refresh: false,
          preset: 'aggressive',
          scope: 'all',
        })
        if (pageScanRequestVersionRef.current !== version) return
        if (response.status === 'ready') {
          applyPageScan(response)
          setLoading(false)
          setScanPhase(response.refreshing ? 'scanning' : 'ready')
          setRefreshing(Boolean(response.refreshing))
          refreshingRef.current = Boolean(response.refreshing)
          setPageError(null)
          onLastScanHint?.(response.scannedAt ?? undefined)
          if (response.refreshing) void pollPageSnapshot(version)
          return
        }
        startPageScanRef.current?.({ silent: false, source: 'initial' })
      } catch (error) {
        if (pageScanRequestVersionRef.current !== version) return
        const message = error instanceof ApiError && error.status >= 500 ? CLEANUP_REFRESH_ERROR : toErrorMessage(error)
        setPageError(message)
        setLoading(false)
      }
    })()
  }, [applyPageScan, onLastScanHint, pollPageSnapshot])

  const readCompletedPageSnapshot = useCallback(async () => {
    const response = await scanCleanups({
      reason: 'page',
      refresh: false,
      preset: 'aggressive',
      scope: 'all',
    })
    if (response.status !== 'ready' || response.refreshing) return
    applyPageScan(response)
    setScanPhase('ready')
    setRefreshing(false)
    refreshingRef.current = false
    setLoading(false)
    setPageError(null)
    setStaleResourceKeys(new Set())
    onLastScanHint?.(response.scannedAt ?? undefined)
  }, [applyPageScan, onLastScanHint])

  useManagementEventBatch(({ events, resyncRequired }) => {
    if (refreshingRef.current || activeScanEventsRef.current) return
    if (resyncRequired) {
      void readCompletedPageSnapshot().catch((error: unknown) => {
        setPageError(error instanceof ApiError && error.status >= 500 ? CLEANUP_REFRESH_ERROR : toErrorMessage(error))
        setRefreshing(false)
        setLoading(false)
      })
    }
    const event = events.find((candidate) =>
      candidate.domain === 'cleanup' && candidate.entities.some((entity) => entity.entityType === 'scan' && entity.id === 'active'),
    )
    if (!event) return
    if (
      typeof event.summary.scanId === 'string' &&
      event.summary.scanId !== activeScanIdRef.current
    ) return
    if (event.summary.phase === 'ready') {
      void readCompletedPageSnapshot().catch((error: unknown) => {
        setPageError(error instanceof ApiError && error.status >= 500 ? CLEANUP_REFRESH_ERROR : toErrorMessage(error))
        setRefreshing(false)
        setLoading(false)
      })
      return
    }
    if (event.summary.phase === 'failed') {
      setPageError(CLEANUP_REFRESH_ERROR)
      setStaleResourceKeys(new Set())
      setRefreshing(false)
      setScanPhase('failed')
      setLoading(false)
    }
  })

  useEffect(() => {
    const onInteract = () => {
      lastInteractionAtRef.current = Date.now()
    }
    const interactionEvents = ['pointerdown', 'keydown', 'wheel', 'touchstart'] as const
    for (const type of interactionEvents) document.addEventListener(type, onInteract, { passive: true })
    const onVisibility = () => {
      if (document.visibilityState === 'visible') lastInteractionAtRef.current = Date.now()
    }
    document.addEventListener('visibilitychange', onVisibility)
    let timer: number | null = null
    const tick = () => {
      const now = Date.now()
      const current = pageScanRef.current
      const shouldRefresh =
        document.visibilityState === 'visible' &&
        !dialogRef.current?.open &&
        !refreshingRef.current &&
        !shouldStopCleanupRefresh(lastInteractionAtRef.current, now) &&
        backoffUntilRef.current <= now &&
        current != null &&
        nextCleanupRefreshDelayMs(
          snapshotAgeNow(current, pageScanReceivedAtRef.current, now),
          lastScanDurationRef.current,
        ) === 0
      if (shouldRefresh) startPageScanRef.current?.({ silent: true, source: 'foreground' })
      timer = window.setTimeout(tick, 1000)
    }
    timer = window.setTimeout(tick, 1000)
    return () => {
      for (const type of interactionEvents) document.removeEventListener(type, onInteract)
      document.removeEventListener('visibilitychange', onVisibility)
      if (timer != null) window.clearTimeout(timer)
    }
  }, [])

  const openCleanupDialog = useCallback(
    async (target: CleanupActionTarget) => {
      const current = pageScanRef.current
      const age = current
        ? snapshotAgeNow(current, pageScanReceivedAtRef.current, Date.now())
        : null
      const fresh = isSnapshotFresh(age) && Boolean(current?.confirmationFingerprint)
      const projected = current && fresh ? projectResponseForPreset(current, activePresetRef.current) : null
      const visual = projected ? projectResponseForTarget(projected, target) : null
      setActionError(null)
      setDialogState({
        open: true,
        target,
        phase: fresh ? 'loading' : 'discovery',
        response: visual ? { ...visual, snapshotAgeMs: age } : null,
        fingerprint: null,
        receivedAt: Date.now(),
        stale: false,
        stage: null,
        error: null,
      })
      if (!fresh) {
        startPageScanRef.current?.({ silent: false, source: 'dialog' })
        return
      }
      try {
        const scoped = await scanCleanups(scopedScanRequest(target, activePresetRef.current, 'confirm', false))
        if (!dialogRef.current || dialogRef.current.target.actionKey !== target.actionKey) return
        if (scoped.status === 'ready' && scoped.confirmationFingerprint) {
          patchDialog({
            phase: countVisibleResources(scoped) > 0 ? 'confirm' : 'empty',
            response: scoped,
            fingerprint: scoped.confirmationFingerprint,
            receivedAt: Date.now(),
            stage: null,
          })
        } else {
          patchDialog({ phase: 'discovery', response: visual })
          startPageScanRef.current?.({ silent: false, source: 'dialog' })
        }
      } catch (error) {
        patchDialog({ phase: 'error', error: toErrorMessage(error), response: null })
      }
    },
    [patchDialog, scopedScanRequest, setDialogState],
  )

  const closeCleanupDialog = useCallback(() => {
    setDialogState(null)
    setBusyActionKey(null)
  }, [setDialogState])

  const retryCleanupDialog = useCallback(() => {
    const target = dialogRef.current?.target
    if (target) void openCleanupDialog(target)
  }, [openCleanupDialog])

  const confirmCleanupDialog = useCallback(async () => {
    const current = dialogRef.current
    if (!current || current.phase !== 'confirm' || !current.fingerprint) return
    patchDialog({ phase: 'applying', error: null })
    setBusyActionKey(current.target.actionKey)
    try {
      const result = await applyCleanups({
        reason: 'ui',
        preset: current.response?.preset ?? activePresetRef.current,
        scope: current.target.scope,
        stackId: 'stackId' in current.target ? current.target.stackId : undefined,
        serviceId: 'serviceId' in current.target ? current.target.serviceId : undefined,
        confirmationFingerprint: current.fingerprint,
      })
      setDialogState(null)
      startTransition(() => {
        navigate({ name: 'job', jobId: result.jobId })
      })
    } catch (error) {
      if (error instanceof ApiError && error.status === 409 && error.code === 'cleanup_snapshot_stale') {
        const mismatch = parseFingerprintMismatch(error.details)
        if (mismatch?.latest) {
          const latest = mismatch.latest
          if (latest.status === 'ready' && latest.confirmationFingerprint) {
            patchDialog({
              phase: countVisibleResources(latest) > 0 ? 'confirm' : 'empty',
              response: latest,
              fingerprint: latest.confirmationFingerprint,
              receivedAt: Date.now(),
              stale: true,
            })
          } else {
            patchDialog({ phase: 'discovery', response: null, fingerprint: null, stale: true })
            startPageScanRef.current?.({ silent: false, source: 'dialog' })
          }
          return
        }
      }
      patchDialog({
        phase: 'error',
        error: error instanceof ApiError && error.status >= 500 ? CLEANUP_APPLY_ERROR : toErrorMessage(error),
      })
    } finally {
      setBusyActionKey(null)
    }
  }, [patchDialog, setDialogState])

  useEffect(() => {
    if (!dialog?.open || dialog.phase !== 'confirm' || !dialog.response) return
    const age = snapshotAgeNow(dialog.response, dialog.receivedAt, Date.now())
    if (!isSnapshotFresh(age)) {
      patchDialog({ phase: 'discovery', response: null, fingerprint: null })
      startPageScanRef.current?.({ silent: false, source: 'dialog' })
      return
    }
    const remaining = Math.max(0, CLEANUP_SNAPSHOT_MAX_AGE_MS - (age ?? 0)) + 25
    const timer = window.setTimeout(() => {
      if (dialogRef.current?.phase !== 'confirm') return
      patchDialog({ phase: 'discovery', response: null, fingerprint: null })
      startPageScanRef.current?.({ silent: false, source: 'dialog' })
    }, remaining)
    return () => window.clearTimeout(timer)
  }, [dialog?.open, dialog?.phase, dialog?.fingerprint, dialog?.receivedAt, dialog?.response, patchDialog])

  const runCleanupFlow = openCleanupDialog

  const topActions = useMemo(() => {
    const hasTargets = projected ? countVisibleResources(projected) > 0 : false
    const canStartAll = hasTargets || !pageScan
    const allActionBusy = busyActionKey === 'all'
    const allButtonLabel = '全部'
    const rescanButtonLabel = '重扫'
    return (
      <>
        <Button
          disabled={!canStartAll || Boolean(deployCheckCleanupReason) || busyActionKey !== null}
          hint={
            allActionBusy
              ? '正在创建 cleanup 任务…'
              : deployCheckCleanupReason
                ? deployCheckCleanupReason
                : hasTargets
                  ? actionHint('all')
                  : '打开扫描发现弹窗'
          }
          loading={allActionBusy}
          onClick={() =>
            void openCleanupDialog({
              actionKey: 'all',
              scope: 'all',
              title: '确认清理全部',
              targetLabel: '当前规则下的全部候选',
            })
          }
          variant={allActionBusy ? 'ghost' : 'danger'}
        >
          {allActionBusy ? (
            <span>清理中…</span>
          ) : (
            <span className="btnInlineContent">
              <TrashIcon className="inlineIcon" />
              <span>{allButtonLabel}</span>
            </span>
          )}
        </Button>
        <Button
          disabled={busyActionKey !== null || refreshing}
          hint={refreshing ? '正在重新扫描可清理资源…' : '重新扫描 cleanup 候选'}
          loading={refreshing}
          onClick={() => startPageScan({ silent: false, source: 'manual' })}
          variant="ghost"
        >
          {refreshing ? (
            <span>{rescanButtonLabel}</span>
          ) : (
            <span className="btnInlineContent">
              <RefreshIcon className="inlineIcon" />
              <span>{rescanButtonLabel}</span>
            </span>
          )}
        </Button>
      </>
    )
  }, [busyActionKey, deployCheckCleanupReason, openCleanupDialog, pageScan, projected, refreshing, startPageScan])

  useEffect(() => {
    onTopActions(topActions)
    return () => {
      onTopActions(null)
    }
  }, [onTopActions, topActions])

  const statusRailBusy = refreshing || scanPhase === 'scanning'
  const statusRailError = actionError === CLEANUP_REFRESH_ERROR || actionError === CLEANUP_APPLY_ERROR
  const statusRailErrorLabel = actionError === CLEANUP_APPLY_ERROR ? '清理失败' : '刷新失败'
  const retryStatusRail = () => startPageScan({ silent: false, source: 'manual' })

  const response = projected
  const viewResponse = useMemo(
    () => (response ? filterCleanupResponseForView(response, viewScope, viewResourceKinds) : null),
    [response, viewResourceKinds, viewScope],
  )
  const contextNavigation = useMemo(
    () => (
      <CleanupContextNavigation
        scope={viewScope}
        onScopeChange={(scope) => setViewScope(scope as CleanupScope)}
        resourceKinds={viewResourceKinds}
        availableResourceKinds={(Object.keys(KIND_LABEL) as CleanupResourceKind[]).map((key) => ({ key, label: KIND_LABEL[key] }))}
        onResourceKindsChange={setViewResourceKinds}
      />
    ),
    [viewResourceKinds, viewScope],
  )
  useEffect(() => {
    onContextNavigation?.(contextNavigation)
    return () => onContextNavigation?.(null)
  }, [contextNavigation, onContextNavigation])

  const dialogNode = dialog ? (
    <CleanupConfirmDialog
      error={dialog.error}
      onCancel={closeCleanupDialog}
      onConfirm={() => void confirmCleanupDialog()}
      onRetry={retryCleanupDialog}
      open={dialog.open}
      phase={dialog.phase}
      presetLabel={presetLabel(dialog.response?.preset ?? activePreset)}
      response={dialog.response}
      safetyNote={confirmSafetyNote(dialog.target.scope)}
      scopeLabel={scopeLabel(dialog.target.scope)}
      stage={dialog.stage}
      stale={dialog.stale}
      targetLabel={dialog.target.targetLabel}
      title={dialog.target.title}
    />
  ) : null

  if (initialScanPending) {
    return (
      <div className="page cleanupPage">
        <div className="card cleanupOverviewCard">
          <div className="cleanupLoadingState">正在扫描可清理资源…</div>
        </div>
        {dialogNode}
      </div>
    )
  }

  if (!pageScan && pageError) {
    return (
      <div className="page cleanupPage">
        <div className="card cleanupOverviewCard">
          <div className="cleanupAlert cleanupAlertError">{pageError}</div>
          <Button loading={refreshing} onClick={() => startPageScan({ silent: false, source: 'manual' })} variant="primary">
            重新扫描
          </Button>
        </div>
        {dialogNode}
      </div>
    )
  }

  return (
    <div className="page cleanupPage">
      <section className="card cleanupStatusCard">
        <div className="cleanupStatusHead">
          <div className="cleanupOverviewIntro">
            <SectionTitle>空间概览</SectionTitle>
            <div className="cleanupOverviewTitleRow">
              <div className="title">可回收候选</div>
              <Pill tone="info">Docker 清理候选</Pill>
            </div>
            <div className="cleanupUsageSectionHint">这是最近一次全量扫描识别到的可回收候选分布，用来看清回收空间主要集中在哪类资源。</div>
          </div>
          <div className="cleanupOverviewStats cleanupStatusStats">
            <div className={`cleanupOverviewStat cleanupDiskStat${hasStaleResources ? ' cleanupStaleLoading' : ''}`}>
              <div className="sectionTitle">服务器磁盘</div>
              <div className="cleanupOverviewStatValue">{serverDiskUsage.value}</div>
              <div className="cleanupOverviewStatHint">{serverDiskUsage.hint}</div>
              <div aria-hidden="true" className="cleanupDiskUsageBar">
                <span style={{ width: `${Math.round(serverDiskUsage.percent * 100)}%` }} />
              </div>
            </div>
            <div className={`cleanupOverviewStat${hasStaleResources ? ' cleanupStaleLoading' : ''}`}>
              <div className="sectionTitle">当前可回收候选</div>
              <div className="cleanupOverviewStatValue">
                {pageScan ? formatEstimate(pageScan.estimatedReclaimableBytes, pageScan.hasUnknownSize) : '-'}
              </div>
              <div className="cleanupOverviewStatHint">
                {pageSnapshotStale
                  ? '快照已过期，动作会先重新发现'
                  : hasStaleResources
                    ? '扫描进行中，局部结果会逐步覆盖'
                    : pageUnknownCount > 0
                    ? `${formatUnknownCount(pageUnknownCount)}，已知部分按下限展示`
                    : '基于最近一次全量扫描候选'}
              </div>
            </div>
            <div className={`cleanupOverviewStat cleanupLatestScanStat${hasStaleResources ? ' cleanupStaleLoading' : ''}`}>
              <div className="sectionTitle">最新扫描</div>
              <div className="cleanupOverviewStatMeta">
                <Mono>{pageScan ? formatShort(pageScan.scannedAt) : '-'}</Mono>
              </div>
              <div className="cleanupOverviewStatHint">{pageSnapshotStale ? '快照已过期' : '最近一次全量扫描'}</div>
            </div>
          </div>
        </div>

        <div className="cleanupUsageSection">
          <div className="cleanupUsageGrid">
            {usageCards.map((card) => (
              <CleanupUsageCardView key={card.key} card={card} refreshing={staleUsageBuckets.has(card.key)} />
            ))}
          </div>
        </div>
      </section>

      <section className="card cleanupOverviewCard">
        <div className="cleanupOverviewHead">
          <div className="cleanupOverviewIntro cleanupRuleIntro">
            <SectionTitle>清理规则</SectionTitle>
            <div className="cleanupOverviewTitleRow">
              <div className="title">{activePresetMeta.label}</div>
              <Pill tone={activePreset === 'aggressive' ? 'warn' : activePreset === 'project_deep_clean' ? 'info' : 'muted'}>
                {response ? `${countVisibleResources(response)} 项候选` : '扫描中'}
              </Pill>
            </div>

            <Tabs onValueChange={(value) => setActivePreset(value as CleanupPreset)} value={activePreset}>
              <TabsList className="cleanupPresetTabs cleanupPresetTabsInline" aria-label="清理规则">
                {PRESET_META.map((preset) => (
                  <TabsTrigger
                    key={preset.key}
                    className={activePreset === preset.key ? 'cleanupPresetTab cleanupPresetTabActive' : 'cleanupPresetTab'}
                    value={preset.key}
                  >
                    {preset.label}
                  </TabsTrigger>
                ))}
              </TabsList>
            </Tabs>
          </div>
          <div className="cleanupOverviewStats cleanupRuleStats">
            <div className={`cleanupOverviewStat${hasStaleResources ? ' cleanupStaleLoading' : ''}`}>
              <div className="sectionTitle">当前规则预计释放</div>
              <div className="cleanupOverviewStatValue">
                {response ? formatEstimate(response.estimatedReclaimableBytes, response.hasUnknownSize) : '-'}
              </div>
              <div className="cleanupOverviewStatHint">
                {pageSnapshotStale
                  ? '快照已过期，动作会先重新发现'
                  : hasStaleResources
                    ? '缓存项保持可读，等待扫描覆盖'
                    : projectedUnknownCount > 0
                    ? `${formatUnknownCount(projectedUnknownCount)}，释放量按下限展示`
                    : '会随规则切换重新投影'}
              </div>
            </div>
            <div className={`cleanupOverviewStat${hasStaleResources ? ' cleanupStaleLoading' : ''}`}>
              <div className="sectionTitle">最新扫描</div>
              <div className="cleanupOverviewStatMeta">
                <Mono>{response ? formatShort(response.scannedAt) : '-'}</Mono>
              </div>
              <div className="cleanupOverviewStatHint">{pageSnapshotStale ? '快照已过期' : '会随规则切换重新投影'}</div>
            </div>
          </div>
        </div>
      </section>

      {statusRailBusy || statusRailError ? (
        <div
          aria-atomic="true"
          aria-busy={statusRailBusy}
          className={`cleanupStatusRail${statusRailError ? ' cleanupStatusRailError' : ''}`}
          role="status"
        >
          <span aria-hidden="true" className={`cleanupStatusRailIndicator${statusRailBusy ? ' cleanupStatusRailIndicatorBusy' : ''}`} />
          <span className="cleanupStatusRailLabel">{statusRailError ? statusRailErrorLabel : '更新中'}</span>
          {statusRailError ? (
            <Button
              className="cleanupStatusRailRetry"
              disabled={busyActionKey !== null}
              onClick={retryStatusRail}
              variant="ghost"
            >
              重试
            </Button>
          ) : null}
        </div>
      ) : null}

      {pageError ? <div className="cleanupAlert cleanupAlertError">{pageError}</div> : null}
      {actionError && !statusRailError ? <div className="cleanupAlert cleanupAlertError">{actionError}</div> : null}

      {viewResponse ? (
        <CleanupResponseView
          busyActionKey={busyActionKey}
          deployCheckReport={deployCheckReport}
          scanBusy={false}
          onServiceAction={(stack, serviceId, serviceName) =>
            void runCleanupFlow({
              actionKey: `service:${stack.stackId}:${serviceId}`,
              scope: 'service',
              stackId: stack.stackId,
              serviceId,
              title: `确认清理服务 · ${serviceName}`,
              targetLabel: `${stack.stackName} / ${serviceName}`,
            })
          }
          onStackAction={(stack) =>
            void runCleanupFlow({
              actionKey: `stack:${stack.stackId}`,
              scope: 'stack',
              stackId: stack.stackId,
              title: `确认清理 Stack · ${stack.stackName}`,
              targetLabel: stack.stackName,
            })
          }
          response={viewResponse}
          staleResourceKeys={staleResourceKeys}
        />
      ) : null}
      {dialogNode}
    </div>
  )
}
