import { Download, Play, RotateCw, Square } from 'lucide-react'
import { cloneElement, type HTMLAttributes, type KeyboardEvent, type ReactElement, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  ApiError,
  getServiceLifecycleStatus,
  getStack,
  getStackLifecycleStatus,
  triggerServiceLifecycle,
  triggerStackLifecycle,
  triggerUpdate,
  type Service,
  type ServiceLifecycleAction,
  type ServiceLifecycleStatusResponse,
  type StackDetail,
} from '../api'
import { partitionAggregateUpdateServices } from '../aggregateUpdateGuard'
import { selfUpgradeBaseUrl, isDockrevImageRef } from '../runtimeConfig'
import { navigate } from '../routes'
import { buildUpdateServiceTarget, buildUpdateServiceTargets } from '../updateTargets'
import { openSelfUpgradeUrl } from '../pages/serviceDetailUtils'
import { useConfirm } from '../confirm'
import { UPDATE_JOB_SETTLED_EVENT, resolveUpdateActionTargetKey, useUpdateActionTracker, type UpdateJobSettledDetail } from '../updateActionTracking'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from './ui/context-menu'
import { Toast, ToastProvider, ToastTitle, ToastViewport } from './ui/toast'

type ContextTarget =
  | { kind: 'stack'; stackId: string; stack?: StackDetail | null; archived?: boolean }
  | { kind: 'service'; stackId: string; service: Service }

type Notice = { id: number; message: string; jobId?: string }

function ActionLabel(props: { label: string; reason?: string }) {
  return (
    <span className="serviceTreeContextActionLabel">
      <span>{props.label}</span>
      {props.reason ? <span className="serviceTreeContextReason">{props.reason}</span> : null}
    </span>
  )
}

const reasonLabels: Record<string, string> = {
  lifecycle_status_loading: '正在读取实时状态',
  lifecycle_status_unavailable: '暂时无法读取运行状态',
  partial_replicas_running: '仅部分副本正在运行',
  stack_services_have_mixed_states: 'Stack 内服务运行状态不一致',
  container_missing_for_compose_v1: 'Compose V1 未找到已有容器',
  stack_has_no_services: 'Stack 内没有服务',
  stack_archived: '归档 Stack 不可操作',
  stack_contains_archived_service: 'Stack 包含归档服务不可操作',
  service_archived: '归档服务不可操作',
  dockrev_service_managed_via_supervisor: 'Dockrev 服务由 Supervisor 管理',
  dockrev_stack_managed_via_supervisor: '包含 Dockrev 的 Stack 不支持生命周期操作',
  rollback_in_progress: '回滚任务正在执行',
  service_lifecycle_in_progress: '服务生命周期任务正在执行',
  stack_lifecycle_in_progress: 'Stack 生命周期任务正在执行',
  service_update_in_progress: '服务更新任务正在执行',
  stack_update_in_progress: 'Stack 更新任务正在执行',
  global_update_in_progress: '全局更新任务正在执行',
  no_update_candidate: '没有可用更新',
  update_ignored: '更新已被忽略规则阻止',
  architecture_mismatch: '候选镜像架构不匹配',
}

function reasonLabel(reason: string | null | undefined): string | undefined {
  if (!reason) return undefined
  return reasonLabels[reason] ?? reason
}

function errorNotice(error: unknown): Notice {
  if (error instanceof ApiError) {
    const details = error.details && typeof error.details === 'object' ? error.details as Record<string, unknown> : null
    const jobId = typeof details?.existingJobId === 'string' ? details.existingJobId : undefined
    const reason = typeof details?.reason === 'string' ? reasonLabel(details.reason) : undefined
    return { id: Date.now(), message: reason ?? error.message, jobId }
  }
  return { id: Date.now(), message: error instanceof Error ? error.message : String(error) }
}

function updateDisabledReason(target: ContextTarget): string | null {
  if (target.kind === 'stack') {
    if (target.archived || target.stack?.archived) return '归档 Stack 不可更新'
    if (!target.stack) return '正在加载 Stack 更新状态'
    const partition = partitionAggregateUpdateServices(target.stack.services)
    return partition.actionable.length > 0 ? null : '没有可执行的更新候选'
  }
  const service = target.service
  if (service.archived) return '归档服务不可更新'
  if (isDockrevImageRef(service.image.ref)) return null
  if (service.ignore?.matched) return service.ignore.reason || reasonLabels.update_ignored
  if (!service.candidate) return reasonLabels.no_update_candidate
  if (service.candidate.archMatch === 'mismatch') return reasonLabels.architecture_mismatch
  return null
}

