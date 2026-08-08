import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { ArrowLeft, Play, RefreshCw, RotateCw, Square } from 'lucide-react'
import {
  ApiError,
  getStack,
  getStackLifecycleStatus,
  getStackSettings,
  listJobs,
  putStackSettings,
  triggerStackLifecycle,
  type JobListItem,
  type ServiceLifecycleAction,
  type ServiceLifecycleStatusResponse,
  type StackDetail,
  type StackSettings,
} from '../api'
import { createDefaultAutoUpdatePolicy } from '../components/AutoUpdatePolicyEditor'
import { AutoUpdatePolicyDrawer } from '../components/AutoUpdatePolicyDrawer'
import { AutoUpdatePolicyResultCard } from '../components/AutoUpdatePolicyResultCard'
import { RecentUpdateRecords, selectRecentStackUpdateJobs } from '../components/RecentUpdateRecords'
import { ReadonlySnapshotNotice } from '../components/ReadonlySnapshotNotice'
import { ServiceMobileActionMenu, ServiceSplitActionButton } from '../components/ServiceSplitActionButton'
import { useConfirm } from '../confirm'
import { usePwaStatus } from '../pwaStatus'
import { buildReadonlySnapshotKey, readReadonlySnapshot, writeReadonlySnapshot } from '../readonlySnapshotCache'
import { navigate } from '../routes'
import { publishServiceTreeRefresh } from '../serviceTreeRefresh'
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

const lifecycleReasonLabels: Record<string, string> = {
  lifecycle_status_loading: '正在读取实时状态',
  lifecycle_status_unavailable: '暂时无法读取运行状态，请刷新后重试',
  partial_replicas_running: '仅部分服务正在运行，请先处理运行态异常',
  stack_services_have_mixed_states: 'Stack 内服务运行状态不一致',
  stack_archived: '归档 Stack 不可操作',
  stack_contains_archived_service: 'Stack 包含归档服务不可操作',
  dockrev_stack_managed_via_supervisor: '包含 Dockrev 的 Stack 不支持生命周期操作',
  stack_lifecycle_in_progress: 'Stack 生命周期任务正在执行',
  stack_update_in_progress: 'Stack 更新任务正在执行',
  global_update_in_progress: '全局更新任务正在执行',
}

function lifecycleReasonLabel(reason: string | null | undefined): string | undefined {
  if (!reason) return undefined
  return lifecycleReasonLabels[reason] ?? reason
}

function activeOperation(status: string | null | undefined): boolean {
  return status === 'queued' || status === 'running'
}

const STACK_DETAIL_SNAPSHOT_STALE_MS = 60_000

type StackDetailSnapshotPayload = {
  stack: StackDetail
  jobs: JobListItem[]
  policy: StackSettings['autoUpdatePolicy'] | null
}

