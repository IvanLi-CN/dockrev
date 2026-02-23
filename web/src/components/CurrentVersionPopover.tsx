import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import {
  ApiError,
  getServiceDigestTagsSnapshot,
  isServiceDigestTagsSnapshotPending,
  type ServiceDigestTagsScanSummary,
} from '../api'
import { normalizeDigest, shortenDigest } from './digest'

type TagSeries = {
  major: number
  minor: number | null
  patch: number | null
  precision: 1 | 2 | 3
}

function isStrictSemverTag(tag: string): boolean {
  const t = tag.trim()
  if (!t) return false
  return /^v?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(t)
}

function parseTagSeries(tag: string): TagSeries | null {
  let t = tag.trim()
  if (!t) return null
  if (t.startsWith('v')) t = t.slice(1)
  if (!t) return null

  // Best-effort: accept semver-like core with optional prerelease/build.
  const core = t.split(/[+-]/, 1)[0]
  const parts = core.split('.')
  if (parts.length < 1 || parts.length > 3) return null
  if (!parts.every((p) => /^\d+$/.test(p))) return null

  const nums = parts.map((p) => Number(p))
  if (!nums.every((n) => Number.isFinite(n) && n >= 0)) return null

  return {
    major: nums[0],
    minor: parts.length >= 2 ? nums[1] : null,
    patch: parts.length >= 3 ? nums[2] : null,
    precision: parts.length as 1 | 2 | 3,
  }
}

function inferredTagForDisplay(tag: string, resolvedTag: string | null | undefined): string {
  const r = (resolvedTag ?? '').trim()
  if (r && isStrictSemverTag(r)) return r
  const t = tag.trim()
  if (t && isStrictSemverTag(t)) return t
  return '-'
}

