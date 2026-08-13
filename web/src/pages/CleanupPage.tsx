import { startTransition, useCallback, useEffect, useRef, useMemo, useState, type ReactNode } from 'react'
import { Box, HardDrive, Layers3, Package } from 'lucide-react'
import {
  ApiError,
  applyCleanups,
  scanCleanups,
  startCleanupScanRun,
  type CleanupApplyRequest,
  type CleanupFingerprintMismatchError,
  type CleanupPreset,
  type CleanupResourceItem,
  type CleanupResourceKind,
  type CleanupScanRequest,
  type CleanupScanResponse,
  type CleanupScope,
  type CleanupStackGroup,
} from '../api'
import { useConfirm } from '../confirm'
import { useManagementEventBatch } from '../managementEvents'
import { navigate } from '../routes'
import { Button, Mono, Pill, RefreshIcon, SectionTitle, Tabs, TabsList, TabsTrigger, TrashIcon } from '../ui'
import {
  KIND_LABEL,
  aggregateStackResources,
  buildUsageCards,
  cleanupResourceKey,
  cleanupResourceKeys,
  countUnknownResources,
  countVisibleResources,
  flattenAllResources,
  formatBytes,
  formatDiskUsage,
  formatEstimate,
  formatPercent,
  formatUnknownCount,
  itemHasUnknownSize,
  kindSummary,
  projectResponseForPreset,
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
  onStackAction?: (stack: CleanupStackGroup) => void
  onServiceAction?: (stack: CleanupStackGroup, serviceId: string, serviceName: string) => void
}) {
  if (countVisibleResources(props.response) === 0) {
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

      {props.response.stackGroups.map((stack) => (
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
                  disabled={stack.estimatedReclaimableBytes <= 0 && stack.hasUnknownSize !== true}
                  hint={actionHint('stack')}
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
                    disabled={stack.estimatedReclaimableBytes <= 0 && stack.hasUnknownSize !== true}
                    hint={actionHint('stack')}
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
                    disabled={false}
                    hint={actionHint('service')}
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
      ))}

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

function CleanupConfirmBody(props: { response: CleanupScanResponse; targetLabel: string; stale?: boolean }) {
  return (
    <div className="cleanupConfirmBody">
      <div className="cleanupConfirmSummary">
        <div className="cleanupConfirmSummaryText">
          <div className="modalLead">
            {props.targetLabel} · {presetLabel(props.response.preset)} · {scopeLabel(props.response.scope)}
          </div>
          <div className="muted">
            最新扫描：<Mono>{formatShort(props.response.scannedAt)}</Mono>
          </div>
        </div>
        <div className="cleanupConfirmEstimate">
          <div className="sectionTitle">预计释放</div>
          <div className="cleanupConfirmEstimateValue">{formatEstimate(
            props.response.estimatedReclaimableBytes,
            props.response.hasUnknownSize,
          )}</div>
        </div>
      </div>

      {props.stale ? (
        <div className="cleanupAlert cleanupAlertWarn">候选已变化，已替换为最新扫描结果，请再次确认。</div>
      ) : null}

      <div className="cleanupAlert cleanupAlertInfo">{confirmSafetyNote(props.response.scope)}</div>

      <CleanupResponseView compact response={props.response} />
    </div>
  )
}

export function CleanupPage(props: {
  onLastScanHint?: (lastScan?: string) => void
  onTopActions: (node: ReactNode) => void
}) {
  const { onLastScanHint, onTopActions } = props
  const confirm = useConfirm()
  const activeScanIdRef = useRef<string | null>(null)
  const [activePreset, setActivePreset] = useState<CleanupPreset>('balanced')
  const [pageScan, setPageScan] = useState<CleanupScanResponse | null>(null)
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [staleResourceKeys, setStaleResourceKeys] = useState<Set<string>>(() => new Set())
  const [pageError, setPageError] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [busyActionKey, setBusyActionKey] = useState<string | null>(null)

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
  const hasStaleResources = staleResourceKeys.size > 0 && refreshing

  const fetchCleanupScan = useCallback(async (input: CleanupScanRequest) => scanCleanups(input), [])

  const loadCompletedPageScan = useCallback(async () => {
    const response = await scanCleanups({
      reason: 'page',
      refresh: false,
      preset: 'aggressive',
      scope: 'all',
    })
    if (response.status !== 'ready' || response.refreshing) return
    setPageScan(response)
    setStaleResourceKeys(new Set())
    setLoading(false)
    setRefreshing(false)
    onLastScanHint?.(response.scannedAt ?? undefined)
  }, [onLastScanHint])

  const refreshPageScan = useCallback(async () => {
    setRefreshing(true)
    setPageError(null)
    try {
      const request: CleanupScanRequest = {
        reason: 'page',
        refresh: true,
        preset: 'aggressive',
        scope: 'all',
      }
      const started = await startCleanupScanRun(request)
      activeScanIdRef.current = started.scanId
      if (started.previousSnapshot) {
        setPageScan(started.previousSnapshot)
        setStaleResourceKeys(cleanupResourceKeys(started.previousSnapshot))
        onLastScanHint?.(started.previousSnapshot.scannedAt ?? undefined)
        setLoading(false)
      } else {
        setStaleResourceKeys(new Set())
      }
    } catch (error) {
      const message = toErrorMessage(error)
      setPageError(message)
      setStaleResourceKeys(new Set())
      onLastScanHint?.(undefined)
      setLoading(false)
      setRefreshing(false)
    }
  }, [onLastScanHint])

  useEffect(() => {
    onLastScanHint?.(undefined)
    void refreshPageScan()
  }, [onLastScanHint, refreshPageScan])

  useManagementEventBatch(({ events, resyncRequired }) => {
    if (resyncRequired) {
      void loadCompletedPageScan().catch((error: unknown) => setPageError(toErrorMessage(error)))
    }
    const event = events.find((candidate) =>
      candidate.domain === 'cleanup' && candidate.entities.some((entity) => entity.entityType === 'scan' && entity.id === 'active'),
    )
    if (!event) return
    if (event.summary.phase === 'ready') {
      void loadCompletedPageScan().catch((error: unknown) => {
        setPageError(toErrorMessage(error))
        setRefreshing(false)
        setLoading(false)
      })
      return
    }
    if (event.summary.phase === 'failed') {
      setPageError(typeof event.summary.message === 'string' ? event.summary.message : 'cleanup scan failed')
      setStaleResourceKeys(new Set())
      setRefreshing(false)
      setLoading(false)
    }
  })

  const runCleanupFlow = useCallback(
    async (target: CleanupActionTarget) => {
      setActionError(null)
      setBusyActionKey(target.actionKey)
      try {
        const confirmRequest: CleanupApplyRequest = {
          reason: 'ui',
          preset: activePreset,
          scope: target.scope,
          stackId: 'stackId' in target ? target.stackId : undefined,
          serviceId: 'serviceId' in target ? target.serviceId : undefined,
          confirmationFingerprint: '',
        }

        let latest = await fetchCleanupScan({
          reason: 'confirm',
          refresh: true,
          preset: activePreset,
          scope: target.scope,
          stackId: confirmRequest.stackId,
          serviceId: confirmRequest.serviceId,
        })

        if (latest.status !== 'ready') {
          throw new Error('cleanup snapshot is not ready')
        }

        if (countVisibleResources(latest) === 0) {
          await confirm({
            title: '暂无可清理资源',
            body: <CleanupConfirmBody response={latest} targetLabel={target.targetLabel} />,
            badgeText: '已刷新',
            badgeTone: 'muted',
            bodyClassName: 'cleanupConfirmDialogBody',
            cardClassName: 'cleanupConfirmDialogCard',
            cancelText: '关闭',
            confirmText: '知道了',
            confirmVariant: 'ghost',
          })
          return
        }

        let stale = false
        while (true) {
          const approved = await confirm({
            title: target.title,
            body: <CleanupConfirmBody response={latest} stale={stale} targetLabel={target.targetLabel} />,
            badgeText: stale ? '候选已变化' : '二次确认',
            badgeTone: stale ? 'warn' : 'bad',
            bodyClassName: 'cleanupConfirmDialogBody',
            cardClassName: 'cleanupConfirmDialogCard',
            cancelText: '取消',
            confirmText: '确认清理',
            confirmVariant: 'danger',
          })
          if (!approved) return

          try {
            const result = await applyCleanups({
              ...confirmRequest,
              confirmationFingerprint: latest.confirmationFingerprint ?? '',
            })
            startTransition(() => {
              navigate({ name: 'job', jobId: result.jobId })
            })
            return
          } catch (error) {
            if (error instanceof ApiError && error.status === 409 && error.code === 'cleanup_snapshot_stale') {
              const mismatch = parseFingerprintMismatch(error.details)
              if (mismatch?.latest) {
                latest = mismatch.latest
                if (latest.status !== 'ready') {
                  latest = await fetchCleanupScan({
                    reason: 'confirm',
                    refresh: true,
                    preset: activePreset,
                    scope: target.scope,
                    stackId: confirmRequest.stackId,
                    serviceId: confirmRequest.serviceId,
                  })
                }
                stale = true
                if (latest.status !== 'ready' || countVisibleResources(latest) === 0) {
                  await confirm({
                    title: '候选已变化',
                    body:
                      latest.status === 'ready' ? (
                        <CleanupConfirmBody response={latest} stale targetLabel={target.targetLabel} />
                      ) : (
                        '最新 cleanup snapshot 仍在刷新，请稍后重试。'
                      ),
                    badgeText: '无需执行',
                    badgeTone: 'muted',
                    bodyClassName: 'cleanupConfirmDialogBody',
                    cardClassName: 'cleanupConfirmDialogCard',
                    cancelText: '关闭',
                    confirmText: '知道了',
                    confirmVariant: 'ghost',
                  })
                  return
                }
                continue
              }
            }
            throw error
          }
        }
      } catch (error) {
        setActionError(toErrorMessage(error))
      } finally {
        setBusyActionKey(null)
      }
    },
    [activePreset, confirm, fetchCleanupScan],
  )

  const topActions = useMemo(() => {
    const hasTargets = projected ? countVisibleResources(projected) > 0 : false
    const scanBusy = initialScanPending || refreshing
    const allActionBusy = busyActionKey === 'all'
    const allButtonLabel = allActionBusy ? '清理中…' : scanBusy ? '等待扫描' : '全部'
    const rescanButtonLabel = initialScanPending ? '扫描中…' : refreshing ? '重扫中…' : '重扫'
    return (
      <>
        <Button
          disabled={!hasTargets || busyActionKey !== null || scanBusy}
          hint={
            allActionBusy
              ? '正在创建 cleanup 任务…'
              : scanBusy
                ? '等待扫描完成后才可执行全部清理'
                : hasTargets
                  ? actionHint('all')
                  : '当前规则下没有可清理项'
          }
          loading={allActionBusy}
          onClick={() =>
            void runCleanupFlow({
              actionKey: 'all',
              scope: 'all',
              title: '确认清理全部',
              targetLabel: '当前规则下的全部候选',
            })
          }
          variant={allActionBusy || (!scanBusy && hasTargets) ? 'danger' : 'ghost'}
        >
          {allActionBusy || scanBusy ? (
            <span>{allButtonLabel}</span>
          ) : (
            <span className="btnInlineContent">
              <TrashIcon className="inlineIcon" />
              <span>{allButtonLabel}</span>
            </span>
          )}
        </Button>
        <Button
          disabled={busyActionKey !== null || scanBusy}
          hint={scanBusy ? '正在重新扫描可清理资源…' : '重新扫描 cleanup 候选'}
          loading={scanBusy}
          onClick={() => void refreshPageScan()}
          variant="ghost"
        >
          {scanBusy ? (
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
  }, [busyActionKey, initialScanPending, projected, refreshPageScan, refreshing, runCleanupFlow])

  useEffect(() => {
    onTopActions(topActions)
    return () => {
      onTopActions(null)
    }
  }, [onTopActions, topActions])

  if (loading && !pageScan) {
    return (
      <div className="page cleanupPage">
        <div className="card cleanupOverviewCard">
          <div className="cleanupLoadingState">正在扫描可清理资源…</div>
        </div>
      </div>
    )
  }

  if (!pageScan && pageError) {
    return (
      <div className="page cleanupPage">
        <div className="card cleanupOverviewCard">
          <div className="cleanupAlert cleanupAlertError">{pageError}</div>
          <Button loading={refreshing} onClick={() => void refreshPageScan()} variant="primary">
            重新扫描
          </Button>
        </div>
      </div>
    )
  }

  const response = projected

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
                {hasStaleResources
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
                {hasStaleResources
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
            </div>
          </div>
        </div>
      </section>

      {pageError ? <div className="cleanupAlert cleanupAlertError">{pageError}</div> : null}
      {actionError ? <div className="cleanupAlert cleanupAlertError">{actionError}</div> : null}

      {response ? (
        <CleanupResponseView
          busyActionKey={busyActionKey}
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
          response={response}
          staleResourceKeys={staleResourceKeys}
        />
      ) : null}
    </div>
  )
}
