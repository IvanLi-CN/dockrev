import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { listServiceCandidates, type ServiceCandidateOption } from '../api'

function normalizeDigest(digest: string | null | undefined): string | null {
  const raw = (digest ?? '').trim()
  if (!raw) return null
  return raw.includes(':') ? raw : `sha256:${raw}`
}

function shortenDigest(digest: string, keep: number = 12): string {
  const normalized = normalizeDigest(digest) ?? digest
  const parts = normalized.split(':')
  if (parts.length < 2) return normalized
  const prefix = parts[0]
  const rest = parts.slice(1).join(':')
  if (rest.length <= keep) return normalized
  return `${prefix}:${rest.slice(0, keep)}…`
}

function uniqueSorted(values: Array<string | null | undefined>): string[] {
  const out = Array.from(new Set(values.map((v) => (v ?? '').trim()).filter(Boolean)))
  out.sort((a, b) => a.localeCompare(b))
  return out
}

function digestMatches(a: string | null, b: string | null): boolean {
  const aa = normalizeDigest(a)
  const bb = normalizeDigest(b)
  return Boolean(aa && bb && aa === bb)
}

export function VersionTagsPopover(props: {
  serviceId: string
  candidateTag: string | null
  candidateDigest: string | null
  triggerTitle?: string
  children: ReactNode
}) {
  const { serviceId, candidateTag, candidateDigest, children } = props
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const popoverRef = useRef<HTMLDivElement | null>(null)
  const hoverCloseTimer = useRef<number | null>(null)

  const [pinned, setPinned] = useState(false)
  const [hoverOpen, setHoverOpen] = useState(false)
  const open = pinned || hoverOpen

  const [pos, setPos] = useState<{ left: number; top: number } | null>(null)
  const [opts, setOpts] = useState<ServiceCandidateOption[] | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)

  const candidateDigestNorm = useMemo(() => normalizeDigest(candidateDigest), [candidateDigest])

  const clearHoverCloseTimer = useCallback(() => {
    if (hoverCloseTimer.current == null) return
    window.clearTimeout(hoverCloseTimer.current)
    hoverCloseTimer.current = null
  }, [])

  const scheduleHoverClose = () => {
    if (pinned) return
    clearHoverCloseTimer()
    hoverCloseTimer.current = window.setTimeout(() => {
      setHoverOpen(false)
      hoverCloseTimer.current = null
    }, 140)
  }

  const close = useCallback(() => {
    clearHoverCloseTimer()
    setPinned(false)
    setHoverOpen(false)
  }, [clearHoverCloseTimer])

  useEffect(() => {
    return () => {
      clearHoverCloseTimer()
    }
  }, [clearHoverCloseTimer])

  useEffect(() => {
    if (!open) return
    if (!candidateDigestNorm) return
    if (opts) return

    let alive = true
    void (async () => {
      setLoadError(null)
      try {
        const data = await listServiceCandidates(serviceId)
        if (!alive) return
        setOpts(data)
      } catch (e: unknown) {
        if (!alive) return
        setLoadError(e instanceof Error ? e.message : String(e))
        setOpts([])
      }
    })()

    return () => {
      alive = false
    }
  }, [candidateDigestNorm, open, opts, serviceId])

  const tagsForCandidate = useMemo(() => {
    if (!candidateTag) return []
    if (!candidateDigestNorm) return [candidateTag]
    const fromCandidates = (opts ?? [])
      .filter((o) => digestMatches(o.digest ?? null, candidateDigestNorm))
      .map((o) => o.tag)
    return uniqueSorted([candidateTag, ...fromCandidates])
  }, [candidateDigestNorm, candidateTag, opts])

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

  const popoverBody = open ? (
    <div
      ref={popoverRef}
      className="versionTagsPopover"
      style={pos ? { left: pos.left, top: pos.top } : undefined}
      role="dialog"
      aria-label="Version tags"
      onPointerEnter={() => {
        clearHoverCloseTimer()
        setHoverOpen(true)
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
        <div className="versionTagsPopoverMeta">
          <span className={pinned ? 'pill pillOk' : 'pill pillMuted'}>{pinned ? '已固定' : '悬浮预览'}</span>
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
        ) : opts == null ? (
          <div className="muted">加载中…</div>
        ) : loadError ? (
          <>
            <div className="versionTagsPopoverChips">
              <span className="versionTagsChip" title={candidateTag}>
                <span className="mono">{candidateTag}</span>
              </span>
            </div>
            <div className="muted">候选列表不可用</div>
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

      <div className="versionTagsPopoverFooter muted">点击版本可固定；按 ESC 或点空白关闭。</div>
    </div>
  ) : null

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        className="versionTagsTrigger mono monoPrimary"
        title={props.triggerTitle}
        aria-haspopup="dialog"
        aria-expanded={open}
        onPointerEnter={() => {
          clearHoverCloseTimer()
          setHoverOpen(true)
        }}
        onPointerLeave={() => {
          scheduleHoverClose()
        }}
        onClick={() => {
          clearHoverCloseTimer()
          setPinned((prev) => !prev)
          setHoverOpen(true)
        }}
      >
        {children}
      </button>
      {open ? createPortal(popoverBody, document.body) : null}
    </>
  )
}
