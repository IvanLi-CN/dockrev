import { startTransition, useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import {
  ApiError,
  applyCleanups,
  scanCleanups,
  type CleanupApplyRequest,
  type CleanupFingerprintMismatchError,
  type CleanupPreset,
  type CleanupResourceItem,
  type CleanupResourceKind,
  type CleanupScanResponse,
  type CleanupScope,
  type CleanupStackGroup,
} from '../api'
import { useConfirm } from '../confirm'
import { navigate } from '../routes'
import { Button, Mono, Pill, SectionTitle, Tabs, TabsList, TabsTrigger } from '../ui'

const PRESET_ORDER: CleanupPreset[] = ['conservative', 'balanced', 'project_deep_clean', 'aggressive']

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

const KIND_LABEL: Record<CleanupResourceKind, string> = {
  image: '镜像',
  container: '容器',
  network: '网络',
  volume: '卷',
  builder_cache: 'Builder Cache',
}

const KIND_TONE: Record<CleanupResourceKind, 'info' | 'muted' | 'warn'> = {
  image: 'info',
  container: 'muted',
  network: 'muted',
  volume: 'warn',
  builder_cache: 'warn',
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
  return date.toLocaleString()
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let value = bytes
  let index = 0
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024
    index += 1
  }
  const digits = value >= 100 || index === 0 ? 0 : value >= 10 ? 1 : 2
  return `${value.toFixed(digits)} ${units[index]}`
}

function formatEstimate(bytes: number, hasUnknown?: boolean): string {
  if (hasUnknown) {
    if (bytes > 0) return `${formatBytes(bytes)} + 未知`
    return '估算中'
  }
  return formatBytes(bytes)
}

function countVisibleResources(response: CleanupScanResponse): number {
  let total = 0
  for (const stack of response.stackGroups) {
    total += stack.stackOrphans.length
    for (const service of stack.services) total += service.resources.length
  }
  total += response.unownedGroup?.resources.length ?? 0
  return total
}

function kindSummary(resources: CleanupResourceItem[]): string {
  const counts = new Map<CleanupResourceKind, number>()
  for (const resource of resources) {
    counts.set(resource.kind, (counts.get(resource.kind) ?? 0) + 1)
  }
  return [...counts.entries()]
    .map(([kind, count]) => `${KIND_LABEL[kind]} ${count}`)
    .join(' · ')
}

function toErrorMessage(error: unknown): string {
  if (error instanceof ApiError) return error.message
  if (error instanceof Error && error.message.trim()) return error.message
  return '请求失败，请稍后重试。'
}

function includesPreset(active: CleanupPreset, candidate: CleanupPreset): boolean {
  return PRESET_ORDER.indexOf(active) >= PRESET_ORDER.indexOf(candidate)
}

function itemHasUnknownSize(item: CleanupResourceItem): boolean {
  return item.estimateUnknown === true || item.estimatedReclaimableBytes == null
}

