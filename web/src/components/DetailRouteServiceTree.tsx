import { ChevronDown, ChevronRight } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { getStack, listStacks, listStacksArchived, type Service, type ServiceLifecycleState, type StackDetail, type StackListItem, type StackStatus } from '../api'
import { currentHref, navigate, type Route } from '../routes'
import { SERVICE_TREE_REFRESH_EVENT, type ServiceTreeRefreshDetail } from '../serviceTreeRefresh'
import { useManagementEventBatch } from '../managementEvents'
import { Mono } from '../ui'
import { UPDATE_JOB_SETTLED_EVENT, type UpdateJobSettledDetail } from '../updateActionTracking'
import { serviceRowStatus, statusLabel } from '../updateStatus'
import { ServiceTreeContextActions } from './ServiceTreeContextActions'
import { AsyncDataRegion, AsyncDataSkeleton } from './AsyncDataRegion'
import type { AsyncDataPhase, AsyncDataTrigger } from '../asyncData'

type DetailRoute = Extract<Route, { name: 'stack' | 'service' }>

type TreeStack = StackListItem & {
  detail: StackDetail | null
  detailStatus: 'idle' | 'loading' | 'loaded' | 'error'
  detailError: string | null
  detailTrigger: AsyncDataTrigger
  detailRevision: number
  detailLoadedRevision: number
}

function isDetailRoute(route: Route): route is DetailRoute {
  return route.name === 'stack' || route.name === 'service'
}

function currentStackId(route: DetailRoute): string {
  return route.stackId
}

function currentServiceSection(route: DetailRoute): Extract<Route, { name: 'service' }>['section'] | undefined {
  return route.name === 'service' ? route.section : undefined
}

function serviceVersionLabel(service: Service): string {
  const resolved = (service.image.resolvedTag ?? '').trim()
  const raw = (service.image.tag ?? '').trim()
  return resolved || raw || '-'
}

function lifecycleStateLabel(state: ServiceLifecycleState): string {
  switch (state) {
    case 'running':
      return '运行中'
    case 'stopped':
      return '已停止'
    case 'partial':
      return '部分运行'
    default:
      return '未知'
  }
}

function lifecycleStateClassName(state: ServiceLifecycleState): string {
  return `detailRouteStatusDot detailRouteStatusDotLifecycle-${state}`
}

export function serviceSectionLabel(section: Extract<Route, { name: 'service' }>['section'] | undefined): string {
  switch (section) {
    case 'versions':
      return '版本'
    case 'history':
      return '更新记录'
    case 'monitoring':
      return '监控'
    case 'backup':
      return '备份'
    case 'logs':
      return '日志'
    case 'settings':
      return '设置'
    default:
      return '概览'
  }
}

function stackStatusClassName(status: StackStatus): string {
  switch (status) {
    case 'healthy':
      return 'detailRouteStackDot detailRouteStackDotHealthy'
    case 'degraded':
      return 'detailRouteStackDot detailRouteStackDotDegraded'
    default:
      return 'detailRouteStackDot detailRouteStackDotUnknown'
  }
}

