import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { listServiceDigestTags } from '../api'
import { normalizeDigest, shortenDigest } from './digest'

function uniquePreserveOrder(values: Array<string | null | undefined>): string[] {
  const out: string[] = []
  const seen = new Set<string>()
  for (const v of values) {
    const t = (v ?? '').trim()
    if (!t) continue
    if (seen.has(t)) continue
    seen.add(t)
    out.push(t)
  }
  return out
}

const HOVER_CLOSE_DELAY_MS = 300
const POPOVER_ANIM_MS = 160
const FETCH_DEBOUNCE_MS = 220

type DigestTagsState = {
  key: string
  tags: string[] | null
  error: string | null
}

export function VersionTagsPopover(props: {
  serviceId: string
  candidateTag: string | null
  candidateDigest: string | null
  children: ReactNode
}) {
  const { serviceId, candidateTag, candidateDigest, children } = props
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const popoverRef = useRef<HTMLDivElement | null>(null)
  const hoverCloseTimer = useRef<number | null>(null)
  const popoverUnmountTimer = useRef<number | null>(null)
  const popoverShowRaf = useRef<number | null>(null)
  const fetchTimer = useRef<number | null>(null)
  const pinnedRef = useRef(false)

  const [pinned, setPinned] = useState(false)
  const [hoverOpen, setHoverOpen] = useState(false)
  const open = pinned || hoverOpen
  const [renderPopover, setRenderPopover] = useState(false)
  const [popoverVisible, setPopoverVisible] = useState(false)

  const [pos, setPos] = useState<{ left: number; top: number } | null>(null)

  const candidateDigestNorm = useMemo(() => normalizeDigest(candidateDigest), [candidateDigest])
  const digestKey = useMemo(() => `${serviceId}:${candidateDigestNorm ?? ''}`, [candidateDigestNorm, serviceId])
  const [digestState, setDigestState] = useState<DigestTagsState>(() => ({
    key: digestKey,
    tags: null,
    error: null,
  }))
  const digestTags = digestState.key === digestKey ? digestState.tags : null
  const loadError = digestState.key === digestKey ? digestState.error : null

  const clearHoverCloseTimer = useCallback(() => {
    if (hoverCloseTimer.current == null) return
    window.clearTimeout(hoverCloseTimer.current)
    hoverCloseTimer.current = null
  }, [])

  const clearPopoverUnmountTimer = useCallback(() => {
    if (popoverUnmountTimer.current == null) return
    window.clearTimeout(popoverUnmountTimer.current)
    popoverUnmountTimer.current = null
  }, [])

  const clearPopoverShowRaf = useCallback(() => {
    if (popoverShowRaf.current == null) return
    window.cancelAnimationFrame(popoverShowRaf.current)
    popoverShowRaf.current = null
  }, [])

  const showPopover = useCallback(() => {
    clearPopoverUnmountTimer()

    if (!renderPopover) {
      setRenderPopover(true)
      setPopoverVisible(false)
      clearPopoverShowRaf()
      popoverShowRaf.current = window.requestAnimationFrame(() => {
        setPopoverVisible(true)
        popoverShowRaf.current = null
      })
      return
    }

    setPopoverVisible(true)
  }, [clearPopoverShowRaf, clearPopoverUnmountTimer, renderPopover])

  const hidePopover = useCallback(() => {
    if (!renderPopover) return

    clearPopoverShowRaf()
    setPopoverVisible(false)
    clearPopoverUnmountTimer()
    popoverUnmountTimer.current = window.setTimeout(() => {
      setRenderPopover(false)
      popoverUnmountTimer.current = null
    }, POPOVER_ANIM_MS)
  }, [clearPopoverShowRaf, clearPopoverUnmountTimer, renderPopover])

  const scheduleHoverClose = () => {
    if (pinnedRef.current) return
    clearHoverCloseTimer()
    hoverCloseTimer.current = window.setTimeout(() => {
      hoverCloseTimer.current = null
      if (pinnedRef.current) return
      setHoverOpen(false)
      hidePopover()
    }, HOVER_CLOSE_DELAY_MS)
  }

  const close = useCallback(() => {
    clearHoverCloseTimer()
    setPinned(false)
    pinnedRef.current = false
    setHoverOpen(false)
    hidePopover()
  }, [clearHoverCloseTimer, hidePopover])

  useEffect(() => {
    return () => {
      clearHoverCloseTimer()
      clearPopoverShowRaf()
      clearPopoverUnmountTimer()
      if (fetchTimer.current != null) {
        window.clearTimeout(fetchTimer.current)
        fetchTimer.current = null
      }
    }
  }, [clearHoverCloseTimer, clearPopoverShowRaf, clearPopoverUnmountTimer])

  useEffect(() => {
    if (!open) return
    if (!candidateDigestNorm) return
    if (digestTags != null) return

    let alive = true
    const delay = pinned ? 0 : FETCH_DEBOUNCE_MS
    if (fetchTimer.current != null) window.clearTimeout(fetchTimer.current)
    fetchTimer.current = window.setTimeout(() => {
      setDigestState({ key: digestKey, tags: null, error: null })
      listServiceDigestTags(serviceId, candidateDigestNorm)
        .then((data) => {
          if (!alive) return
          setDigestState({ key: digestKey, tags: data.tags, error: null })
        })
        .catch((e: unknown) => {
          if (!alive) return
          setDigestState({
            key: digestKey,
            tags: [],
            error: e instanceof Error ? e.message : String(e),
          })
        })
        .finally(() => {
          fetchTimer.current = null
        })
    }, delay)

    return () => {
      alive = false
      if (fetchTimer.current != null) {
        window.clearTimeout(fetchTimer.current)
        fetchTimer.current = null
      }
    }
  }, [candidateDigestNorm, digestKey, digestTags, open, pinned, serviceId])

  const tagsForCandidate = useMemo(() => {
    if (!candidateTag) return []
    if (!candidateDigestNorm) return [candidateTag]
    return uniquePreserveOrder([candidateTag, ...(digestTags ?? [])])
  }, [candidateDigestNorm, candidateTag, digestTags])

  useLayoutEffect(() => {
    if (!open) return
    const trigger = triggerRef.current
    if (!trigger) return
    const rect = trigger.getBoundingClientRect()
    setPos({ left: rect.left, top: rect.bottom + 8 })
  }, [open])

  useLayoutEffect(() => {
    if (!open) return
    const trigger = triggerRef.current
    const pop = popoverRef.current
    if (!trigger || !pop) return

    const reposition = () => {
      const rect = trigger.getBoundingClientRect()
      const popRect = pop.getBoundingClientRect()

      let left = rect.left
      let top = rect.bottom + 8

      const margin = 10
      if (left + popRect.width > window.innerWidth - margin) left = window.innerWidth - margin - popRect.width
      if (left < margin) left = margin

      if (top + popRect.height > window.innerHeight - margin) top = rect.top - 8 - popRect.height
      if (top < margin) top = margin

      setPos((prev) => (prev && prev.left === left && prev.top === top ? prev : { left, top }))
    }

    reposition()
    window.addEventListener('resize', reposition)
    window.addEventListener('scroll', reposition, true)
    return () => {
      window.removeEventListener('resize', reposition)
      window.removeEventListener('scroll', reposition, true)
    }
  }, [open])

  useEffect(() => {
    if (!pinned) return

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close()
    }

    const onPointerDown = (e: PointerEvent) => {
      const t = e.target as unknown
      const el = t instanceof Element ? t : null
      if (!el) return
      if (triggerRef.current?.contains(el)) return
      if (popoverRef.current?.contains(el)) return
      close()
    }

    document.addEventListener('keydown', onKeyDown)
    document.addEventListener('pointerdown', onPointerDown)
    return () => {
      document.removeEventListener('keydown', onKeyDown)
      document.removeEventListener('pointerdown', onPointerDown)
    }
  }, [close, pinned])

  const popoverBody = renderPopover ? (
    <div
      ref={popoverRef}
      className="versionTagsPopover"
      style={pos ? { left: pos.left, top: pos.top } : undefined}
      role="dialog"
      aria-label="Version tags"
      data-state={popoverVisible ? 'open' : 'closed'}
      onPointerEnter={() => {
        clearHoverCloseTimer()
        setHoverOpen(true)
        showPopover()
      }}
      onPointerLeave={() => {
        scheduleHoverClose()
      }}
    >
      <div className="versionTagsPopoverHeader">
        <div className="versionTagsPopoverTitle">
          <span className="mono monoPrimary">{candidateTag ?? '-'}</span>
          {candidateDigestNorm ? (
            <span className="mono muted" title={candidateDigestNorm}>
              {shortenDigest(candidateDigestNorm)}
            </span>
          ) : (
            <span className="mono muted">digest 未知</span>
          )}
        </div>
      </div>

      <div className="versionTagsPopoverSection">
        <div className="label">该版本所有标签</div>
        {!candidateTag ? (
          <div className="muted">无候选版本</div>
        ) : !candidateDigestNorm ? (
          <>
            <div className="versionTagsPopoverChips">
              <span className="versionTagsChip" title={candidateTag}>
                <span className="mono">{candidateTag}</span>
              </span>
            </div>
            <div className="muted">digest 缺失，无法聚合更多标签</div>
          </>
        ) : digestTags == null ? (
          <div className="muted">加载中…</div>
        ) : loadError ? (
          <>
            {tagsForCandidate.length > 0 ? (
              <div className="versionTagsPopoverChips">
                {tagsForCandidate.map((t) => (
                  <span key={t} className="versionTagsChip" title={t}>
                    <span className="mono">{t}</span>
                  </span>
                ))}
              </div>
            ) : (
              <div className="muted">未找到同 digest 的标签</div>
            )}
            <div className="muted">加载失败</div>
          </>
        ) : tagsForCandidate.length === 0 ? (
          <div className="muted">未找到同 digest 的标签</div>
        ) : (
          <div className="versionTagsPopoverChips">
            {tagsForCandidate.map((t) => (
              <span key={t} className="versionTagsChip" title={t}>
                <span className="mono">{t}</span>
              </span>
            ))}
          </div>
        )}
      </div>

    </div>
  ) : null

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        className="versionTagsTrigger mono monoPrimary"
        aria-haspopup="dialog"
        aria-expanded={open}
        onPointerEnter={() => {
          clearHoverCloseTimer()
          setHoverOpen(true)
          showPopover()
        }}
        onPointerLeave={() => {
          scheduleHoverClose()
        }}
        onClick={() => {
          clearHoverCloseTimer()
          setPinned((prev) => {
            const next = !prev
            pinnedRef.current = next
            return next
          })
          setHoverOpen(true)
          showPopover()
        }}
      >
        {children}
      </button>
      {renderPopover ? createPortal(popoverBody, document.body) : null}
    </>
  )
}