function uniquePreserveOrder(values: string[] | null | undefined): string[] {
  const out: string[] = []
  const seen = new Set<string>()
  for (const v of values ?? []) {
    const t = v.trim()
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
const TAGS_PREVIEW_MAX = 12

type DigestTagsState = {
  key: string
  tags: string[] | null
  scan: ServiceDigestTagsScanSummary | null
  checkedAt: string | null
  missingSnapshot: boolean
  error: string | null
}

function moveToFront(tags: string[], tag: string): string[] {
  const t = tag.trim()
  if (!t) return tags
  const idx = tags.indexOf(t)
  if (idx <= 0) return tags
  return [tags[idx], ...tags.slice(0, idx), ...tags.slice(idx + 1)]
}

export function CurrentVersionPopover(props: {
  serviceId: string
  displayTag: string
  imageTag: string
  imageDigest?: string | null
  resolvedTag?: string | null
  resolvedTags?: string[] | null
  preferSource?: 'resolvedTag' | 'rawTag'
  triggerClassName?: string
  children?: ReactNode
}) {
  const { imageTag, imageDigest, resolvedTag } = props
  const preferSource = props.preferSource ?? 'resolvedTag'
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

  const digestNorm = useMemo(() => normalizeDigest(imageDigest), [imageDigest])

  const displayTag = useMemo(() => {
    const explicit = props.displayTag.trim()
    if (explicit) return explicit
    return inferredTagForDisplay(imageTag, resolvedTag)
  }, [imageTag, props.displayTag, resolvedTag])

  const resolvedTagTrim = useMemo(() => (resolvedTag ?? '').trim(), [resolvedTag])

  const rawSeries = useMemo(() => parseTagSeries(imageTag), [imageTag])

  const digestKey = useMemo(() => `${props.serviceId}:${digestNorm ?? ''}`, [digestNorm, props.serviceId])
  const [digestState, setDigestState] = useState<DigestTagsState>(() => ({
    key: digestKey,
    tags: null,
    scan: null,
    checkedAt: null,
    missingSnapshot: false,
    error: null,
  }))
  const digestTags = digestState.key === digestKey ? digestState.tags : null
  const scan = digestState.key === digestKey ? digestState.scan : null
  const checkedAt = digestState.key === digestKey ? digestState.checkedAt : null
  const missingSnapshot = digestState.key === digestKey ? digestState.missingSnapshot : false
  const loadError = digestState.key === digestKey ? digestState.error : null

  const digestTagsList = useMemo(() => uniquePreserveOrder(digestTags), [digestTags])

  const effectiveTags = useMemo(() => {
    if (!digestNorm) return []
    return resolvedTagTrim ? moveToFront(digestTagsList, resolvedTagTrim) : digestTagsList
  }, [digestNorm, digestTagsList, resolvedTagTrim])

  const tagsPreview = useMemo(() => effectiveTags.slice(0, TAGS_PREVIEW_MAX), [effectiveTags])
  const tagsMore = useMemo(() => Math.max(0, effectiveTags.length - tagsPreview.length), [effectiveTags.length, tagsPreview.length])

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
    if (!digestNorm) return
    // Only fetch when there's no snapshot data loaded yet. Retries should be explicit
    // (e.g. via re-pinning), not continuously driven by pinned+error state.
    if (digestTags != null) return

    let alive = true
    const delay = pinned ? 0 : FETCH_DEBOUNCE_MS
    if (fetchTimer.current != null) {
      window.clearTimeout(fetchTimer.current)
      fetchTimer.current = null
    }

    const timerId = window.setTimeout(() => {
      if (!alive) return
      // Avoid stale request finalizers / callbacks clobbering newer debounce timers.
      if (fetchTimer.current === timerId) fetchTimer.current = null

      const poll = () => {
        getServiceDigestTagsSnapshot(props.serviceId, digestNorm)
          .then((data) => {
            if (!alive) return
            if (isServiceDigestTagsSnapshotPending(data)) {
              const retryAfterMs = Math.max(200, Math.min(5000, Number(data.retryAfterMs) || FETCH_DEBOUNCE_MS))
              fetchTimer.current = window.setTimeout(() => {
                if (fetchTimer.current != null) fetchTimer.current = null
                poll()
              }, retryAfterMs)
              return
            }
            setDigestState({
              key: digestKey,
              tags: data.tags,
              scan: data.scan ?? null,
              checkedAt: data.checkedAt ?? null,
              missingSnapshot: false,
              error: null,
            })
          })
          .catch((e: unknown) => {
            if (!alive) return
            if (e instanceof ApiError && e.status === 404) {
              setDigestState({
                key: digestKey,
                tags: [],
                scan: null,
                checkedAt: null,
                missingSnapshot: true,
                error: null,
              })
              return
            }
            setDigestState({
              key: digestKey,
              tags: [],
              scan: null,
              checkedAt: null,
              missingSnapshot: false,
              error: e instanceof Error ? e.message : String(e),
            })
          })
      }

      poll()
    }, delay)
    fetchTimer.current = timerId

    return () => {
      alive = false
      if (fetchTimer.current === timerId) {
        window.clearTimeout(timerId)
        fetchTimer.current = null
      }
    }
  }, [digestKey, digestNorm, digestTags, open, pinned, props.serviceId])

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

  const inferenceBlock = useMemo<ReactNode>(() => {
    const rawTrim = (imageTag ?? '').trim()
    const resolved = resolvedTagTrim

    const canUseResolvedSemver = Boolean(resolved && isStrictSemverTag(resolved))
    const canUseRawSemver = Boolean(rawTrim && isStrictSemverTag(rawTrim))

    if (preferSource === 'rawTag') {
      if (canUseRawSemver) {
        return (
          <div className="muted" style={{ display: 'grid', gap: 4 }}>
            <div>
              推测 semver: <span className="mono">{rawTrim}</span>
              {' · '}来源: <span className="mono">raw tag</span>
            </div>
          </div>
        )
      }
      if (canUseResolvedSemver) {
        return (
          <div className="muted" style={{ display: 'grid', gap: 4 }}>
            <div>
              推测 semver: <span className="mono">{resolved}</span>
              {' · '}来源: <span className="mono">resolvedTag</span>
              {rawTrim ? `（raw tag 非 semver：${rawTrim}）` : '（raw tag 为空）'}
            </div>
          </div>
        )
      }
    } else {
      if (canUseResolvedSemver) {
        return (
          <div className="muted" style={{ display: 'grid', gap: 4 }}>
            <div>
              推测 semver: <span className="mono">{resolved}</span>
              {' · '}来源: <span className="mono">resolvedTag</span>
            </div>
          </div>
        )
      }
      if (canUseRawSemver) {
        return (
          <div className="muted" style={{ display: 'grid', gap: 4 }}>
            <div>
              推测 semver: <span className="mono">{rawTrim}</span>
              {' · '}来源: <span className="mono">raw tag</span>
              {resolved ? `（resolvedTag 非 semver：${resolved}）` : '（resolvedTag 缺失）'}
            </div>
          </div>
        )
      }
    }

    const reasons: string[] = []
    if (!resolved) reasons.push('resolvedTag 缺失')
    else if (!isStrictSemverTag(resolved)) reasons.push(`resolvedTag 非严格 semver（${resolved}）`)

    if (!rawTrim) reasons.push('raw tag 为空')
    else if (!isStrictSemverTag(rawTrim)) {
      if (rawSeries && rawSeries.precision !== 3) {
        const series =
          rawSeries.precision === 1 || rawSeries.minor == null ? `${rawSeries.major}` : `${rawSeries.major}.${rawSeries.minor}`
        reasons.push(`raw tag 缺少 patch（${rawTrim} -> ${series}.*）`)
      } else {
        reasons.push(`raw tag 非严格 semver（${rawTrim}）`)
      }
    }

    if (!digestNorm) reasons.push('digest 未知（无法反查 tags）')

    const lines: ReactNode[] = []
    lines.push(
      <div key="l1">
        推测 semver: <span className="mono">无法确定</span>
      </div>,
    )
    if (reasons.length > 0) {
      lines.push(
        <div key="l2">
          原因:
        </div>,
      )
      lines.push(
        <div key="reasons" className="versionTagsPopoverChips">
          {reasons.map((r) => (
            <span key={r} className="versionTagsChip">
              <span className="mono">{r}</span>
            </span>
          ))}
        </div>,
      )
    }

    return (
      <div className="muted" style={{ display: 'grid', gap: 4 }}>
        {lines}
      </div>
    )
  }, [digestNorm, imageTag, preferSource, rawSeries, resolvedTagTrim])

  const popoverBody = renderPopover ? (
    <div
      ref={popoverRef}
      className="versionTagsPopover"
      style={pos ? { left: pos.left, top: pos.top } : undefined}
      role="dialog"
      aria-label="Current version"
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
          <span className="mono monoPrimary">{displayTag}</span>
          {digestNorm ? (
            <span className="mono muted">
              {shortenDigest(digestNorm)}
            </span>
          ) : (
            <span className="mono muted">digest 未知</span>
          )}
        </div>
      </div>

      <div className="versionTagsPopoverSection">
        <div className="label">推测</div>
        {inferenceBlock}
      </div>

      <div className="versionTagsPopoverSection">
        <div className="label">当前镜像</div>
        <div className="muted">
          raw tag <span className="mono">{imageTag.trim() ? imageTag : '（空）'}</span>
        </div>
        <div className="muted">
          resolvedTag <span className="mono">{resolvedTagTrim || '（缺失）'}</span>
        </div>
      </div>

      <div className="versionTagsPopoverSection">
        <div className="label">同 digest 的 tags</div>
        {!digestNorm && effectiveTags.length === 0 ? (
          <div className="muted">digest 未知，暂无 tags 信息</div>
        ) : digestNorm && missingSnapshot ? (
          <div className="muted">快照缺失：请先执行一次 check（本气泡不再实时扫描 registry）</div>
        ) : digestNorm && digestTags == null && effectiveTags.length === 0 ? (
          <div className="muted">读取扫描快照中…</div>
        ) : loadError && effectiveTags.length === 0 ? (
          <div className="muted">读取失败：{loadError}</div>
        ) : effectiveTags.length === 0 ? (
          <div className="muted">未找到同 digest 的标签</div>
        ) : (
          <>
            <div className="muted">共 {effectiveTags.length} 个 tags</div>

            {checkedAt ? (
              <div className="muted">
                快照时间 <span className="mono">{checkedAt}</span>
              </div>
            ) : null}

            {scan && digestNorm && scan.repoTagsConsidered < scan.repoTagsTotal ? (
              <div className="muted">
                注意：仅比对最近 {scan.repoTagsConsidered} / {scan.repoTagsTotal} 个 tags，结果可能不完整
              </div>
            ) : null}

            {scan && digestNorm && (scan.manifestsTimeout > 0 || scan.manifestsError > 0) ? (
              <div className="muted">
                注意：digest tags 可能不完整（ok {scan.manifestsOk} / {scan.repoTagsConsidered}
                {scan.manifestsTimeout > 0 ? ` · timeout ${scan.manifestsTimeout}` : ''}
                {scan.manifestsError > 0 ? ` · error ${scan.manifestsError}` : ''}
                ）
              </div>
            ) : null}

            <div className="muted">
              tags 预览：{tagsMore > 0 ? `显示 ${tagsPreview.length}，另有 ${tagsMore} 个` : '全部'}
            </div>
            <div className="versionTagsPopoverChips">
              {tagsPreview.map((t) => (
                <span key={t} className="versionTagsChip">
                  <span className={`mono${t === resolvedTagTrim ? ' monoPrimary' : ''}`}>{t}</span>
                </span>
              ))}
            </div>
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
        className={props.triggerClassName ?? 'versionTagsTrigger mono monoPrimary'}
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
          const next = !pinnedRef.current
          pinnedRef.current = next
          setPinned(next)
          // If we previously failed to load (404/no snapshot yet, or other error),
          // treat pinning as an explicit one-shot retry by clearing state to "loading".
          if (next && (missingSnapshot || loadError)) {
            setDigestState({
              key: digestKey,
              tags: null,
              scan: null,
              checkedAt: null,
              missingSnapshot: false,
              error: null,
            })
          }
          setHoverOpen(true)
          showPopover()
        }}
      >
        {props.children ?? displayTag}
      </button>
      {renderPopover ? createPortal(popoverBody, document.body) : null}
    </>
  )
}