export function DetailRouteServiceTree(props: {
  route: Route
  variant: 'desktop' | 'mobile'
}) {
  const [stacks, setStacks] = useState<TreeStack[]>([])
  const [loading, setLoading] = useState(true)
  const [treeLoaded, setTreeLoaded] = useState(false)
  const [treeTrigger, setTreeTrigger] = useState<AsyncDataTrigger>('background')
  const [error, setError] = useState<string | null>(null)
  const [expandedStackIds, setExpandedStackIds] = useState<string[]>([])
  const [detailFetchTick, setDetailFetchTick] = useState(0)
  const inFlightStackIdsRef = useRef<Set<string>>(new Set())
  const stacksRef = useRef<TreeStack[]>([])
  const mountedRef = useRef(true)
  const treeRequestIdRef = useRef(0)
  const treeGenerationRef = useRef(0)
  stacksRef.current = stacks

  const detailRoute = isDetailRoute(props.route) ? props.route : null
  const activeStackId = detailRoute ? currentStackId(detailRoute) : null
  const activeServiceSection = detailRoute ? currentServiceSection(detailRoute) : undefined

  useEffect(() => {
    if (!activeStackId) return
    setExpandedStackIds((current) => (current.includes(activeStackId) ? current : [...current, activeStackId]))
  }, [activeStackId])

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
    }
  }, [])

  const refreshTree = useCallback(async (trigger: AsyncDataTrigger = 'background') => {
    const requestId = ++treeRequestIdRef.current
    treeGenerationRef.current += 1
    setTreeTrigger(trigger)
    setLoading(true)
    setError(null)
    try {
      inFlightStackIdsRef.current.clear()
      const [activeStacks, archivedStacks] = await Promise.all([
        listStacks(),
        listStacksArchived('only').catch(() => []),
      ])
      const stackList = [...activeStacks]
      const seenStackIds = new Set(activeStacks.map((stack) => stack.id))
      for (const stack of archivedStacks) {
        if (seenStackIds.has(stack.id)) continue
        seenStackIds.add(stack.id)
        stackList.push(stack)
      }
      if (!mountedRef.current || requestId !== treeRequestIdRef.current) return
      setStacks(
        stackList.map((stack) => ({
          ...stack,
          detail: null,
          detailStatus: 'idle',
          detailError: null,
          detailTrigger: 'background',
          detailRevision: 0,
          detailLoadedRevision: 0,
        })),
      )
      setTreeLoaded(true)
      setDetailFetchTick((current) => current + 1)
    } catch (value: unknown) {
      if (!mountedRef.current || requestId !== treeRequestIdRef.current) return
      setError(value instanceof Error ? value.message : String(value))
    } finally {
      if (mountedRef.current && requestId === treeRequestIdRef.current) setLoading(false)
    }
  }, [])

  useEffect(() => {
    const expectedTreeGeneration = treeGenerationRef.current
    const pendingStackIds = expandedStackIds.filter((stackId) =>
      stacksRef.current.some((stack) =>
        stack.id === stackId &&
        (stack.detailStatus === 'idle' || stack.detailRevision > stack.detailLoadedRevision),
      ) && !inFlightStackIdsRef.current.has(stackId),
    )
    if (pendingStackIds.length === 0) return

    const pendingStackIdSet = new Set(pendingStackIds)
    const requestedRevisions = new Map(
      stacksRef.current
        .filter((stack) => pendingStackIdSet.has(stack.id))
        .map((stack) => [stack.id, stack.detailRevision]),
    )
    for (const stackId of pendingStackIds) inFlightStackIdsRef.current.add(stackId)

    setStacks((current) =>
      current.map((stack) =>
        pendingStackIdSet.has(stack.id) ? { ...stack, detailStatus: 'loading', detailError: null } : stack,
      ),
    )

    void Promise.all(
      pendingStackIds.map(async (stackId) => {
        try {
          const detail = await getStack(stackId)
          return {
            stackId,
            revision: requestedRevisions.get(stackId) ?? 0,
            detail,
            detailError: null,
            detailStatus: 'loaded' as const,
          }
        } catch (error: unknown) {
          return {
            stackId,
            revision: requestedRevisions.get(stackId) ?? 0,
            detail: null,
            detailError: error instanceof Error ? error.message : String(error),
            detailStatus: 'error' as const,
          }
        }
      }),
    ).then((results) => {
      for (const result of results) inFlightStackIdsRef.current.delete(result.stackId)
      if (!mountedRef.current || expectedTreeGeneration !== treeGenerationRef.current) return
      const byId = new Map(results.map((result) => [result.stackId, result]))
      setStacks((current) =>
        current.map((stack) => {
          const next = byId.get(stack.id)
          if (!next) return stack
          return {
            ...stack,
            detail: next.detail ?? stack.detail,
            detailError: next.detailError ?? null,
            detailStatus: next.detailStatus,
            detailLoadedRevision: Math.max(stack.detailLoadedRevision, next.revision),
          }
        }),
      )
    })
  }, [detailFetchTick, expandedStackIds])

  useEffect(() => {
    void refreshTree()
  }, [refreshTree])

  const requestStackRefresh = useCallback((stackId: string, trigger: AsyncDataTrigger = 'background') => {
    setStacks((current) =>
      current.map((stack) =>
        stack.id === stackId ? { ...stack, detailRevision: stack.detailRevision + 1, detailTrigger: trigger } : stack,
      ),
    )
    setDetailFetchTick((current) => current + 1)
  }, [])

  useEffect(() => {
    const matchesStack = (stackId: string | null | undefined) =>
      Boolean(stackId && expandedStackIds.includes(stackId))

    const onRefresh = (event: Event) => {
      const detail = event instanceof CustomEvent ? (event.detail as ServiceTreeRefreshDetail | null) : null
      if (detail?.stackId && matchesStack(detail.stackId)) requestStackRefresh(detail.stackId)
    }
    const onUpdateSettled = (event: Event) => {
      const detail = event instanceof CustomEvent ? (event.detail as UpdateJobSettledDetail | null) : null
      if (!detail) return
      if (detail.scope === 'all' || detail.target === 'all') {
        for (const stackId of expandedStackIds) requestStackRefresh(stackId)
        return
      }
      const stackId = detail.stackId ?? (detail.target.startsWith('stack:') ? detail.target.slice(6) : null)
      if (stackId && matchesStack(stackId)) requestStackRefresh(stackId)
    }

    window.addEventListener(SERVICE_TREE_REFRESH_EVENT, onRefresh)
    window.addEventListener(UPDATE_JOB_SETTLED_EVENT, onUpdateSettled)
    return () => {
      window.removeEventListener(SERVICE_TREE_REFRESH_EVENT, onRefresh)
      window.removeEventListener(UPDATE_JOB_SETTLED_EVENT, onUpdateSettled)
    }
  }, [expandedStackIds, requestStackRefresh])

  useManagementEventBatch(({ events, resyncRequired }) => {
    if (resyncRequired) {
      for (const stackId of expandedStackIds) requestStackRefresh(stackId)
      return
    }
    for (const event of events) {
      const stackIds = event.entities
        .filter((entity) => entity.entityType === 'stack')
        .map((entity) => entity.id)
      if (typeof event.summary.stackId === 'string') stackIds.push(event.summary.stackId)
      for (const stackId of stackIds) {
        if (expandedStackIds.includes(stackId)) requestStackRefresh(stackId)
      }
    }
  })

  const treeClassName = props.variant === 'mobile' ? 'detailRouteTree detailRouteTreeMobile' : 'detailRouteTree'
  const treePhase: AsyncDataPhase = error
    ? 'error'
    : loading
      ? treeLoaded ? 'refreshing' : 'initial-loading'
      : stacks.length === 0 ? 'ready-empty' : 'ready-data'
  const totalServices = useMemo(
    () => stacks.reduce((sum, stack) => sum + (stack.detail?.services.length ?? stack.services), 0),
    [stacks],
  )
  const activeStack = useMemo(
    () => (activeStackId ? stacks.find((stack) => stack.id === activeStackId) ?? null : null),
    [activeStackId, stacks],
  )
  const activeService = useMemo(() => {
    if (!detailRoute || detailRoute.name !== 'service' || !activeStack?.detail) return null
    return activeStack.detail.services.find((service) => service.id === detailRoute.serviceId) ?? null
  }, [activeStack, detailRoute])

  const renderServiceLink = (stack: TreeStack, service: Service) => {
    const active = props.route.name === 'service' && props.route.stackId === stack.id && props.route.serviceId === service.id
    const nextRoute: Route = {
      name: 'service',
      stackId: stack.id,
      serviceId: service.id,
      section: activeServiceSection,
    }
    const rowStatus = serviceRowStatus(service)
    const lifecycleState = service.lifecycleState ?? 'unknown'
    const version = serviceVersionLabel(service)
    const updateLabel = rowStatus === 'updatable' ? '有可用更新' : statusLabel(rowStatus)
    const title = `${service.name} · ${lifecycleStateLabel(lifecycleState)} · 当前版本 ${version} · ${updateLabel}`

    return (
      <ServiceTreeContextActions
        key={service.id}
        target={{ kind: 'service', stackId: stack.id, service }}
        onRefresh={requestStackRefresh}
      >
        <a
        href={currentHref(nextRoute)}
        className={active ? 'detailRouteServiceLink detailRouteServiceLinkActive' : 'detailRouteServiceLink'}
        aria-current={active ? 'page' : undefined}
        aria-label={title}
        title={title}
        onClick={(event) => {
          event.preventDefault()
          navigate(nextRoute)
        }}
        >
          <span className={lifecycleStateClassName(lifecycleState)} aria-hidden="true" />
          <span className="detailRouteServiceName">{service.name}</span>
          <span className="detailRouteServiceMeta" aria-label={`当前版本 ${version}`}>
            <Mono>{serviceVersionLabel(service)}</Mono>
            {rowStatus === 'updatable' ? <span className="detailRouteServiceUpdateDot" aria-label="有可用更新" /> : null}
          </span>
        </a>
      </ServiceTreeContextActions>
    )
  }

  return (
    <div className={treeClassName}>
      <div className="detailRouteTreeIntro">
        <div className="detailRouteTreeTitleRow">
          <div className="detailRouteTreeTitle">服务导航</div>
          {treePhase === 'ready-data' ? (
            <div className="detailRouteTreeMeta" aria-label="导航统计">
              <Mono>{stacks.length}</Mono>
              <span>个 Stack</span>
              <span className="detailRouteTreeMetaDivider" aria-hidden="true">
                ·
              </span>
              <Mono>{totalServices}</Mono>
              <span>个服务</span>
            </div>
          ) : null}
        </div>
        <div className="detailRouteTreePath" aria-label="当前导航路径">
          {detailRoute ? (
            <>
              <span className="detailRouteTreePathLabel">当前</span>
              <span>{activeStack?.name ?? detailRoute.stackId}</span>
              {activeService ? <span className="detailRouteTreePathDivider">/</span> : null}
              {activeService ? <span>{activeService.name}</span> : null}
              {props.route.name === 'service' ? <span className="detailRouteTreePathDivider">/</span> : null}
              {props.route.name === 'service' ? <span>{serviceSectionLabel(activeServiceSection)}</span> : null}
            </>
          ) : (
            <span>按 Stack 浏览，并直接切换到目标服务。</span>
          )}
        </div>
      </div>

      <AsyncDataRegion
        className="detailRouteTreeAsync"
        error={error}
        hasData={treeLoaded}
        label="正在刷新服务导航"
        onRetry={() => void refreshTree('user-action')}
        phase={treePhase}
        skeleton={<AsyncDataSkeleton className="detailRouteTreeLoadingSkeleton" lines={6} />}
        trigger={treeTrigger}
      >
      {treePhase === 'ready-empty' ? (
        <div className="detailRouteTreeState"><div className="muted">暂无可导航的 Stack</div></div>
      ) : (
        <div className="detailRouteTreeList">
          {stacks.map((stack) => {
            const expanded = expandedStackIds.includes(stack.id)
            const stackActive = props.route.name === 'stack' && props.route.stackId === stack.id
            const stackCurrent = activeStackId === stack.id
            const services = stack.detail?.services ?? []
            const serviceCount = stack.detail?.services.length ?? stack.services

            return (
              <section className="detailRouteStackGroup" key={stack.id}>
                <div className="detailRouteStackRow">
                  <button
                    type="button"
                    className="detailRouteStackToggle"
                    aria-label={expanded ? `收起 ${stack.name}` : `展开 ${stack.name}`}
                    aria-expanded={expanded}
                    onClick={() =>
                      setExpandedStackIds((current) =>
                        current.includes(stack.id) ? current.filter((id) => id !== stack.id) : [...current, stack.id],
                      )
                    }
                  >
                    {expanded ? <ChevronDown size={15} strokeWidth={2.2} /> : <ChevronRight size={15} strokeWidth={2.2} />}
                  </button>
                  <ServiceTreeContextActions
                    target={{ kind: 'stack', stackId: stack.id, stack: stack.detail, archived: stack.archived }}
                    onRefresh={requestStackRefresh}
                  >
                    <a
                      href={currentHref({ name: 'stack', stackId: stack.id })}
                      className={
                        stackActive
                          ? 'detailRouteStackLink detailRouteStackLinkActive'
                          : stackCurrent
                            ? 'detailRouteStackLink detailRouteStackLinkCurrent'
                            : 'detailRouteStackLink'
                      }
                      aria-current={stackActive ? 'page' : undefined}
                      onClick={(event) => {
                        event.preventDefault()
                        navigate({ name: 'stack', stackId: stack.id })
                      }}
                    >
                      <span className="detailRouteStackTitle">
                        <span className={stackStatusClassName(stack.status)} aria-hidden="true" />
                        <span className="detailRouteStackLabel">{stack.name}</span>
                      </span>
                      <span className="detailRouteStackMeta">
                        <Mono>{serviceCount}</Mono>
                      </span>
                    </a>
                  </ServiceTreeContextActions>
                </div>
                {expanded ? (
                  <AsyncDataRegion
                    className="detailRouteServiceList"
                    error={stack.detailError}
                    hasData={stack.detail !== null}
                    label="正在刷新服务列表"
                    onRetry={() => requestStackRefresh(stack.id, 'user-action')}
                    phase={
                      stack.detailStatus === 'error'
                        ? 'error'
                        : stack.detailStatus === 'loading'
                          ? stack.detail ? 'refreshing' : 'initial-loading'
                          : services.length === 0 ? 'ready-empty' : 'ready-data'
                    }
                    skeleton={<AsyncDataSkeleton className="detailRouteServiceLoadingSkeleton" lines={3} />}
                    trigger={stack.detailTrigger}
                  >
                    {stack.detailStatus === 'loaded' && services.length === 0 ? <div className="muted">暂无服务</div> : null}
                    {(stack.detailStatus === 'loaded' || stack.detailStatus === 'error')
                      ? services.map((service) => renderServiceLink(stack, service))
                      : null}
                  </AsyncDataRegion>
                ) : null}
              </section>
            )
          })}
        </div>
      )}
      </AsyncDataRegion>
    </div>
  )
}
