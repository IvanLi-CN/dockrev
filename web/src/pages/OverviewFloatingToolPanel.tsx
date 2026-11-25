import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent, type RefObject } from 'react'
import { ChevronLeft, ChevronRight } from 'lucide-react'

import {
  Button,
  IconButton,
} from '../ui'
import {
  OVERVIEW_TOOL_BUBBLE_DEFAULT_SIZE,
  OVERVIEW_TOOL_PANEL_DEFAULT_SIZE,
  OVERVIEW_TOOL_PANEL_MARGIN,
  clampOverviewToolPanelLeft,
  clampOverviewToolPanelTop,
  createDefaultOverviewToolPanelState,
  readOverviewToolPanelState,
  resolveOverviewToolPanelRect,
  resolveOverviewToolPanelLeft,
  snapOverviewToolPanelSide,
  writeOverviewToolPanelState,
  type OverviewToolPanelBounds,
  type OverviewToolPanelSize,
  type OverviewToolPanelState,
} from './overviewToolPanelState'
import {
  clearPendingPagesDemoRestoreState,
  readPublicDemoSessionSummary,
  resetPublicDemoSessionState,
} from '../demo/publicDemoControls'
import { navigate, type Route } from '../routes'

const DESKTOP_MEDIA_QUERY = '(min-width: 961px)'
const FLOATING_PANEL_EDGE_OFFSET = 10
const COLLAPSE_BUTTON_CENTER_FALLBACK_Y = 27
const BUBBLE_CLICK_SUPPRESS_MS = 160

type DragSession = {
  pointerId: number
  captureElement: HTMLElement | null
  collapsed: boolean
  moved: boolean
  offsetX: number
  offsetY: number
  size: OverviewToolPanelSize
  startX: number
  startY: number
}

function readDesktopMatches(): boolean {
  return typeof window !== 'undefined' && window.matchMedia(DESKTOP_MEDIA_QUERY).matches
}

function useMediaQuery(query: string, readMatches: () => boolean): boolean {
  const [matches, setMatches] = useState(readMatches)

  useEffect(() => {
    if (typeof window === 'undefined') return
    const media = window.matchMedia(query)
    const sync = () => setMatches(media.matches)
    sync()
    media.addEventListener('change', sync)
    return () => media.removeEventListener('change', sync)
  }, [query])

  return matches
}

function sameBounds(left: OverviewToolPanelBounds | null, right: OverviewToolPanelBounds | null): boolean {
  return (
    left?.left === right?.left &&
    left?.top === right?.top &&
    left?.right === right?.right &&
    left?.bottom === right?.bottom
  )
}

function sameSize(left: OverviewToolPanelSize, right: OverviewToolPanelSize): boolean {
  return left.width === right.width && left.height === right.height
}

function viewportBounds(): OverviewToolPanelBounds | null {
  if (typeof window === 'undefined') return null
  const width = window.innerWidth
  const height = window.innerHeight
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return null
  return {
    left: 0,
    top: 0,
    right: width,
    bottom: height,
  }
}

function resolveExpandedPanelLeft(
  side: 'left' | 'right',
  bounds: OverviewToolPanelBounds,
  width: number,
): number {
  const dockLeft = resolveOverviewToolPanelLeft(side, bounds, width)
  const offset = side === 'left' ? FLOATING_PANEL_EDGE_OFFSET : -FLOATING_PANEL_EDGE_OFFSET
  return clampOverviewToolPanelLeft(dockLeft + offset, bounds, width)
}

function resolveBubbleTopFromButtonCenter(
  panelTop: number,
  bounds: OverviewToolPanelBounds,
  bubbleHeight: number,
  panelNode: HTMLElement | null,
): number {
  const buttonNode =
    panelNode?.querySelector<HTMLButtonElement>('button[aria-label^="收起"]') ?? null
  const panelRect = panelNode?.getBoundingClientRect() ?? null
  const buttonRect = buttonNode?.getBoundingClientRect() ?? null
  const buttonCenterY =
    panelRect && buttonRect
      ? panelTop + (buttonRect.top - panelRect.top) + buttonRect.height / 2
      : panelTop + COLLAPSE_BUTTON_CENTER_FALLBACK_Y
  return clampOverviewToolPanelTop(buttonCenterY - bubbleHeight / 2, bounds, bubbleHeight)
}

