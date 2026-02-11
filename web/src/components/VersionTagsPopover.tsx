import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { listServiceDigestTags, type ServiceDigestTagsScanSummary } from '../api'
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

function isStrictSemverTag(tag: string): boolean {
  const t = tag.trim()
  if (!t) return false
  return /^v?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(t)
}

type Semver = {
  major: number
  minor: number
  patch: number
  prerelease: Array<string | number>
  hasPrerelease: boolean
}

function parseSemverTag(tag: string): Semver | null {
  let t = tag.trim()
  if (!t) return null
  if (t.startsWith('v')) t = t.slice(1)
  if (!t) return null

  const [main, build] = t.split('+', 2)
  void build
  const [core, pre] = main.split('-', 2)
  const parts = core.split('.')
  if (parts.length !== 3) return null
  if (!parts.every((p) => /^\d+$/.test(p))) return null

  const nums = parts.map((p) => Number(p))
  if (!nums.every((n) => Number.isFinite(n) && n >= 0)) return null

  const prerelease = (pre ?? '')
    .split('.')
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => (/^\d+$/.test(p) ? Number(p) : p))

  return {
    major: nums[0],
    minor: nums[1],
    patch: nums[2],
    prerelease,
    hasPrerelease: prerelease.length > 0,
  }
}

function cmpSemver(a: Semver, b: Semver): number {
  if (a.major !== b.major) return a.major < b.major ? -1 : 1
  if (a.minor !== b.minor) return a.minor < b.minor ? -1 : 1
  if (a.patch !== b.patch) return a.patch < b.patch ? -1 : 1

  // No prerelease is higher than prerelease.
  if (a.hasPrerelease !== b.hasPrerelease) return a.hasPrerelease ? -1 : 1
  if (!a.hasPrerelease) return 0

  const len = Math.max(a.prerelease.length, b.prerelease.length)
  for (let i = 0; i < len; i++) {
    const ai = a.prerelease[i]
    const bi = b.prerelease[i]
    if (ai == null) return -1
    if (bi == null) return 1
    if (ai === bi) continue

    const an = typeof ai === 'number'
    const bn = typeof bi === 'number'
    if (an && bn) return ai < bi ? -1 : 1
    if (an !== bn) return an ? -1 : 1

    const as = String(ai)
    const bs = String(bi)
    if (as === bs) continue
    return as < bs ? -1 : 1
  }

  return 0
}

function sortTagsForDisplay(tags: string[]): string[] {
  const uniq = uniquePreserveOrder(tags)
  const semver: Array<{ t: string; v: Semver }> = []
  const other: string[] = []
  for (const t of uniq) {
    const v = parseSemverTag(t)
    if (v) semver.push({ t, v })
    else other.push(t)
  }
  semver.sort((a, b) => cmpSemver(b.v, a.v))
  other.sort((a, b) => a.localeCompare(b))
  return [...semver.map((x) => x.t), ...other]
}

const HOVER_CLOSE_DELAY_MS = 300
const POPOVER_ANIM_MS = 160
const FETCH_DEBOUNCE_MS = 220

