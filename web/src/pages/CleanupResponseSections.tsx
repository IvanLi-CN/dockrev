import { Box, HardDrive, Layers3, Package } from 'lucide-react'
import { Button, Pill } from '../ui'
import type {
  CleanupResourceItem,
  CleanupResourceKind,
  CleanupScanResponse,
  CleanupStackGroup,
} from '../api'
import { deployCheckOperationImpactReason } from '../deployCheck'
import {
  KIND_LABEL,
  aggregateStackResources,
  cleanupResourceKey,
  countRenderableResources,
  formatBytes,
  formatEstimate,
  formatPercent,
  formatUnknownCount,
  itemHasUnknownSize,
  kindSummary,
  type CleanupUsageBucket,
  type CleanupUsageCard,
} from './cleanupPageModel'

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


export function actionHint(scope: 'all' | 'stack' | 'service'): string {
  if (scope === 'all') {
    return '清理当前规则下的全部候选；不会停止正在运行的容器。'
  }
  if (scope === 'stack') {
    return '清理这个 stack 当前规则下的全部候选，不只当前这一行；不会停止正在运行的容器。'
  }
  return '清理这个服务当前规则下的全部候选，不只当前这一行；不会停止正在运行的容器。'
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

export function CleanupUsageCardView(props: { card: CleanupUsageCard; refreshing?: boolean }) {
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

export function CleanupResponseView(props: {
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

