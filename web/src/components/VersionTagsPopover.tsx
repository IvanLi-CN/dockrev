import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import {
  ApiError,
  forceRefreshServiceVersionInference,
  getServiceDigestTagsSnapshot,
  isServiceDigestTagsSnapshotPending,
  type ServiceDigestTagsScanSummary,
} from '../api'
import { normalizeDigest, shortenDigest } from './digest'

function uniquePreserveOrder(values: Array<string | null | undefined> | null | undefined): string[] {
  const out: string[] = []
  const seen = new Set<string>()
  for (const v of values ?? []) {
    const t = (v ?? '').trim()
    if (!t) continue
    if (seen.has(t)) continue
    seen.add(t)
    out.push(t)
  }
  return out
}

function moveToFront(tags: string[], tag: string): string[] {
  const t = tag.trim()
  if (!t) return tags
  const idx = tags.indexOf(t)
  if (idx <= 0) return tags
  return [tags[idx], ...tags.slice(0, idx), ...tags.slice(idx + 1)]
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

type SnapshotFetchPhase = 'idle' | 'loading' | 'ready' | 'missing' | 'error'

export function VersionTagsPopover(props: {
  serviceId: string
  candidateTag: string | null
  candidateDigest: string | null
  prefetchOnMount?: boolean
  children: ReactNode
}) {
  const { serviceId, candidateTag, candidateDigest, prefetchOnMount = false, children } = props
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
    checkedAt: null,
    missingSnapshot: false,
    error: null,
  }))
  const digestTags = digestState.key === digestKey ? digestState.tags : null
  const scan = digestState.key === digestKey ? digestState.scan : null
  const checkedAt = digestState.key === digestKey ? digestState.checkedAt : null
  const missingSnapshot = digestState.key === digestKey ? digestState.missingSnapshot : false
  const loadError = digestState.key === digestKey ? digestState.error : null
  const [snapshotPhase, setSnapshotPhase] = useState<SnapshotFetchPhase>('idle')
  const snapshotPhaseRef = useRef<SnapshotFetchPhase>(snapshotPhase)
  snapshotPhaseRef.current = snapshotPhase
  const [refreshing, setRefreshing] = useState(false)
  const [refreshNotice, setRefreshNotice] = useState<string | null>(null)
  const [refreshError, setRefreshError] = useState<string | null>(null)
  const candidateTagTrim = useMemo(() => (candidateTag ?? '').trim(), [candidateTag])

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

  const triggerForceRefresh = useCallback(async () => {
    if (refreshing) return
    setRefreshing(true)
    setRefreshError(null)
    setRefreshNotice(null)
    try {
      const resp = await forceRefreshServiceVersionInference(serviceId)
      setRefreshNotice(
        resp.reason === 'running'
          ? '已有版本推测任务在进行中。'
          : '已触发强制刷新，版本推测进行中。',
      )
      setDigestState({
        key: digestKey,
        tags: null,
        scan: null,
        checkedAt: null,
        missingSnapshot: false,
        error: null,
      })
      setSnapshotPhase('idle')
      window.dispatchEvent(
        new CustomEvent('dockrev:version-inference-refresh', {
          detail: { serviceId },
        }),
      )
    } catch (e: unknown) {
      setRefreshError(e instanceof Error ? e.message : String(e))
    } finally {
      setRefreshing(false)
    }
  }, [digestKey, refreshing, serviceId])

  useEffect(() => {
    const shouldPollSnapshot = prefetchOnMount || open || snapshotPhaseRef.current === 'loading'
    if (!shouldPollSnapshot) return
    if (!candidateTagTrim) return

    // Digest tag listing is only meaningful when digest is known.
    if (!candidateDigestNorm) return
    // Only fetch when there's no snapshot data loaded yet. Retries should be explicit
    // (e.g. via re-pinning), not continuously driven by pinned+error state.
    if (digestTags != null) return
    if (prefetchOnMount && snapshotPhaseRef.current === 'idle') setSnapshotPhase('loading')

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
        getServiceDigestTagsSnapshot(serviceId, candidateDigestNorm)
          .then((data) => {
            if (!alive) return
            if (isServiceDigestTagsSnapshotPending(data)) {
              setSnapshotPhase('loading')
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
            setSnapshotPhase('ready')
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
              setSnapshotPhase('missing')
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
            setSnapshotPhase('error')
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
  }, [candidateDigestNorm, candidateTagTrim, digestKey, digestTags, open, pinned, prefetchOnMount, serviceId])

  useEffect(() => {
    setSnapshotPhase('idle')
  }, [digestKey])

  const digestTagsUnique = useMemo(() => uniquePreserveOrder(digestTags), [digestTags])
  const tagsPreview = useMemo(() => {
    const base = digestTagsUnique
    const pinnedCandidate = candidateTagTrim ? moveToFront(base, candidateTagTrim) : base
    return pinnedCandidate.slice(0, TAGS_PREVIEW_MAX)
  }, [candidateTagTrim, digestTagsUnique])
  const tagsMore = useMemo(() => Math.max(0, digestTagsUnique.length - tagsPreview.length), [digestTagsUnique.length, tagsPreview.length])

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
    try {
      const p = navigator.clipboard?.writeText(t)
      if (p) void p.catch(() => {})
    } catch {
      // Ignore clipboard failures; copying is a best-effort convenience.
    }
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
        <div className="versionTagsPopoverActions">
          <button
            type="button"
            className="versionTagsPopoverAction"
            disabled={refreshing}
            onClick={() => {
              void triggerForceRefresh()
            }}
          >
            {refreshing ? '强制刷新中…' : '强制刷新'}
          </button>
        </div>
      </div>

      {refreshNotice ? <div className="muted">{refreshNotice}</div> : null}
      {refreshError ? <div className="muted">触发失败：{refreshError}</div> : null}

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
        ) : missingSnapshot ? (
          <div className="muted">快照缺失：请先执行一次 check（本气泡不再实时扫描 registry）</div>
        ) : digestTags == null ? (
          <div className="muted">读取扫描快照中…</div>
        ) : loadError ? (
          <div className="muted">读取失败：{loadError}</div>
        ) : digestTags.length === 0 ? (
          <div className="muted">未找到同 digest 的标签</div>
        ) : (
          <>
            <div className="muted">
              共 {digestTagsUnique.length} 个 tags
            </div>

            {checkedAt ? (
              <div className="muted">
                快照时间 <span className="mono">{checkedAt}</span>
              </div>
            ) : null}

            {scan && candidateDigestNorm && scan.repoTagsConsidered < scan.repoTagsTotal ? (
              <div className="muted">
                注意：仅比对最近 {scan.repoTagsConsidered} / {scan.repoTagsTotal} 个 tags，结果可能不完整
              </div>
            ) : null}

            {scan && candidateDigestNorm && (scan.manifestsTimeout > 0 || scan.manifestsError > 0) ? (
              <div className="muted">
                注意：digest tags 可能不完整（ok {scan.manifestsOk} / {scan.repoTagsConsidered}
                {scan.manifestsTimeout > 0 ? ` · timeout ${scan.manifestsTimeout}` : ''}
                {scan.manifestsError > 0 ? ` · error ${scan.manifestsError}` : ''}
                ）
              </div>
            ) : null}

            {candidateTagTrim && !digestTagsUnique.includes(candidateTagTrim) ? (
              <div className="muted">注意：候选 tag 不在本次 digest tags 结果中（可能是扫描不完整或 digest/tag 不匹配）</div>
            ) : null}

            <div className="muted">
              tags 预览：{tagsMore > 0 ? `显示 ${tagsPreview.length}，另有 ${tagsMore} 个` : '全部'}
            </div>
            <div className="versionTagsPopoverChips">
              {tagsPreview.map((t) => (
                <span key={t} className="versionTagsChip">
                  <span className={`mono${t === candidateTagTrim ? ' monoPrimary' : ''}`}>{t}</span>
                </span>
              ))}
            </div>
          </>
        )}
      </div>

    </div>
  ) : null

  const showLoadingTriggerLabel = Boolean(candidateDigestNorm && candidateTagTrim) && snapshotPhase === 'loading'
  const triggerClassName = showLoadingTriggerLabel
    ? 'versionTagsTrigger mono monoPrimary versionTagsTriggerLoading'
    : 'versionTagsTrigger mono monoPrimary'
  const triggerLabel = showLoadingTriggerLabel ? '加载中…' : children

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        className={triggerClassName}
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
            setSnapshotPhase('idle')
          }
          setHoverOpen(true)
          showPopover()
        }}
      >
        {triggerLabel}
      </button>
      {renderPopover ? createPortal(popoverBody, document.body) : null}
    </>
  )
}