export function StackDetailPage(props: {
  stackId: string
  onLastScanHint: (lastScan?: string) => void
  onTopActions: (node: ReactNode) => void
}) {
  const { stackId, onLastScanHint, onTopActions } = props
  const { isOnline } = usePwaStatus()
  const confirm = useConfirm()
  const snapshotKey = buildReadonlySnapshotKey('stack-detail', stackId)
  const [stack, setStack] = useState<StackDetail | null>(null)
  const [settings, setSettings] = useState<StackSettings | null>(null)
  const [cachedPolicy, setCachedPolicy] = useState<StackSettings['autoUpdatePolicy'] | null>(null)
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [settingsDrawerOpen, setSettingsDrawerOpen] = useState(false)
  const [autoPolicyDraft, setAutoPolicyDraft] = useState(() => createDefaultAutoUpdatePolicy('override'))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [lifecycleStatus, setLifecycleStatus] = useState<ServiceLifecycleStatusResponse | null>(null)
  const [lifecycleSubmitting, setLifecycleSubmitting] = useState(false)
  const lifecycleActiveJobIdRef = useRef<string | null>(null)
  const [, setSnapshotStatus] = useState<'missing' | 'fresh' | 'stale' | 'expired' | 'unsupported'>(
    'missing',
  )
  const [snapshotFetchedAt, setSnapshotFetchedAt] = useState<string | null>(null)
  const [snapshotAnchorFetchedAt, setSnapshotAnchorFetchedAt] = useState<string | null>(null)
  const [snapshotActive, setSnapshotActive] = useState(false)

  const refresh = useCallback(async () => {
    setError(null)
    onLastScanHint(undefined)
    const [stackRes, settingsRes, jobsRes] = await Promise.all([
      getStack(stackId),
      getStackSettings(stackId),
      listJobs().catch(() => []),
    ])
    setStack(stackRes)
    setSettings(settingsRes)
    setCachedPolicy(settingsRes.autoUpdatePolicy ?? null)
    setJobs(jobsRes)
    setSnapshotActive(false)
    setSnapshotAnchorFetchedAt(null)
  }, [onLastScanHint, stackId])

  const refreshLifecycleStatus = useCallback(async () => {
    const next = await getStackLifecycleStatus(stackId)
    setLifecycleStatus(next)
    return next
  }, [stackId])

  const refreshAll = useCallback(async () => {
    await Promise.all([
      refresh(),
      refreshLifecycleStatus().catch(() => undefined),
    ])
  }, [refresh, refreshLifecycleStatus])

  const activeLifecycleJob = lifecycleStatus?.activeJob && activeOperation(lifecycleStatus.activeJob.status)
    ? lifecycleStatus.activeJob
    : null
  const activeLifecycleJobId = activeLifecycleJob?.id ?? null
  const activeLifecycleJobType = activeLifecycleJob?.type ?? null
  const activeLifecycleJobStatus = activeLifecycleJob?.status ?? null
  const activeLifecycleJobAction = activeLifecycleJob?.action ?? null
  const stackArchived = Boolean(stack?.archived)
  const lifecycleStatusLoading = lifecycleStatus === null

  const requestLifecycleAction = useCallback((action: ServiceLifecycleAction) => {
    void (async () => {
      if (!stack) return
      if (activeLifecycleJobId && activeLifecycleJobType === 'stack_lifecycle' && activeLifecycleJobStatus && activeOperation(activeLifecycleJobStatus)) {
        navigate({ name: 'job', jobId: activeLifecycleJobId })
        return
      }
      if (action !== 'start') {
        const actionLabel = action === 'stop' ? '停止' : '重启'
        const ok = await confirm({
          title: `确认${actionLabel} Stack ${stack.name}？`,
          body: <div className="modalLead">该操作会立即影响 Stack 内的 {stack.services.length} 个服务。</div>,
          confirmText: actionLabel,
          cancelText: '取消',
          confirmVariant: action === 'stop' ? 'danger' : 'primary',
          badgeText: null,
        })
        if (!ok) return
      }
      setLifecycleSubmitting(true)
      setError(null)
      try {
        const result = await triggerStackLifecycle(stack.id, action)
        lifecycleActiveJobIdRef.current = result.jobId
        setLifecycleStatus((previous) => ({
          state: previous?.state ?? (action === 'start' ? 'stopped' : 'running'),
          activeJob: { id: result.jobId, type: 'stack_lifecycle', status: 'queued', action },
          unavailableReason: null,
        }))
        publishServiceTreeRefresh({ stackId, reason: 'lifecycle-job-started' })
        const refreshed = await refreshLifecycleStatus().catch(() => null)
        if (refreshed && !refreshed.activeJob) {
          setLifecycleStatus((previous) => previous?.activeJob?.id === result.jobId ? { ...refreshed, activeJob: previous.activeJob } : refreshed)
        }
      } catch (e: unknown) {
        if (e instanceof ApiError && e.status === 409) {
          const details = e.details && typeof e.details === 'object' ? e.details as Record<string, unknown> : null
          const existingJobId = typeof details?.existingJobId === 'string' ? details.existingJobId : null
          if (existingJobId) navigate({ name: 'job', jobId: existingJobId })
          else setError(e.message)
          await refreshLifecycleStatus().catch(() => undefined)
        } else {
          setError(errorMessage(e))
        }
      } finally {
        setLifecycleSubmitting(false)
      }
    })()
  }, [activeLifecycleJobId, activeLifecycleJobStatus, activeLifecycleJobType, confirm, refreshLifecycleStatus, stack, stackId])

  useEffect(() => {
    let cancelled = false
    void (async () => {
      const snapshot = await readReadonlySnapshot<StackDetailSnapshotPayload>(snapshotKey)
      if (cancelled) return
      setSnapshotStatus(snapshot.status)
      setSnapshotFetchedAt(snapshot.record?.fetchedAt ?? null)
      setSnapshotAnchorFetchedAt(snapshot.record?.fetchedAt ?? null)
      if (snapshot.status !== 'fresh') return
      setStack(snapshot.record.payload.stack)
      setJobs(snapshot.record.payload.jobs)
      setCachedPolicy(snapshot.record.payload.policy ?? null)
      setSnapshotActive(true)
    })()
    return () => {
      cancelled = true
    }
  }, [snapshotKey])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(errorMessage(e)))
  }, [refresh])

  useEffect(() => {
    lifecycleActiveJobIdRef.current = null
    setLifecycleStatus(null)
  }, [stackId])

  useEffect(() => {
    let cancelled = false
    let timer: number | null = null
    const refreshStatus = async () => {
      try {
        const next = await getStackLifecycleStatus(stackId)
        if (cancelled) return
        const previousActiveJobId = lifecycleActiveJobIdRef.current
        const nextActiveJobId = next.activeJob?.id ?? null
        lifecycleActiveJobIdRef.current = nextActiveJobId
        setLifecycleStatus(next)
        if (previousActiveJobId && !nextActiveJobId) {
          await refresh().catch(() => undefined)
          publishServiceTreeRefresh({ stackId, reason: 'lifecycle-job-settled' })
        }
        if (nextActiveJobId) timer = window.setTimeout(() => void refreshStatus(), 1200)
      } catch {
        if (cancelled) return
        setLifecycleStatus((previous) => ({
          state: 'unknown',
          unavailableReason: 'lifecycle_status_unavailable',
          activeJob: previous?.activeJob ?? null,
        }))
        if (isOnline) timer = window.setTimeout(() => void refreshStatus(), 2400)
      }
    }
    void refreshStatus()
    return () => {
      cancelled = true
      if (timer != null) window.clearTimeout(timer)
    }
  }, [isOnline, lifecycleStatus?.activeJob?.id, refresh, stackId])

  useEffect(() => {
    if (!stack) return
    void writeReadonlySnapshot(
      snapshotKey,
      {
        stack,
        jobs,
        policy: settings?.autoUpdatePolicy ?? cachedPolicy ?? null,
      },
      {
        staleAfterMs: STACK_DETAIL_SNAPSHOT_STALE_MS,
        fetchedAt: snapshotAnchorFetchedAt ? Date.parse(snapshotAnchorFetchedAt) || undefined : undefined,
      },
    )
  }, [cachedPolicy, jobs, settings?.autoUpdatePolicy, snapshotAnchorFetchedAt, snapshotKey, stack])

  useEffect(() => {
    const lifecycleJob = activeLifecycleJobType === 'stack_lifecycle' && activeLifecycleJobId && activeLifecycleJobStatus
      ? { id: activeLifecycleJobId, action: activeLifecycleJobAction, status: activeLifecycleJobStatus }
      : null
    const hasOtherActiveJob = Boolean(activeLifecycleJobId && activeLifecycleJobType !== 'stack_lifecycle')
    const lifecycleState = lifecycleStatus?.state ?? 'unknown'
    const lifecycleReason = lifecycleSubmitting
      ? '操作正在提交'
      : !isOnline
        ? '离线时无法操作 Stack'
        : stackArchived
          ? '归档 Stack 不可操作'
      : lifecycleReasonLabel(lifecycleStatus?.unavailableReason ?? (lifecycleStatusLoading ? 'lifecycle_status_loading' : null))
    const lifecycleItems = (['start', 'stop', 'restart'] as ServiceLifecycleAction[]).map((action) => {
      const compatible = (action === 'start' && lifecycleState === 'stopped') ||
        ((action === 'stop' || action === 'restart') && lifecycleState === 'running')
      const activeAction = Boolean(lifecycleJob && lifecycleJob.action === action)
      const icon = action === 'start' ? Play : action === 'stop' ? Square : RotateCw
      return {
        id: `lifecycle-${action}`,
        label: action === 'start' ? '启动' : action === 'stop' ? '停止' : '重启',
        icon,
        iconVariant: action === 'start' || action === 'stop' ? 'solid' as const : undefined,
        description: lifecycleJob
          ? activeAction ? '任务进行中，点击查看任务详情' : '其他生命周期任务进行中'
          : hasOtherActiveJob ? lifecycleReasonLabel(lifecycleStatus?.unavailableReason) ?? '其他任务正在执行'
            : lifecycleReason ?? (compatible ? undefined : '当前 Stack 状态不支持该操作'),
        disabled: lifecycleJob ? !activeAction : hasOtherActiveJob || Boolean(lifecycleReason) || busy || lifecycleSubmitting || !compatible,
        onSelect: () => lifecycleJob && activeAction ? navigate({ name: 'job', jobId: lifecycleJob.id }) : requestLifecycleAction(action),
        loading: Boolean(lifecycleJob && activeAction),
        loadingClickable: Boolean(lifecycleJob && activeAction),
      }
    })
    const lifecyclePrimary = lifecycleJob
      ? { ...lifecycleItems.find((item) => item.id === `lifecycle-${lifecycleJob.action ?? 'restart'}`)!, label: lifecycleJob.status === 'queued' ? '操作排队中…' : '操作进行中…', disabled: false, loading: true, loadingClickable: true }
      : lifecycleState === 'stopped' ? lifecycleItems[0] : lifecycleItems[1]
    const lifecycleGroupDisabledReason = lifecycleJob
      ? undefined
      : hasOtherActiveJob
        ? lifecycleReasonLabel(lifecycleStatus?.unavailableReason) ?? '其他任务正在执行'
        : lifecycleSubmitting
          ? '操作正在提交'
          : !isOnline
            ? '离线时无法操作 Stack'
            : busy
              ? '正在保存 Stack 设置'
              : undefined
    onTopActions(
      <>
        <div className="serviceDesktopActions">
          <ServiceSplitActionButton
            ariaLabel="Stack 生命周期"
            disabled={Boolean(lifecycleGroupDisabledReason)}
            disabledReason={lifecycleGroupDisabledReason}
            items={lifecycleItems}
            primary={lifecyclePrimary}
          />
          <Button disabled={busy} onClick={() => navigate({ name: 'services' })}>返回服务</Button>
          <Button disabled={busy || !isOnline} onClick={() => void refreshAll()}>刷新</Button>
        </div>
        <ServiceMobileActionMenu
          ariaLabel="Stack 操作"
          groups={[
            { id: 'lifecycle', items: lifecycleItems },
            { id: 'navigation', items: [
              { id: 'return-services', label: '返回服务', icon: ArrowLeft, disabled: busy, onSelect: () => navigate({ name: 'services' }) },
              { id: 'refresh', label: '刷新', icon: RefreshCw, disabled: busy || !isOnline, description: !isOnline ? '离线时无法刷新' : undefined, onSelect: () => void refreshAll() },
            ] },
          ]}
        />
      </>,
    )
    return () => onTopActions(null)
  }, [activeLifecycleJobAction, activeLifecycleJobId, activeLifecycleJobStatus, activeLifecycleJobType, busy, isOnline, lifecycleStatus?.state, lifecycleStatus?.unavailableReason, lifecycleStatusLoading, lifecycleSubmitting, onTopActions, refresh, refreshAll, requestLifecycleAction, stackArchived])

  if (!stack) {
    if (!isOnline) {
      return (
        <div className="page">
          <ReadonlySnapshotNotice
            tone="bad"
            title="当前没有可用的离线 Stack 数据。"
            detail="请恢复联网后重新加载该页面。"
          />
        </div>
      )
    }
    return <div className="muted">加载中…</div>
  }

  const policy = settings?.autoUpdatePolicy ?? cachedPolicy ?? createDefaultAutoUpdatePolicy('override')
  const updatable = stack.services.filter((service) => serviceRowStatus(service) !== 'ok').length
  const recentUpdateJobs = selectRecentStackUpdateJobs(jobs, stack)
  const stableServices = Math.max(stack.services.length - updatable, 0)

  return (
    <div className="page">
      {snapshotActive ? (
        <ReadonlySnapshotNotice
          tone={!isOnline ? 'warn' : 'info'}
          title={!isOnline ? '当前离线，显示已缓存的 Stack 数据。' : '先显示已缓存的 Stack 数据，后台会继续刷新。'}
          detail="自动更新策略只展示只读摘要；恢复联网后才能编辑并保存。"
          fetchedAt={snapshotFetchedAt}
          actionLabel="重试刷新"
          actionDisabled={!isOnline || busy}
          onAction={() => void refresh()}
        />
      ) : null}
      <section className="detailHeroShell">
        <div className="stackDetailHero detailHeroCard detailHeroCardStack">
          <div className="detailHeroPrimary">
            <div className="detailHeroContext" aria-label="当前 Stack 状态">
              <span>Stack 工作区</span>
              <span className="detailHeroContextDivider" aria-hidden="true">
                /
              </span>
              <span>{stack.archived ? '已归档' : '运行中'}</span>
            </div>
            <div className="svcTitleName detailHeroName">
              <Mono>{stack.name}</Mono>
            </div>
            <div className="muted detailHeroDescription">
              共 <Mono>{stack.services.length}</Mono> 个服务，其中 <Mono>{updatable}</Mono> 项需要关注。
            </div>
          </div>
          <div className="detailHeroAside">
            <div className="detailHeroStatusPanel">
              <div className="detailHeroMetaLabel">运行概况</div>
              <div className="detailHeroStatusHeadline">
                {updatable === 0 ? '当前没有待处理项' : `${updatable} 个服务需要关注`}
              </div>
              <div className="detailHeroStatusCopy">
                稳定 <Mono>{stableServices}</Mono> · 总计 <Mono>{stack.services.length}</Mono>
              </div>
              <div className="detailHeroStatusFoot">
                <Pill tone={stack.archived ? 'warn' : 'info'}>
                  {stack.archived ? 'archived' : stack.compose.type}
                </Pill>
              </div>
            </div>
          </div>
        </div>

        <div className="detailHeroMetaGrid detailHeroMetaGridStack">
          <div className="detailHeroMetaCard">
            <div className="detailHeroMetaLabel">Stack ID</div>
            <div className="detailHeroMetaValue">
              <Mono>{stack.id}</Mono>
            </div>
          </div>
          <div className="detailHeroMetaCard">
            <div className="detailHeroMetaLabel">Compose</div>
            <div className="detailHeroMetaValue">{stack.compose.type}</div>
          </div>
          <div className="detailHeroMetaCard">
            <div className="detailHeroMetaLabel">服务数</div>
            <div className="detailHeroMetaValue">
              <Mono>{stack.services.length}</Mono>
            </div>
          </div>
          <div className="detailHeroMetaCard">
            <div className="detailHeroMetaLabel">需关注</div>
            <div className="detailHeroMetaValue">
              <Mono>{updatable}</Mono>
            </div>
          </div>
        </div>
      </section>

      <div className="settingsSummaryGrid">
        <AutoUpdatePolicyResultCard
          busy={busy}
          onOpenSettings={() => {
            if (!settings || !isOnline) return
            setAutoPolicyDraft(policy)
            setSettingsDrawerOpen(true)
          }}
          policy={policy}
          scope="stack"
        />
        <RecentUpdateRecords jobs={recentUpdateJobs} />
      </div>

      <div className="card" style={{ marginTop: 16 }}>
        <div className="title">服务</div>
        <div className="stackServiceList">
          {stack.services.map((service) => {
            const status = serviceRowStatus(service)
            return (
              <div className="stackServiceRow" key={service.id}>
                <div className="stackServiceCopy">
                  <div className="stackServiceName mono">{service.name}</div>
                  <div className="stackServiceRef muted">{service.image.ref}</div>
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
      <AutoUpdatePolicyDrawer
        busy={busy}
        onChange={setAutoPolicyDraft}
        onOpenChange={setSettingsDrawerOpen}
        onSave={() => {
          void (async () => {
            setBusy(true)
            setError(null)
            try {
              await putStackSettings(stack.id, { autoUpdatePolicy: autoPolicyDraft })
              await refresh()
            } catch (e: unknown) {
              setError(errorMessage(e))
            } finally {
              setBusy(false)
            }
          })()
        }}
        open={settingsDrawerOpen}
        policy={autoPolicyDraft}
        scope="stack"
      />
      {error ? <div className="error">{error}</div> : null}
    </div>
  )
}