function useMeasuredSize<T extends HTMLElement>(
  ref: RefObject<T | null>,
  fallback: OverviewToolPanelSize,
): OverviewToolPanelSize {
  const [size, setSize] = useState(fallback)

  useEffect(() => {
    const node = ref.current
    if (!node) return

    const update = () => {
      const rect = node.getBoundingClientRect()
      const next = {
        width: Math.round(rect.width) || fallback.width,
        height: Math.round(rect.height) || fallback.height,
      }
      setSize((current) => (sameSize(current, next) ? current : next))
    }

    update()
    if (typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(update)
    observer.observe(node)
    return () => observer.disconnect()
  }, [fallback.height, fallback.width, ref])

  return size
}

export function HomepageFloatingToolPanel(props: {
  pageRef: RefObject<HTMLElement | null>
}) {
  const desktop = useMediaQuery(DESKTOP_MEDIA_QUERY, readDesktopMatches)
  const panelRef = useRef<HTMLElement | null>(null)
  const bubbleRef = useRef<HTMLDivElement | null>(null)
  const dragSessionRef = useRef<DragSession | null>(null)
  const suppressBubbleClickUntilRef = useRef(0)
  const boundsRef = useRef<OverviewToolPanelBounds | null>(null)
  const dragRectRef = useRef<{ left: number; top: number } | null>(null)
  const [bounds, setBounds] = useState<OverviewToolPanelBounds | null>(null)
  const [panelState, setPanelState] = useState<OverviewToolPanelState | null>(() => readOverviewToolPanelState())
  const [dragging, setDragging] = useState(false)
  const [dragRect, setDragRect] = useState<{ left: number; top: number } | null>(null)

  const panelSize = useMeasuredSize(panelRef, OVERVIEW_TOOL_PANEL_DEFAULT_SIZE)
  const bubbleSize = useMeasuredSize(bubbleRef, OVERVIEW_TOOL_BUBBLE_DEFAULT_SIZE)
  boundsRef.current = bounds
  dragRectRef.current = dragRect

  useEffect(() => {
    if (!desktop || typeof window === 'undefined') return

    let frame = 0
    const measure = () => {
      frame = 0
      const next = viewportBounds()
      setBounds((current) => (sameBounds(current, next) ? current : next))
    }
    const scheduleMeasure = () => {
      if (frame !== 0) return
      frame = window.requestAnimationFrame(measure)
    }

    measure()
    window.addEventListener('resize', scheduleMeasure)
    window.visualViewport?.addEventListener('resize', scheduleMeasure)
    return () => {
      if (frame !== 0) window.cancelAnimationFrame(frame)
      window.removeEventListener('resize', scheduleMeasure)
      window.visualViewport?.removeEventListener('resize', scheduleMeasure)
    }
  }, [desktop, props.pageRef])

  useEffect(() => {
    if (!desktop || !bounds) return
    setPanelState((current) => {
      const next = current ?? createDefaultOverviewToolPanelState(bounds, panelSize)
      const size = next.collapsed ? bubbleSize : panelSize
      const clampedLeft = clampOverviewToolPanelLeft(next.left, bounds, panelSize.width)
      const clampedTop = clampOverviewToolPanelTop(next.top, bounds, size.height)
      if (
        current &&
        current.collapsed === next.collapsed &&
        current.left === clampedLeft &&
        current.side === next.side &&
        current.top === clampedTop
      ) {
        return current
      }
      return { ...next, left: clampedLeft, top: clampedTop }
    })
  }, [bounds, bubbleSize, desktop, panelSize])

  useEffect(() => {
    if (!panelState) return
    writeOverviewToolPanelState(panelState)
  }, [panelState])

  const onWindowPointerMove = useCallback((event: PointerEvent) => {
    const session = dragSessionRef.current
    const currentBounds = boundsRef.current
    if (!session || !currentBounds || event.pointerId !== session.pointerId) return

    if (event.pointerType === 'mouse' && (event.buttons & 1) === 0) {
      window.dispatchEvent(new PointerEvent('pointercancel', { pointerId: session.pointerId }))
      return
    }
    if (event.pointerType === 'touch') event.preventDefault()

    const minLeft = currentBounds.left + OVERVIEW_TOOL_PANEL_MARGIN
    const maxLeft = Math.max(minLeft, currentBounds.right - session.size.width - OVERVIEW_TOOL_PANEL_MARGIN)
    const nextLeft = Math.min(Math.max(event.clientX - session.offsetX, minLeft), maxLeft)
    const nextTop = clampOverviewToolPanelTop(
      event.clientY - session.offsetY,
      currentBounds,
      session.size.height,
      OVERVIEW_TOOL_PANEL_MARGIN,
    )

    if (!session.moved && Math.hypot(event.clientX - session.startX, event.clientY - session.startY) >= 6) {
      session.moved = true
    }

    setDragRect({ left: nextLeft, top: nextTop })
  }, [])

  const endDrag = useCallback(() => {
    const session = dragSessionRef.current
    const currentBounds = boundsRef.current
    if (!session || !currentBounds) {
      dragSessionRef.current = null
      setDragging(false)
      setDragRect(null)
      document.body.style.removeProperty('user-select')
      return
    }

    if (session.captureElement?.hasPointerCapture(session.pointerId)) {
      session.captureElement.releasePointerCapture(session.pointerId)
    }

    suppressBubbleClickUntilRef.current =
      session.collapsed && session.moved ? Date.now() + BUBBLE_CLICK_SUPPRESS_MS : 0

    setPanelState((current) => {
      const base = current ?? createDefaultOverviewToolPanelState(currentBounds, session.size)
      const rect = dragRectRef.current ?? resolveOverviewToolPanelRect(base, currentBounds, session.size)
      const side = snapOverviewToolPanelSide(rect.left, currentBounds, session.size.width)

      if (base.collapsed) {
        return {
          collapsed: true,
          left: resolveExpandedPanelLeft(side, currentBounds, panelSize.width),
          side,
          top: clampOverviewToolPanelTop(rect.top, currentBounds, session.size.height),
        }
      }

      return {
        collapsed: false,
        left: clampOverviewToolPanelLeft(rect.left, currentBounds, session.size.width),
        side,
        top: clampOverviewToolPanelTop(rect.top, currentBounds, session.size.height),
      }
    })

    dragSessionRef.current = null
    setDragging(false)
    setDragRect(null)
    document.body.style.removeProperty('user-select')
  }, [panelSize.width])

  const onWindowPointerEnd = useCallback(function handleWindowPointerEnd(event: PointerEvent) {
    const session = dragSessionRef.current
    if (!session || event.pointerId !== session.pointerId) return
    window.removeEventListener('pointermove', onWindowPointerMove)
    window.removeEventListener('pointerup', handleWindowPointerEnd)
    window.removeEventListener('pointercancel', handleWindowPointerEnd)
    endDrag()
  }, [endDrag, onWindowPointerMove])

  useEffect(() => {
    return () => {
      const session = dragSessionRef.current
      dragSessionRef.current = null
      if (session?.captureElement?.hasPointerCapture(session.pointerId)) {
        session.captureElement.releasePointerCapture(session.pointerId)
      }
      document.body.style.removeProperty('user-select')
      window.removeEventListener('pointermove', onWindowPointerMove)
      window.removeEventListener('pointerup', onWindowPointerEnd)
      window.removeEventListener('pointercancel', onWindowPointerEnd)
    }
  }, [onWindowPointerEnd, onWindowPointerMove])

  const beginDrag = useCallback(
    (
      event: ReactPointerEvent<HTMLElement>,
      size: OverviewToolPanelSize,
      collapsed: boolean,
    ) => {
      if (!desktop || !bounds || !panelState) return
      if (event.pointerType === 'mouse' && event.button !== 0) return
      if (
        !collapsed &&
        (event.target as HTMLElement).closest('button, a, input, textarea, select, [role="button"]')
      ) {
        return
      }

      const restingRect = resolveOverviewToolPanelRect(panelState, bounds, size)
      event.preventDefault()
      document.body.style.userSelect = 'none'
      try {
        event.currentTarget.setPointerCapture(event.pointerId)
      } catch {
        // Some input sources do not support pointer capture here.
      }

      dragSessionRef.current = {
        pointerId: event.pointerId,
        captureElement: event.currentTarget,
        collapsed,
        moved: false,
        offsetX: event.clientX - (dragRect?.left ?? restingRect.left),
        offsetY: event.clientY - (dragRect?.top ?? restingRect.top),
        size,
        startX: event.clientX,
        startY: event.clientY,
      }
      setDragging(true)
      setDragRect(dragRect ?? restingRect)
      window.addEventListener('pointermove', onWindowPointerMove, { passive: false })
      window.addEventListener('pointerup', onWindowPointerEnd)
      window.addEventListener('pointercancel', onWindowPointerEnd)
    },
    [bounds, desktop, dragRect, onWindowPointerEnd, onWindowPointerMove, panelState],
  )

  const expand = useCallback(() => {
    if (!bounds) return
    setPanelState((current) => {
      const base = current ?? createDefaultOverviewToolPanelState(bounds, panelSize)
      const expandedLeft = resolveExpandedPanelLeft(base.side, bounds, panelSize.width)
      return {
        collapsed: false,
        left: expandedLeft,
        side: base.side,
        top: clampOverviewToolPanelTop(base.top, bounds, panelSize.height),
      }
    })
  }, [bounds, panelSize])

  const collapse = useCallback(() => {
    if (!bounds) return
    setPanelState((current) => {
      const base = current ?? createDefaultOverviewToolPanelState(bounds, panelSize)
      const visible = dragRectRef.current ?? resolveOverviewToolPanelRect(base, bounds, panelSize)
      const side = snapOverviewToolPanelSide(visible.left, bounds, panelSize.width)
      return {
        collapsed: true,
        left: resolveExpandedPanelLeft(side, bounds, panelSize.width),
        side,
        top: resolveBubbleTopFromButtonCenter(
          visible.top,
          bounds,
          bubbleSize.height,
          panelRef.current,
        ),
      }
    })
  }, [bounds, bubbleSize, panelSize])

  const expandFromBubble = useCallback(() => {
    if (Date.now() < suppressBubbleClickUntilRef.current) {
      return
    }
    expand()
  }, [expand])

  const visibleRect = useMemo(() => {
    if (!bounds || !panelState) return null
    const size = panelState.collapsed ? bubbleSize : panelSize
    return dragRect ?? resolveOverviewToolPanelRect(panelState, bounds, size)
  }, [bounds, bubbleSize, dragRect, panelSize, panelState])

  const projectedCollapseSide = useMemo(() => {
    if (!bounds || !visibleRect) return panelState?.side ?? 'right'
    if (panelState?.collapsed) return panelState.side
    return snapOverviewToolPanelSide(visibleRect.left, bounds, panelSize.width)
  }, [bounds, panelSize.width, panelState, visibleRect])
  const demoSummary = readPublicDemoSessionSummary()
  const routeActions: Array<{ label: string; note: string; route: Route }> = [
    { label: 'Queue 假写', note: '任务队列与进度', route: { name: 'queue' } },
    { label: 'GHCR 假写', note: 'Webhook 维护', route: { name: 'ghcr-webhook-registry' } },
    { label: 'Cleanup 假写', note: '扫描与 apply', route: { name: 'cleanup' } },
    { label: 'Deploy 检查', note: 'welcome / deploy', route: { name: 'deploy-check' } },
  ]

  if (!desktop || !bounds || !panelState || !visibleRect) return null

  if (panelState.collapsed) {
    return (
      <div
        ref={bubbleRef}
        className="homepageToolBubble"
        data-dragging={dragging ? 'true' : 'false'}
        data-side={panelState.side}
        style={{ left: `${visibleRect.left}px`, top: `${visibleRect.top}px` }}
      >
        <button
          className="homepageToolBubbleButton"
          aria-label="展开 Demo 控制面板"
          onClick={expandFromBubble}
          onPointerDown={(event) => beginDrag(event, bubbleSize, true)}
          type="button"
        >
          <span className="homepageToolBubbleIconShell" aria-hidden="true">
            <span className="homepageToolGripDots" />
          </span>
          <span className="homepageToolBubbleCount">DEMO</span>
        </button>
      </div>
    )
  }

  return (
    <section
      ref={panelRef}
      className="homepageToolFloatWindow"
      aria-label="Demo 控制面板"
      data-dragging={dragging ? 'true' : 'false'}
      data-side={projectedCollapseSide}
      style={{ left: `${visibleRect.left}px`, top: `${visibleRect.top}px` }}
    >
      <div className="homepageToolFloatHead" onPointerDown={(event) => beginDrag(event, panelSize, false)}>
        <div className="homepageToolFloatGrip" aria-hidden="true">
          <span className="homepageToolGripDots" />
        </div>
        <div className="homepageToolFloatMeta">
          <div className="homepageToolFloatEyebrow">Public Demo</div>
          <div className="homepageToolFloatTitle">Demo 控制面板</div>
        </div>
        <div className="homepageToolFloatActions">
          <IconButton onClick={collapse} title="收起 Demo 控制面板" variant="ghost">
            {projectedCollapseSide === 'left' ? (
              <ChevronLeft aria-hidden="true" size={16} strokeWidth={2.2} />
            ) : (
              <ChevronRight aria-hidden="true" size={16} strokeWidth={2.2} />
            )}
          </IconButton>
        </div>
      </div>

      <div className="homepageToolFloatBody">
        <div className="homepageToolFloatSection">
          <div className="homepageToolFloatLabel">Demo 场景</div>
          <div className="homepageToolActionGrid">
            {routeActions.map((action) => (
              <Button
                key={action.label}
                className="homepageToolActionButton"
                onClick={() => navigate(action.route)}
                variant="ghost"
              >
                <span>{action.label}</span>
                <span className="homepageToolActionNote">{action.note}</span>
              </Button>
            ))}
          </div>
        </div>

        <div className="homepageToolFloatSection">
          <div className="homepageToolFloatLabel">状态控制</div>
          <div className="homepageToolActionGrid homepageToolActionGridCompact">
            <Button
              className="homepageToolActionButton"
              onClick={() => resetPublicDemoSessionState()}
              variant="ghost"
            >
              <span>重置 Seed</span>
              <span className="homepageToolActionNote">清空当前会话并回到 /demo/</span>
            </Button>
            <Button
              className="homepageToolActionButton"
              disabled={!demoSummary.routeRestorePending}
              onClick={() => {
                clearPendingPagesDemoRestoreState()
                setPanelState((current) => (current ? { ...current } : current))
              }}
              variant="ghost"
            >
              <span>清空 Restore</span>
              <span className="homepageToolActionNote">移除 404 深链恢复状态</span>
            </Button>
            <Button
              className="homepageToolActionButton"
              onClick={() => window.location.reload()}
              variant="ghost"
            >
              <span>重载当前页</span>
              <span className="homepageToolActionNote">重新挂载当前 Demo 会话</span>
            </Button>
          </div>
        </div>

        <div className="homepageToolFloatHint">
          切换 Demo 场景或重置当前会话；不会影响真实环境。
        </div>
      </div>
    </section>
  )
}