function projectResponseForPreset(pageScan: CleanupScanResponse, preset: CleanupPreset): CleanupScanResponse {
  const stackGroups: CleanupStackGroup[] = []
  let totalBytes = 0
  let totalUnknown = false

  for (const stack of pageScan.stackGroups) {
    const projectedOrphans = stack.stackOrphans.filter((item) => includesPreset(preset, item.minPreset))
    const projectedServices = stack.services
      .map((service) => {
        const resources = service.resources.filter((item) => includesPreset(preset, item.minPreset))
        const estimatedReclaimableBytes = resources.reduce(
          (sum, item) => sum + (item.estimatedReclaimableBytes ?? 0),
          0,
        )
        const hasUnknownSize = resources.some(itemHasUnknownSize)
        return {
          ...service,
          resources,
          estimatedReclaimableBytes,
          hasUnknownSize,
        }
      })
      .filter((service) => service.resources.length > 0)

    if (projectedOrphans.length === 0 && projectedServices.length === 0) continue

    const orphanBytes = projectedOrphans.reduce((sum, item) => sum + (item.estimatedReclaimableBytes ?? 0), 0)
    const orphanUnknown = projectedOrphans.some(itemHasUnknownSize)
    const serviceBytes = projectedServices.reduce((sum, service) => sum + service.estimatedReclaimableBytes, 0)
    const serviceUnknown = projectedServices.some((service) => service.hasUnknownSize === true)
    const estimatedReclaimableBytes = orphanBytes + serviceBytes
    const hasUnknownSize = orphanUnknown || serviceUnknown

    totalBytes += estimatedReclaimableBytes
    totalUnknown ||= hasUnknownSize
    stackGroups.push({
      ...stack,
      stackOrphans: projectedOrphans,
      services: projectedServices,
      estimatedReclaimableBytes,
      hasUnknownSize,
    })
  }

  const projectedUnowned =
    pageScan.unownedGroup?.resources.filter((item) => includesPreset(preset, item.minPreset)) ?? []

  const unownedGroup =
    projectedUnowned.length > 0
      ? {
          title: pageScan.unownedGroup?.title ?? '未归属资源',
          resources: projectedUnowned,
          estimatedReclaimableBytes: projectedUnowned.reduce(
            (sum, item) => sum + (item.estimatedReclaimableBytes ?? 0),
            0,
          ),
          hasUnknownSize: projectedUnowned.some(itemHasUnknownSize),
        }
      : null

  if (unownedGroup) {
    totalBytes += unownedGroup.estimatedReclaimableBytes
    totalUnknown ||= unownedGroup.hasUnknownSize === true
  }

  return {
    ...pageScan,
    preset,
    estimatedReclaimableBytes: totalBytes,
    hasUnknownSize: totalUnknown,
    stackGroups,
    unownedGroup,
  }
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

function CleanupResourceList(props: { resources: CleanupResourceItem[]; compact?: boolean }) {
  return (
    <div className={props.compact ? 'cleanupResourceList cleanupResourceListCompact' : 'cleanupResourceList'}>
      {props.resources.map((resource) => (
        <div key={resource.resourceId} className="cleanupResourceRow">
          <div className="cleanupResourceMain">
            <Pill tone={KIND_TONE[resource.kind]}>{KIND_LABEL[resource.kind]}</Pill>
            <div className="cleanupResourceLabel">{resource.label}</div>
          </div>
          <div className="cleanupResourceEstimate">
            <Mono>{formatEstimate(resource.estimatedReclaimableBytes ?? 0, itemHasUnknownSize(resource))}</Mono>
          </div>
        </div>
      ))}
    </div>
  )
}

function CleanupResponseView(props: {
  response: CleanupScanResponse
  compact?: boolean
  busyActionKey?: string | null
  onStackAction?: (stack: CleanupStackGroup) => void
  onServiceAction?: (stack: CleanupStackGroup, serviceId: string, serviceName: string) => void
}) {
  if (countVisibleResources(props.response) === 0) {
    return <div className="cleanupEmptyState">当前规则下没有可清理资源。</div>
  }

  return (
    <div className={props.compact ? 'cleanupStackGrid cleanupStackGridCompact' : 'cleanupStackGrid'}>
      {props.response.stackGroups.map((stack) => (
        <section key={stack.stackId} className={props.compact ? 'card cleanupStackCard cleanupStackCardCompact' : 'card cleanupStackCard'}>
          <div className="cleanupStackHeader">
            <div className="cleanupStackHeading">
              <div className="cleanupStackTitleRow">
                <div className="title">{stack.stackName}</div>
                <Pill tone="muted">Stack</Pill>
              </div>
              <div className="muted">
                {formatEstimate(stack.estimatedReclaimableBytes, stack.hasUnknownSize)} ·
                {' '}
                {stack.stackOrphans.length > 0 ? `${kindSummary(stack.stackOrphans)} · ` : ''}
                服务 {stack.services.length}
              </div>
            </div>
            {props.onStackAction ? (
              <Button
                disabled={stack.estimatedReclaimableBytes <= 0 && stack.hasUnknownSize !== true}
                loading={props.busyActionKey === `stack:${stack.stackId}`}
                onClick={() => props.onStackAction?.(stack)}
                variant="danger"
              >
                清理此 stack
              </Button>
            ) : null}
          </div>

          {stack.stackOrphans.length > 0 ? (
            <div className="cleanupSectionBlock">
              <div className="cleanupSectionHeader">
                <SectionTitle>Stack Orphans</SectionTitle>
                <Mono>{formatEstimate(
                  stack.stackOrphans.reduce((sum, item) => sum + (item.estimatedReclaimableBytes ?? 0), 0),
                  stack.stackOrphans.some(itemHasUnknownSize),
                )}</Mono>
              </div>
              <CleanupResourceList compact={props.compact} resources={stack.stackOrphans} />
            </div>
          ) : null}

          <div className="cleanupServiceGrid">
            {stack.services.map((service) => (
              <article key={service.serviceId} className="cleanupServiceCard">
                <div className="cleanupServiceHeader">
                  <div className="cleanupServiceHeading">
                    <div className="cleanupServiceTitleRow">
                      <div className="cleanupServiceName">{service.serviceName}</div>
                      <Pill tone="muted">{service.resources.length} 项</Pill>
                    </div>
                    <div className="muted">
                      {kindSummary(service.resources)} · {formatEstimate(service.estimatedReclaimableBytes, service.hasUnknownSize)}
                    </div>
                  </div>
                  {props.onServiceAction ? (
                    <Button
                      disabled={service.estimatedReclaimableBytes <= 0 && service.hasUnknownSize !== true}
                      loading={props.busyActionKey === `service:${stack.stackId}:${service.serviceId}`}
                      onClick={() => props.onServiceAction?.(stack, service.serviceId, service.serviceName)}
                      variant="danger"
                    >
                      清理此服务
                    </Button>
                  ) : null}
                </div>
                <CleanupResourceList compact={props.compact} resources={service.resources} />
              </article>
            ))}
          </div>
        </section>
      ))}

      {props.response.unownedGroup?.resources.length ? (
        <section className={props.compact ? 'card cleanupStackCard cleanupStackCardCompact' : 'card cleanupStackCard'}>
          <div className="cleanupStackHeader">
            <div className="cleanupStackHeading">
              <div className="cleanupStackTitleRow">
                <div className="title">{props.response.unownedGroup.title}</div>
                <Pill tone="warn">仅全部</Pill>
              </div>
              <div className="muted">
                {kindSummary(props.response.unownedGroup.resources)} ·{' '}
                {formatEstimate(
                  props.response.unownedGroup.estimatedReclaimableBytes,
                  props.response.unownedGroup.hasUnknownSize,
                )}
              </div>
            </div>
          </div>
          <CleanupResourceList compact={props.compact} resources={props.response.unownedGroup.resources} />
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
  const [activePreset, setActivePreset] = useState<CleanupPreset>('balanced')
  const [pageScan, setPageScan] = useState<CleanupScanResponse | null>(null)
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [pageError, setPageError] = useState<string | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [busyActionKey, setBusyActionKey] = useState<string | null>(null)

  const projected = useMemo(
    () => (pageScan ? projectResponseForPreset(pageScan, activePreset) : null),
    [activePreset, pageScan],
  )
  const activePresetMeta = useMemo(
    () => PRESET_META.find((item) => item.key === activePreset) ?? PRESET_META[1],
    [activePreset],
  )

  const refreshPageScan = useCallback(async () => {
    setRefreshing(true)
    setPageError(null)
    try {
      const response = await scanCleanups({
        reason: 'page',
        preset: 'aggressive',
        scope: 'all',
      })
      setPageScan(response)
      onLastScanHint?.(response.scannedAt)
    } catch (error) {
      const message = toErrorMessage(error)
      setPageError(message)
      onLastScanHint?.(undefined)
    } finally {
      setLoading(false)
      setRefreshing(false)
    }
  }, [onLastScanHint])

  useEffect(() => {
    onLastScanHint?.(undefined)
    void refreshPageScan()
  }, [onLastScanHint, refreshPageScan])

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

        let latest = await scanCleanups({
          reason: 'confirm',
          preset: activePreset,
          scope: target.scope,
          stackId: confirmRequest.stackId,
          serviceId: confirmRequest.serviceId,
        })

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
                stale = true
                if (countVisibleResources(latest) === 0) {
                  await confirm({
                    title: '候选已变化',
                    body: <CleanupConfirmBody response={latest} stale targetLabel={target.targetLabel} />,
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
    [activePreset, confirm],
  )

  const topActions = useMemo(() => {
    const hasTargets = projected ? countVisibleResources(projected) > 0 : false
    return (
      <>
        <Button
          disabled={!hasTargets || busyActionKey !== null || refreshing}
          hint={hasTargets ? undefined : '当前规则下没有可清理项'}
          loading={busyActionKey === 'all'}
          onClick={() =>
            void runCleanupFlow({
              actionKey: 'all',
              scope: 'all',
              title: '确认清理全部',
              targetLabel: '当前规则下的全部候选',
            })
          }
          variant="danger"
        >
          全部
        </Button>
        <Button disabled={busyActionKey !== null} loading={refreshing} onClick={() => void refreshPageScan()} variant="ghost">
          重扫
        </Button>
      </>
    )
  }, [busyActionKey, projected, refreshPageScan, refreshing, runCleanupFlow])

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
      <section className="card cleanupOverviewCard">
        <div className="cleanupOverviewHead">
          <div className="cleanupOverviewIntro">
            <SectionTitle>清理规则</SectionTitle>
            <div className="cleanupOverviewTitleRow">
              <div className="title">{activePresetMeta.label}</div>
              <Pill tone={activePreset === 'aggressive' ? 'warn' : activePreset === 'project_deep_clean' ? 'info' : 'muted'}>
                {response ? `${countVisibleResources(response)} 项候选` : '扫描中'}
              </Pill>
            </div>
            <div className="muted">{activePresetMeta.description}</div>
          </div>
          <div className="cleanupOverviewStats">
            <div className="cleanupOverviewStat">
              <div className="sectionTitle">预计释放</div>
              <div className="cleanupOverviewStatValue">
                {response ? formatEstimate(response.estimatedReclaimableBytes, response.hasUnknownSize) : '-'}
              </div>
            </div>
            <div className="cleanupOverviewStat">
              <div className="sectionTitle">最新扫描</div>
              <div className="cleanupOverviewStatMeta">
                <Mono>{response ? formatShort(response.scannedAt) : '-'}</Mono>
              </div>
            </div>
          </div>
        </div>

        <Tabs onValueChange={(value) => setActivePreset(value as CleanupPreset)} value={activePreset}>
          <TabsList className="cleanupPresetTabs" aria-label="清理规则">
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
        />
      ) : null}
    </div>
  )
}
