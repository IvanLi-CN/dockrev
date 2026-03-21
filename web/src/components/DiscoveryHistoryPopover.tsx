import { useEffect, useRef, useState } from 'react'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import {
  ApiError,
  getServiceNewVersionDiscoveryTimeline,
  type NewVersionDiscoveryTimelineItem,
} from '../api'
import { useHoverPinnedPopover } from './HoverPinnedPopover'

const FETCH_DEBOUNCE_MS = 140

type TimelineState =
  | { status: 'idle'; items: null; error: null }
  | { status: 'loading'; items: NewVersionDiscoveryTimelineItem[] | null; error: null }
  | { status: 'ready'; items: NewVersionDiscoveryTimelineItem[]; error: null }
  | { status: 'error'; items: NewVersionDiscoveryTimelineItem[] | null; error: string }

const timelineCache = new Map<string, NewVersionDiscoveryTimelineItem[]>()

function emptyState(): TimelineState {
  return { status: 'idle', items: null, error: null }
}

function formatOccurredAt(value: string | null | undefined): string {
  const trimmed = (value ?? '').trim()
  if (!trimmed) return '时间未知'
  const parsed = new Date(trimmed)
  if (Number.isNaN(parsed.valueOf())) return trimmed
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(parsed)
}

function kindLabel(kind: NewVersionDiscoveryTimelineItem['kind']): string {
  if (kind === 'currentCandidate') return '当前候选'
  if (kind === 'currentRunning') return '当前运行'
  return '历史发现'
}

function loadErrorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    return error.message || `加载失败（${error.status}）`
  }
  if (error instanceof Error && error.message.trim()) {
    return error.message
  }
  return '加载失败，请稍后重试。'
}

export function DiscoveryHistoryPopover(props: {
  serviceId: string
  count: number | null | undefined
}) {
  const count = props.count ?? 0
  const {
    contentProps,
    open,
    popoverProps,
    triggerProps,
  } = useHoverPinnedPopover()
  const fetchTimer = useRef<number | null>(null)
  const activeServiceId = useRef(props.serviceId)
  const mountedRef = useRef(true)
  const [reloadToken, setReloadToken] = useState(0)
  const [state, setState] = useState<TimelineState>(() => {
    const cached = timelineCache.get(props.serviceId)
    return cached
      ? { status: 'ready', items: cached, error: null }
      : emptyState()
  })

  useEffect(() => {
    activeServiceId.current = props.serviceId
    const cached = timelineCache.get(props.serviceId)
    setState(cached ? { status: 'ready', items: cached, error: null } : emptyState())
  }, [props.serviceId])

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      if (fetchTimer.current != null) {
        window.clearTimeout(fetchTimer.current)
        fetchTimer.current = null
      }
    }
  }, [])

  useEffect(() => {
    if (!open) return
    const requestedServiceId = props.serviceId
    const cached = timelineCache.get(requestedServiceId) ?? null
    setState({ status: 'loading', items: cached, error: null })
    fetchTimer.current = window.setTimeout(async () => {
      try {
        const response = await getServiceNewVersionDiscoveryTimeline(requestedServiceId)
        if (!mountedRef.current || activeServiceId.current !== requestedServiceId) return
        timelineCache.set(requestedServiceId, response.items)
        setState({ status: 'ready', items: response.items, error: null })
      } catch (error) {
        if (!mountedRef.current || activeServiceId.current !== requestedServiceId) return
        setState((prev) => ({
          status: 'error',
          items: prev.items,
          error: loadErrorMessage(error),
        }))
      }
    }, FETCH_DEBOUNCE_MS)

    return () => {
      if (fetchTimer.current != null) {
        window.clearTimeout(fetchTimer.current)
        fetchTimer.current = null
      }
    }
  }, [count, open, props.serviceId, reloadToken])

  if (count <= 0) return null

  const retry = () => {
    timelineCache.delete(props.serviceId)
    setReloadToken((value) => value + 1)
  }

  return (
    <Popover {...popoverProps}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="discoveryHistoryTrigger pill pillMuted"
          aria-label={`发现 ${count} 次，查看版本时间线`}
          {...triggerProps}
        >
          {`发现 ${count} 次`}
        </button>
      </PopoverTrigger>
      <PopoverContent
        className="discoveryHistoryPopover"
        align="start"
        sideOffset={10}
        {...contentProps}
      >
        <div className="discoveryHistoryHeader">
          <div className="discoveryHistoryTitle">版本发现时间线</div>
          <div className="discoveryHistoryMeta">{`${count} 个候选版本`}</div>
        </div>

        {state.status === 'loading' && !state.items ? (
          <div className="discoveryHistoryState">正在加载版本记录…</div>
        ) : null}

        {state.status === 'error' && !state.items ? (
          <div className="discoveryHistoryState discoveryHistoryStateError">
            <span>{state.error}</span>
            <button type="button" className="discoveryHistoryRetry" onClick={retry}>
              重试
            </button>
          </div>
        ) : null}

        {state.items && state.items.length > 0 ? (
          <ol className="discoveryTimeline">
            {state.items.map((item, index) => (
              <li
                key={`${item.kind}:${item.version}:${item.occurredAt ?? 'unknown'}:${index}`}
                className={`discoveryTimelineItem discoveryTimelineItem-${item.kind}`}
              >
                <span className="discoveryTimelineDot" aria-hidden="true" />
                <div className="discoveryTimelineBody">
                  <div className="discoveryTimelineHeading">
                    <span className="discoveryTimelineVersion mono">{item.version}</span>
                    <span className="discoveryTimelineKind">{kindLabel(item.kind)}</span>
                  </div>
                  <div className="discoveryTimelineTime">{formatOccurredAt(item.occurredAt)}</div>
                </div>
              </li>
            ))}
          </ol>
        ) : null}

        {state.status === 'ready' && (!state.items || state.items.length === 0) ? (
          <div className="discoveryHistoryState">暂无版本发现记录。</div>
        ) : null}
      </PopoverContent>
    </Popover>
  )
}