export function ServiceTreeContextActions(props: {
  target: ContextTarget
  children: ReactElement<HTMLAttributes<HTMLElement>>
  onRefresh: (stackId: string) => void
}) {
  const confirm = useConfirm()
  const { trackJob } = useUpdateActionTracker()
  const trackedJobIdRef = useRef<string | null>(null)
  const [status, setStatus] = useState<ServiceLifecycleStatusResponse | null>(null)
  const [stackDetail, setStackDetail] = useState<StackDetail | null>(props.target.kind === 'stack' ? props.target.stack ?? null : null)
  const [serviceDetail, setServiceDetail] = useState<Service | null>(props.target.kind === 'service' ? props.target.service : null)
  const [notice, setNotice] = useState<Notice | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const target = useMemo(
    () => props.target.kind === 'stack'
      ? { ...props.target, stack: stackDetail }
      : { ...props.target, service: serviceDetail ?? props.target.service },
    [props.target, serviceDetail, stackDetail],
  )
  const lifecycleState = status?.state ?? 'unknown'
  const lifecycleReason = submitting
    ? '操作正在提交'
    : target.kind === 'service' && target.service.archived
      ? '归档服务不可操作'
    : reasonLabel(status?.unavailableReason ?? (!status ? 'lifecycle_status_loading' : null))
  const lifecycleDisabled = Boolean(lifecycleReason)
  const showStart = lifecycleState === 'stopped'
  const updateReason = updateDisabledReason(target)
  const isDockrevService = target.kind === 'service' && isDockrevImageRef(target.service.image.ref)
  const refreshStack = props.onRefresh
  const targetStackId = props.target.stackId

  useEffect(() => {
    const onSettled = (event: Event) => {
      const detail = event instanceof CustomEvent ? event.detail as UpdateJobSettledDetail | null : null
      if (!detail || detail.jobId !== trackedJobIdRef.current) return
      trackedJobIdRef.current = null
      refreshStack(targetStackId)
    }
    window.addEventListener(UPDATE_JOB_SETTLED_EVENT, onSettled)
    return () => window.removeEventListener(UPDATE_JOB_SETTLED_EVENT, onSettled)
  }, [refreshStack, targetStackId])

  const loadState = useCallback(async () => {
    setStatus(null)
    try {
      const contextTarget = props.target
      if (contextTarget.kind === 'stack') {
        const [nextStatus, detail] = await Promise.all([
          getStackLifecycleStatus(contextTarget.stackId),
          getStack(contextTarget.stackId),
        ])
        setStatus(nextStatus)
        setStackDetail(detail)
      } else {
        const [nextStatus, detail] = await Promise.all([
          getServiceLifecycleStatus(contextTarget.service.id),
          getStack(contextTarget.stackId),
        ])
        setStatus(nextStatus)
        setServiceDetail(detail.services.find((service) => service.id === contextTarget.service.id) ?? contextTarget.service)
      }
    } catch (error) {
      setStatus({ state: 'unknown', unavailableReason: errorNotice(error).message })
    }
  }, [props.target])

  const submitLifecycle = useCallback(async (action: ServiceLifecycleAction) => {
    if (props.target.kind === 'stack' && action !== 'start') {
      const detail = stackDetail ?? props.target.stack
      if (!detail) {
        setNotice({ id: Date.now(), message: 'Stack 信息仍在加载，请稍后重试' })
        return
      }
      const actionLabel = action === 'stop' ? '停止' : '重启'
      const ok = await confirm({
        title: `确认${actionLabel} Stack ${detail.name}？`,
        body: <div className="modalLead">该操作会立即影响 Stack 内的 {detail.services.length} 个服务。</div>,
        confirmText: actionLabel,
        cancelText: '取消',
        confirmVariant: action === 'stop' ? 'danger' : 'primary',
        badgeText: null,
      })
      if (!ok) return
    }
    setSubmitting(true)
    try {
      const result = props.target.kind === 'stack'
        ? await triggerStackLifecycle(props.target.stackId, action)
        : await triggerServiceLifecycle(props.target.service.id, action)
      const targetKey = resolveUpdateActionTargetKey(
        props.target.kind === 'stack' ? 'stack' : 'service',
        props.target.stackId,
        props.target.kind === 'service' ? props.target.service.id : undefined,
      )
      if (targetKey) {
        trackedJobIdRef.current = result.jobId
        trackJob(targetKey, result.jobId, 'queued')
      }
      setNotice({ id: Date.now(), message: `${action === 'start' ? '启动' : action === 'stop' ? '停止' : '重启'}任务已创建`, jobId: result.jobId })
      props.onRefresh(props.target.stackId)
    } catch (error) {
      setNotice(errorNotice(error))
    } finally {
      setSubmitting(false)
    }
  }, [confirm, props, stackDetail, trackJob])

  const submitUpdate = useCallback(async () => {
    if (target.kind === 'service' && isDockrevImageRef(target.service.image.ref)) {
      openSelfUpgradeUrl(selfUpgradeBaseUrl())
      return
    }
    setSubmitting(true)
    try {
      const result = target.kind === 'service'
        ? await triggerUpdate({
            scope: 'service',
            stackId: props.target.stackId,
            ...(await buildUpdateServiceTarget(target.service)),
            mode: 'apply',
            allowArchMismatch: false,
            backupMode: 'inherit',
          })
        : await (async () => {
            const detail = stackDetail ?? await getStack(props.target.stackId)
            const services = partitionAggregateUpdateServices(detail.services).actionable.map((item) => item.svc)
            return triggerUpdate({
              scope: 'stack',
              stackId: props.target.stackId,
              targets: await buildUpdateServiceTargets(services),
              mode: 'apply',
              allowArchMismatch: false,
              backupMode: 'inherit',
            })
          })()
      const targetKey = resolveUpdateActionTargetKey(
        props.target.kind === 'stack' ? 'stack' : 'service',
        props.target.stackId,
        props.target.kind === 'service' ? props.target.service.id : undefined,
      )
      if (targetKey) {
        trackedJobIdRef.current = result.jobId
        trackJob(targetKey, result.jobId, 'queued')
      }
      setNotice({ id: Date.now(), message: '更新任务已创建', jobId: result.jobId })
      props.onRefresh(props.target.stackId)
    } catch (error) {
      setNotice(errorNotice(error))
    } finally {
      setSubmitting(false)
    }
  }, [props, stackDetail, target, trackJob])

  const trigger = useMemo(() => cloneElement(props.children, {
    onKeyDown: (event: KeyboardEvent<HTMLElement>) => {
      props.children.props.onKeyDown?.(event)
      if (event.defaultPrevented || (event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10'))) return
      event.preventDefault()
      const rect = event.currentTarget.getBoundingClientRect()
      event.currentTarget.dispatchEvent(new MouseEvent('contextmenu', {
        bubbles: true,
        clientX: rect.left + Math.min(24, rect.width / 2),
        clientY: rect.top + Math.min(24, rect.height / 2),
      }))
    },
  } as HTMLAttributes<HTMLElement>), [props.children])

  return (
    <ToastProvider duration={4200}>
      <ContextMenu onOpenChange={(open) => { if (open) void loadState() }}>
        <ContextMenuTrigger asChild>{trigger}</ContextMenuTrigger>
        <ContextMenuContent className="serviceTreeContextMenu" aria-label="快捷操作">
          {!isDockrevService ? (
            <>
              {showStart ? (
                <ContextMenuItem disabled={lifecycleDisabled} title={lifecycleReason} onSelect={() => void submitLifecycle('start')}>
                  <Play className="serviceSplitActionIconSolid" /><ActionLabel label="启动" reason={lifecycleReason} />
                </ContextMenuItem>
              ) : (
                <ContextMenuItem disabled={lifecycleDisabled} title={lifecycleReason} onSelect={() => void submitLifecycle('restart')}>
                  <RotateCw /><ActionLabel label="重启" reason={lifecycleReason} />
                </ContextMenuItem>
              )}
              {!showStart ? (
                <ContextMenuItem disabled={lifecycleDisabled} title={lifecycleReason} onSelect={() => void submitLifecycle('stop')}>
                  <Square /><ActionLabel label="停止" reason={lifecycleReason} />
                </ContextMenuItem>
              ) : null}
              <ContextMenuSeparator />
            </>
          ) : null}
          <ContextMenuItem disabled={Boolean(updateReason) || submitting} title={updateReason ?? undefined} onSelect={() => void submitUpdate()}>
            <Download /><ActionLabel label="更新" reason={updateReason ?? (submitting ? '操作正在提交' : undefined)} />
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
      {notice ? (
        <Toast key={notice.id} open onOpenChange={(open) => { if (!open) setNotice(null) }}>
          <ToastTitle className="serviceTreeActionToastTitle">
            <span>{notice.message}</span>
            {notice.jobId ? <button type="button" onClick={() => navigate({ name: 'job', jobId: notice.jobId! })}>查看任务</button> : null}
          </ToastTitle>
        </Toast>
      ) : null}
      <ToastViewport />
    </ToastProvider>
  )
}