type DigestTagsState = {
  key: string
  tags: string[] | null
  scan: ServiceDigestTagsScanSummary | null
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
    scan: null,
    error: null,
  }))
  const digestTags = digestState.key === digestKey ? digestState.tags : null
  const scan = digestState.key === digestKey ? digestState.scan : null
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
    if (!(candidateTag ?? '').trim()) return

    // Digest tag listing is only meaningful when digest is known.
    if (!candidateDigestNorm) return
    if (digestTags != null) return

    let alive = true
    const delay = pinned ? 0 : FETCH_DEBOUNCE_MS
    if (fetchTimer.current != null) window.clearTimeout(fetchTimer.current)
    fetchTimer.current = window.setTimeout(() => {
      setDigestState({ key: digestKey, tags: null, scan: null, error: null })
      listServiceDigestTags(serviceId, candidateDigestNorm ?? '')
        .then((data) => {
          if (!alive) return
          setDigestState({
            key: digestKey,
            tags: data.tags,
            scan: data.scan ?? null,
            error: null,
          })
        })
        .catch((e: unknown) => {
          if (!alive) return
          setDigestState({
            key: digestKey,
            tags: [],
            scan: null,
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
  }, [candidateDigestNorm, candidateTag, digestKey, digestTags, open, pinned, serviceId])

  const candidateTagTrim = useMemo(() => (candidateTag ?? '').trim(), [candidateTag])
  const candidateIsSemver = useMemo(() => Boolean(candidateTagTrim && isStrictSemverTag(candidateTagTrim)), [candidateTagTrim])

  const digestTagsTotal = useMemo(() => (digestTags ? digestTags.length : 0), [digestTags])
  const digestSemverTags = useMemo(() => {
    if (!digestTags) return []
    return sortTagsForDisplay(digestTags.filter(isStrictSemverTag))
  }, [digestTags])

  const digestOtherTagsTotal = useMemo(() => {
    if (!digestTags) return 0
    return digestTags.length - digestSemverTags.length
  }, [digestSemverTags.length, digestTags])

  const semverPreview = useMemo(() => digestSemverTags.slice(0, 6), [digestSemverTags])
  const semverMore = useMemo(() => Math.max(0, digestSemverTags.length - semverPreview.length), [digestSemverTags.length, semverPreview.length])

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

  const copyText = useCallback((text: string) => {
    const t = text.trim()
    if (!t) return
    void navigator.clipboard?.writeText(t)
  }, [])

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
          <span className="mono monoPrimary">{candidateTag ?? '无候选版本'}</span>
          {candidateDigestNorm ? (
            <span className="mono muted">
              {shortenDigest(candidateDigestNorm)}
            </span>
          ) : (
            <span className="mono muted">digest 未知</span>
          )}
        </div>
      </div>

      <div className="versionTagsPopoverSection">
        <div className="label">参考信息</div>
        {!candidateTag ? (
          <div className="muted">无候选版本</div>
        ) : !candidateDigestNorm ? (
          <>
            <div className="muted">digest 缺失，无法列出同 digest 的 tags</div>
            <div className="versionTagsPopoverActions">
              <button type="button" className="versionTagsPopoverAction" onClick={() => copyText(candidateTag)}>
                复制
              </button>
            </div>
          </>
        ) : digestTags == null ? (
          <div className="muted">加载中…</div>
        ) : loadError ? (
          <div className="muted">加载失败：{loadError}</div>
        ) : digestTags.length === 0 ? (
          <div className="muted">未找到同 digest 的标签</div>
        ) : (
          <>
            <div className="muted">
              共 {digestTagsTotal} 个 tags（semver {digestSemverTags.length} · 其他 {digestOtherTagsTotal}）
            </div>

            {scan && candidateDigestNorm && (scan.manifestsTimeout > 0 || scan.manifestsError > 0) ? (
              <div className="muted">
                注意：digest tags 可能不完整（ok {scan.manifestsOk} / {scan.repoTagsTotal}
                {scan.manifestsTimeout > 0 ? ` · timeout ${scan.manifestsTimeout}` : ''}
                {scan.manifestsError > 0 ? ` · error ${scan.manifestsError}` : ''}
                ）
              </div>
            ) : null}

            {candidateIsSemver && digestSemverTags.length > 0 && !digestSemverTags.includes(candidateTagTrim) ? (
              <div className="muted">注意：候选 tag 不在本次 digest tags 结果中（可能是扫描不完整或 digest/tag 不匹配）</div>
            ) : null}

            {digestSemverTags.length === 0 ? (
              <div className="muted">未找到可用于对比的 semver tags</div>
            ) : (
              <>
                <div className="muted">
                  semver 预览（按 semver 排序）：{semverMore > 0 ? `显示 ${semverPreview.length}，另有 ${semverMore} 个` : '全部'}
                </div>
                <div className="versionTagsPopoverChips">
                  {semverPreview.map((t) => (
                    <span key={t} className="versionTagsChip">
                      <span className={`mono${t === candidateTagTrim ? ' monoPrimary' : ''}`}>{t}</span>
                    </span>
                  ))}
                </div>
              </>
            )}
          </>
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
