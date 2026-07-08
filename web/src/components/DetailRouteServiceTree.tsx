import { ChevronDown, ChevronRight } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { getStack, listStacks, type Service, type StackDetail, type StackListItem, type StackStatus } from '../api'
import { currentHref, navigate, type Route } from '../routes'
import { Mono } from '../ui'
import { serviceRowStatus, statusLabel } from '../updateStatus'

type DetailRoute = Extract<Route, { name: 'stack' | 'service' }>

type TreeStack = StackListItem & {
  detail: StackDetail | null
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

function serviceSectionLabel(section: Extract<Route, { name: 'service' }>['section'] | undefined): string {
  switch (section) {
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
  const [error, setError] = useState<string | null>(null)
  const [expandedStackIds, setExpandedStackIds] = useState<string[]>([])

  const detailRoute = isDetailRoute(props.route) ? props.route : null
  const activeStackId = detailRoute ? currentStackId(detailRoute) : null
  const activeServiceSection = detailRoute ? currentServiceSection(detailRoute) : undefined

  useEffect(() => {
    if (!activeStackId) return
    setExpandedStackIds((current) => (current.includes(activeStackId) ? current : [...current, activeStackId]))
  }, [activeStackId])

  useEffect(() => {
    let cancelled = false

    const load = async () => {
      setLoading(true)
      setError(null)
      try {
        const stackList = await listStacks()
        const details = await Promise.all(
          stackList.map(async (stack) => {
            try {
              return await getStack(stack.id)
            } catch {
              return null
            }
          }),
        )
        if (cancelled) return
        setStacks(stackList.map((stack, index) => ({ ...stack, detail: details[index] })))
      } catch (value: unknown) {
        if (cancelled) return
        setError(value instanceof Error ? value.message : String(value))
        setStacks([])
      } finally {
        if (!cancelled) setLoading(false)
      }
    }

    void load()
    return () => {
      cancelled = true
    }
  }, [])

  const treeClassName = props.variant === 'mobile' ? 'detailRouteTree detailRouteTreeMobile' : 'detailRouteTree'
  const showState = useMemo(() => loading || Boolean(error) || stacks.length === 0, [error, loading, stacks.length])
  const totalServices = useMemo(() => stacks.reduce((sum, stack) => sum + stack.services, 0), [stacks])
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
    const title = `${service.name} · ${statusLabel(rowStatus)}`

    return (
      <a
        key={service.id}
        href={currentHref(nextRoute)}
        className={active ? 'detailRouteServiceLink detailRouteServiceLinkActive' : 'detailRouteServiceLink'}
        aria-current={active ? 'page' : undefined}
        title={title}
        onClick={(event) => {
          event.preventDefault()
          navigate(nextRoute)
        }}
      >
        <span className={`detailRouteStatusDot detailRouteStatusDot-${rowStatus}`} aria-hidden="true" />
        <span className="detailRouteServiceName">{service.name}</span>
        <span className="detailRouteServiceMeta">
          <Mono>{serviceVersionLabel(service)}</Mono>
        </span>
      </a>
    )
  }

  return (
    <div className={treeClassName}>
      <div className="detailRouteTreeIntro">
        <div className="detailRouteTreeTitleRow">
          <div className="detailRouteTreeTitle">服务导航</div>
          {!showState ? (
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

      {showState ? (
        <div className="detailRouteTreeState">
          {loading ? <div className="muted">加载服务列表…</div> : null}
          {!loading && error ? <div className="muted">服务树暂不可用：{error}</div> : null}
          {!loading && !error && stacks.length === 0 ? <div className="muted">暂无可导航的 Stack</div> : null}
        </div>
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
                </div>
                {expanded ? (
                  <div className="detailRouteServiceList" role="list" aria-label={`${stack.name} 服务`}>
                    {services.map((service) => renderServiceLink(stack, service))}
                  </div>
                ) : null}
              </section>
            )
          })}
        </div>
      )}
    </div>
  )
}
