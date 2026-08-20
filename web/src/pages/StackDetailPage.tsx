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
import { AsyncDataRegion, AsyncDataSkeleton } from '../components/AsyncDataRegion'
import type { AsyncDataPhase, AsyncDataSource, AsyncDataTrigger } from '../asyncData'
import { ReadonlySnapshotNotice } from '../components/ReadonlySnapshotNotice'
import { ServiceMobileActionMenu, ServiceSplitActionButton } from '../components/ServiceSplitActionButton'
import { useConfirm } from '../confirm'
import { usePwaStatus } from '../pwaStatus'
import { useManagementEventBatch } from '../managementEvents'
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
  compose_v2_required: '需要 Compose V2+ 才能执行操作',
  stack_has_no_services: 'Stack 内没有服务',
  rollback_in_progress: '回滚任务正在执行',
  service_lifecycle_in_progress: '服务生命周期任务正在执行',
  service_update_in_progress: '服务更新任务正在执行',
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
  version: 2
  readiness: {
    stack: boolean
    jobs: boolean
  }
  committedQueryKey: string
  stack: StackDetail
  jobs: JobListItem[]
  policy: StackSettings['autoUpdatePolicy'] | null
}

function isStackDetailSnapshotPayload(value: unknown, expectedStackId: string): value is StackDetailSnapshotPayload {
  if (!value || typeof value !== 'object') return false
  const payload = value as Record<string, unknown>
  if (payload.version !== 2 || payload.committedQueryKey !== expectedStackId) return false
  if (!payload.readiness || typeof payload.readiness !== 'object') return false
  const readiness = payload.readiness as Record<string, unknown>
  return Boolean(payload.stack) && Array.isArray(payload.jobs) && readiness.stack === true && readiness.jobs === true
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
  const refreshRequestIdRef = useRef(0)
  const lifecycleStatusRequestIdRef = useRef(0)
  const stackIdRef = useRef(stackId)
  stackIdRef.current = stackId
  const [, setSnapshotStatus] = useState<'missing' | 'fresh' | 'stale' | 'expired' | 'unsupported'>(
    'missing',
  )
  const [snapshotFetchedAt, setSnapshotFetchedAt] = useState<string | null>(null)
  const [snapshotAnchorFetchedAt, setSnapshotAnchorFetchedAt] = useState<string | null>(null)
  const [snapshotActive, setSnapshotActive] = useState(false)
  const [snapshotHydrated, setSnapshotHydrated] = useState(false)
  const [liveDataCommitted, setLiveDataCommitted] = useState(false)
  const [loadPhase, setLoadPhase] = useState<AsyncDataPhase>('initial-loading')
  const [loadSource, setLoadSource] = useState<AsyncDataSource>('none')
  const [loadTrigger, setLoadTrigger] = useState<AsyncDataTrigger>('background')
  const [loadError, setLoadError] = useState<string | null>(null)
  const [settingsPhase, setSettingsPhase] = useState<AsyncDataPhase>('initial-loading')
  const [settingsLoadError, setSettingsLoadError] = useState<string | null>(null)
  const [jobsPhase, setJobsPhase] = useState<AsyncDataPhase>('initial-loading')
  const [jobsLoadError, setJobsLoadError] = useState<string | null>(null)
  const [jobsLoaded, setJobsLoaded] = useState(false)
  const stackRef = useRef(stack)
  const settingsRef = useRef(settings)
  const cachedPolicyRef = useRef(cachedPolicy)
  const jobsLoadedRef = useRef(jobsLoaded)
  const coreRefreshRequestIdRef = useRef(0)
  const settingsRefreshRequestIdRef = useRef(0)
  const jobsRefreshRequestIdRef = useRef(0)
  const snapshotActiveRef = useRef(snapshotActive)
  stackRef.current = stack
  settingsRef.current = settings
  cachedPolicyRef.current = cachedPolicy
  jobsLoadedRef.current = jobsLoaded
  snapshotActiveRef.current = snapshotActive

  const refresh = useCallback(async (
    source: AsyncDataSource = 'live',
    trigger: AsyncDataTrigger = 'background',
    domains: readonly ('stack' | 'settings' | 'jobs')[] = ['stack', 'settings', 'jobs'],
  ) => {
    const requestedStackId = stackId
    if (stackIdRef.current !== requestedStackId) return
    const requestId = ++refreshRequestIdRef.current
    const requestedDomains = new Set(domains)
    const coreRequestId = requestedDomains.has('stack') ? ++coreRefreshRequestIdRef.current : null
    const settingsRequestId = requestedDomains.has('settings') ? ++settingsRefreshRequestIdRef.current : null
    const jobsRequestId = requestedDomains.has('jobs') ? ++jobsRefreshRequestIdRef.current : null
    setLoadSource(snapshotActiveRef.current ? 'fresh-snapshot' : source)
    setLoadTrigger(trigger)
    if (coreRequestId !== null) {
      setLoadPhase(stackRef.current?.id === requestedStackId ? 'refreshing' : 'initial-loading')
      setLoadError(null)
    }
    if (settingsRequestId !== null) {
      setSettingsPhase(settingsRef.current || cachedPolicyRef.current ? 'refreshing' : 'initial-loading')
      setSettingsLoadError(null)
    }
    if (jobsRequestId !== null) {
      setJobsPhase(jobsLoadedRef.current ? 'refreshing' : 'initial-loading')
      setJobsLoadError(null)
    }
    setError(null)
    if (coreRequestId !== null) onLastScanHint(undefined)
    const readStack = async (): Promise<boolean> => {
      if (coreRequestId === null) return true
      try {
        const nextStack = await getStack(requestedStackId)
        if (stackIdRef.current !== requestedStackId || coreRequestId !== coreRefreshRequestIdRef.current) return false
        setStack(nextStack)
        setLoadPhase('ready-data')
        setSnapshotActive(false)
        snapshotActiveRef.current = false
        setSnapshotAnchorFetchedAt(null)
        return true
      } catch (reason: unknown) {
        if (stackIdRef.current === requestedStackId && coreRequestId === coreRefreshRequestIdRef.current) {
          setLoadError(errorMessage(reason))
          setLoadPhase('error')
        }
        return false
      }
    }
    const readSettings = async (): Promise<boolean> => {
      if (settingsRequestId === null) return true
      try {
        const nextSettings = await getStackSettings(requestedStackId)
        if (stackIdRef.current !== requestedStackId || settingsRequestId !== settingsRefreshRequestIdRef.current) return false
        setSettings(nextSettings)
        setCachedPolicy(nextSettings.autoUpdatePolicy ?? null)
        setSettingsPhase('ready-data')
        return true
      } catch (reason: unknown) {
        if (stackIdRef.current === requestedStackId && settingsRequestId === settingsRefreshRequestIdRef.current) {
          setSettingsLoadError(errorMessage(reason))
          setSettingsPhase('error')
        }
        return false
      }
    }
    const readJobs = async (): Promise<boolean> => {
      if (jobsRequestId === null) return true
      try {
        const nextJobs = await listJobs()
        if (stackIdRef.current !== requestedStackId || jobsRequestId !== jobsRefreshRequestIdRef.current) return false
        setJobs(nextJobs)
        setJobsLoaded(true)
        setJobsPhase(nextJobs.length === 0 ? 'ready-empty' : 'ready-data')
        return true
      } catch (reason: unknown) {
        if (stackIdRef.current === requestedStackId && jobsRequestId === jobsRefreshRequestIdRef.current) {
          setJobsLoadError(errorMessage(reason))
          setJobsPhase('error')
        }
        return false
      }
    }
    const [coreSucceeded, settingsSucceeded, jobsSucceeded] = await Promise.all([
      readStack(),
      readSettings(),
      readJobs(),
    ])
    if (requestedDomains.size === 3 && coreSucceeded && settingsSucceeded && jobsSucceeded && requestId === refreshRequestIdRef.current) {
      setLiveDataCommitted(true)
    }
  }, [onLastScanHint, stackId])

  const refreshLifecycleStatus = useCallback(async () => {
    const requestedStackId = stackId
    if (stackIdRef.current !== requestedStackId) return null
    const requestId = ++lifecycleStatusRequestIdRef.current
    const next = await getStackLifecycleStatus(requestedStackId)
    if (stackIdRef.current !== requestedStackId || requestId !== lifecycleStatusRequestIdRef.current) return null
    setLifecycleStatus(next)
    return next
  }, [stackId])

  const refreshAll = useCallback(async (trigger: AsyncDataTrigger = 'background') => {
    await Promise.all([
      refresh('live', trigger),
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
      if (!stack || stack.id !== stackId || stackIdRef.current !== stackId) return
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
      if (stack.id !== stackId || stackIdRef.current !== stackId) return
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
      if (snapshot.status !== 'fresh' || !isStackDetailSnapshotPayload(snapshot.record.payload, stackId)) {
        setSnapshotHydrated(true)
        return
      }
      const payload = snapshot.record.payload
      setStack(payload.stack)
      setJobs(payload.jobs)
      setJobsLoaded(true)
      setJobsPhase(payload.jobs.length === 0 ? 'ready-empty' : 'ready-data')
      setCachedPolicy(payload.policy ?? null)
      setSettingsPhase('ready-data')
      setLoadSource('fresh-snapshot')
      setLoadPhase('ready-data')
      setSnapshotActive(true)
      snapshotActiveRef.current = true
      setSnapshotHydrated(true)
    })()
    return () => {
      cancelled = true
    }
  }, [snapshotKey, stackId])

  useEffect(() => {
    if (!snapshotHydrated) return
    void refresh().catch((e: unknown) => setError(errorMessage(e)))
  }, [refresh, snapshotHydrated])

  useEffect(() => {
    lifecycleActiveJobIdRef.current = null
    setLifecycleStatus(null)
    setStack(null)
    setSettings(null)
    setJobs([])
    setJobsLoaded(false)
    setSettingsPhase('initial-loading')
    setJobsPhase('initial-loading')
    setSettingsLoadError(null)
    setJobsLoadError(null)
    setLiveDataCommitted(false)
  }, [stackId])

  useEffect(() => {
    void refreshLifecycleStatus().catch(() => {
      setLifecycleStatus((previous) => ({
        state: 'unknown',
        unavailableReason: 'lifecycle_status_unavailable',
        activeJob: previous?.activeJob ?? null,
      }))
    })
  }, [refreshLifecycleStatus])

  useManagementEventBatch(({ events, resyncRequired }) => {
    const relevant = resyncRequired || events.some((event) =>
      event.entities.some((entity) => entity.entityType === 'stack' && entity.id === stackId) ||
      event.summary.stackId === stackId,
    )
    if (!relevant) return
    void refreshAll()
      .then(() => publishServiceTreeRefresh({ stackId, reason: 'management-event' }))
      .catch((error: unknown) => setError(errorMessage(error)))
  })

  useEffect(() => {
    if (!stack || !liveDataCommitted) return
    void writeReadonlySnapshot(
      snapshotKey,
      {
        version: 2,
        readiness: { stack: true, jobs: true },
        committedQueryKey: stackId,
        stack,
        jobs,
        policy: settings?.autoUpdatePolicy ?? cachedPolicy ?? null,
      },
      {
        staleAfterMs: STACK_DETAIL_SNAPSHOT_STALE_MS,
        fetchedAt: snapshotAnchorFetchedAt ? Date.parse(snapshotAnchorFetchedAt) || undefined : undefined,
      },
    )
  }, [cachedPolicy, jobs, liveDataCommitted, settings?.autoUpdatePolicy, snapshotAnchorFetchedAt, snapshotKey, stack, stackId])

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
              : lifecycleReason ?? undefined
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
          <Button disabled={busy || !isOnline} onClick={() => void refreshAll('user-action')}>刷新</Button>
        </div>
        <ServiceMobileActionMenu
          ariaLabel="Stack 操作"
          groups={[
            { id: 'lifecycle', items: lifecycleItems },
            { id: 'navigation', items: [
              { id: 'return-services', label: '返回服务', icon: ArrowLeft, disabled: busy, onSelect: () => navigate({ name: 'services' }) },
              { id: 'refresh', label: '刷新', icon: RefreshCw, disabled: busy || !isOnline, description: !isOnline ? '离线时无法刷新' : undefined, onSelect: () => void refreshAll('user-action') },
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
    return (
      <div className="page">
        <AsyncDataRegion
          error={loadError ?? error}
          hasData={false}
          label="正在加载 Stack 详情"
          onRetry={() => void refresh('memory', 'user-action')}
          phase={loadPhase}
          skeleton={<AsyncDataSkeleton className="stackDetailLoadingSkeleton" lines={8} />}
        />
      </div>
    )
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
          onAction={() => void refresh('memory', 'user-action')}
        />
      ) : null}
      <AsyncDataRegion
        className="stackDetailData"
        error={loadError}
        hasData
        label="正在刷新 Stack 详情"
        onRetry={() => void refresh('memory', 'user-action')}
        phase={loadPhase}
        source={loadSource}
        trigger={loadTrigger}
      >
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
        <AsyncDataRegion
          className="stackDetailPolicyRegion"
          error={settingsLoadError}
          hasData={settings !== null || cachedPolicy !== null}
          label="正在刷新自动更新策略"
          onRetry={() => void refresh('memory', 'user-action', ['settings'])}
          phase={settingsPhase}
          skeleton={<AsyncDataSkeleton lines={3} />}
          source={loadSource}
          trigger={loadTrigger}
        >
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
        </AsyncDataRegion>
        <AsyncDataRegion
          className="stackDetailJobsRegion"
          error={jobsLoadError}
          hasData={jobsLoaded}
          label="正在刷新最近更新记录"
          onRetry={() => void refresh('memory', 'user-action', ['jobs'])}
          phase={jobsPhase}
          skeleton={<AsyncDataSkeleton lines={3} />}
          source={loadSource}
          trigger={loadTrigger}
        >
          <RecentUpdateRecords jobs={recentUpdateJobs} />
        </AsyncDataRegion>
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
      </AsyncDataRegion>
      {error ? <div className="error">{error}</div> : null}
    </div>
  )
}
