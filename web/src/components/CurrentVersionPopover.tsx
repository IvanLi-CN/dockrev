import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { listServiceDigestTags } from '../api'
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
  if (r) return r
  const t = tag.trim()
  if (t) return t
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
  repoTags: string[] | null
  error: string | null
}

type FilterState = {
  key: string
  value: string
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
  const { serviceId, imageTag, imageDigest, resolvedTag } = props
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

  const digestKey = useMemo(() => `${serviceId}:${digestNorm ?? ''}`, [digestNorm, serviceId])
  const [filterState, setFilterState] = useState<FilterState>(() => ({ key: digestKey, value: '' }))
  const tagFilter = filterState.key === digestKey ? filterState.value : ''

  const [digestState, setDigestState] = useState<DigestTagsState>(() => ({
    key: digestKey,
    tags: null,
    repoTags: null,
    error: null,
  }))
  const digestTags = digestState.key === digestKey ? digestState.tags : null
  const repoTags = digestState.key === digestKey ? digestState.repoTags : null
  const loadError = digestState.key === digestKey ? digestState.error : null

  const displayTag = useMemo(() => {
    const explicit = props.displayTag.trim()
    if (explicit) return explicit
    return inferredTagForDisplay(imageTag, resolvedTag)
  }, [imageTag, props.displayTag, resolvedTag])

  const resolvedTagTrim = useMemo(() => (resolvedTag ?? '').trim(), [resolvedTag])

  const rawSeries = useMemo(() => parseTagSeries(imageTag), [imageTag])

  const allDigestTags = useMemo(() => {
    return digestTags == null ? [] : sortTagsForDisplay(digestTags)
  }, [digestTags])

  const digestTagStats = useMemo(() => {
    if (digestTags == null) return null
    const total = allDigestTags.length
    const semverTotal = allDigestTags.filter(isStrictSemverTag).length
    return { total, semverTotal, otherTotal: total - semverTotal }
  }, [allDigestTags, digestTags])

  const seriesPatchStats = useMemo(() => {
    if (digestTags == null) return null
    if (!rawSeries || rawSeries.precision !== 2 || rawSeries.minor == null) return null
    const patches: number[] = []
    for (const t of allDigestTags) {
      const s = parseTagSeries(t)
      if (!s || s.precision !== 3 || s.minor == null || s.patch == null) continue
      if (s.major !== rawSeries.major || s.minor !== rawSeries.minor) continue
      patches.push(s.patch)
    }
    if (patches.length === 0) return null
    patches.sort((a, b) => a - b)
    const min = patches[0]
    const max = patches[patches.length - 1]
    return {
      count: patches.length,
      min,
      max,
      minTag: `${rawSeries.major}.${rawSeries.minor}.${min}`,
      maxTag: `${rawSeries.major}.${rawSeries.minor}.${max}`,
    }
  }, [allDigestTags, digestTags, rawSeries])

  const filteredDigestTags = useMemo(() => {
    if (digestTags == null) return []
    const q = tagFilter.trim().toLowerCase()
    if (!q) return allDigestTags
    return allDigestTags.filter((t) => t.toLowerCase().includes(q))
  }, [allDigestTags, digestTags, tagFilter])

  const allRepoTags = useMemo(() => {
    if (repoTags == null) return []
    const sorted = sortTagsForDisplay(repoTags)
    const current = (resolvedTagTrim || imageTag).trim()
    return current && sorted.includes(current) ? [current, ...sorted.filter((t) => t !== current)] : sorted
  }, [imageTag, repoTags, resolvedTagTrim])

  const repoTagStats = useMemo(() => {
    if (repoTags == null) return null
    const total = allRepoTags.length
    const semverTotal = allRepoTags.filter(isStrictSemverTag).length
    return { total, semverTotal, otherTotal: total - semverTotal }
  }, [allRepoTags, repoTags])

  const filteredRepoTags = useMemo(() => {
    if (repoTags == null) return []
    const q = tagFilter.trim().toLowerCase()
    if (!q) return allRepoTags
    return allRepoTags.filter((t) => t.toLowerCase().includes(q))
  }, [allRepoTags, repoTags, tagFilter])

  const showFilter = Math.max(allDigestTags.length, allRepoTags.length) > 20 || tagFilter.trim().length > 0

  useEffect(() => {
    if (!open) return

    // Always fetch the repo tag list (debug value) even when digest is missing.
    // Digest-matching tags are only available when digest is known.
    if (digestNorm) {
      if (digestTags != null && repoTags != null) return
    } else {
      if (repoTags != null) return
    }

    let alive = true
    const delay = pinned ? 0 : FETCH_DEBOUNCE_MS
    if (fetchTimer.current != null) window.clearTimeout(fetchTimer.current)
    fetchTimer.current = window.setTimeout(() => {
      setDigestState({ key: digestKey, tags: null, repoTags: null, error: null })
      listServiceDigestTags(serviceId, digestNorm ?? '')
        .then((data) => {
          if (!alive) return
          setDigestState({ key: digestKey, tags: data.tags, repoTags: data.repoTags ?? null, error: null })
        })
        .catch((e: unknown) => {
          if (!alive) return
          setDigestState({
            key: digestKey,
            tags: [],
            repoTags: null,
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
  }, [digestKey, digestNorm, digestTags, open, pinned, repoTags, serviceId])

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

  const semverDigestTags = useMemo(() => {
    if (!digestNorm) return null
    if (digestTags == null) return null
    return allDigestTags.filter(isStrictSemverTag)
  }, [allDigestTags, digestNorm, digestTags])

  const semverDigestPreview = useMemo(() => {
    if (!semverDigestTags) return null
    const max = 8
    const head = semverDigestTags.slice(0, max)
    const more = semverDigestTags.length > max ? semverDigestTags.length - max : 0
    return { head, more, total: semverDigestTags.length }
  }, [semverDigestTags])

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
    else if (!isStrictSemverTag(resolved)) reasons.push(`resolvedTag 非 semver（${resolved}）`)

    if (!rawTrim) reasons.push('raw tag 为空')
    else if (!isStrictSemverTag(rawTrim)) reasons.push(`raw tag 非 semver（${rawTrim}）`)

    if (!digestNorm) reasons.push('digest 未知')

    const lines: ReactNode[] = []
    lines.push(
      <div key="l1">
        推测 semver: <span className="mono">无法确定</span>
      </div>,
    )
    if (reasons.length > 0) {
      lines.push(
        <div key="l2">
          原因: {reasons.join(' · ')}
        </div>,
      )
    }

    if (rawSeries && rawSeries.precision === 2 && rawSeries.minor != null && seriesPatchStats) {
      if (seriesPatchStats.count > 1) {
        lines.push(
          <div key="series">
            raw tag 序列 <span className="mono">{`${rawSeries.major}.${rawSeries.minor}`}</span> 命中 {seriesPatchStats.count}{' '}
            个 patch tags（<span className="mono">{seriesPatchStats.minTag}</span>…<span className="mono">{seriesPatchStats.maxTag}</span>）
          </div>,
        )
      } else if (seriesPatchStats.count === 1) {
        lines.push(
          <div key="series">
            raw tag 序列 <span className="mono">{`${rawSeries.major}.${rawSeries.minor}`}</span> 仅命中 1 个 patch tag（
            <span className="mono">{seriesPatchStats.maxTag}</span>）
          </div>,
        )
      }
    }

    if (semverDigestPreview) {
      if (semverDigestPreview.total === 0) {
        lines.push(<div key="semver">同 digest 未找到 semver tags</div>)
      } else {
        const shown = semverDigestPreview.head.join(', ')
        lines.push(
          <div key="semver">
            同 digest semver tags: <span className="mono">{shown}</span>
            {semverDigestPreview.more > 0 ? ` …（+${semverDigestPreview.more}）` : ''}
          </div>,
        )
      }
    } else if (digestNorm && digestTags == null) {
      lines.push(<div key="semver">同 digest semver tags: 加载中…</div>)
    }

    return (
      <div className="muted" style={{ display: 'grid', gap: 4 }}>
        {lines}
      </div>
    )
  }, [digestNorm, digestTags, imageTag, preferSource, rawSeries, resolvedTagTrim, semverDigestPreview, seriesPatchStats])

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
            <span className="mono muted" title={digestNorm}>
              {shortenDigest(digestNorm)}
            </span>
          ) : (
            <span className="mono muted">digest 未知</span>
          )}
        </div>
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
        <div className="label">推测</div>
        {inferenceBlock}
      </div>

      {showFilter ? (
        <div className="versionTagsPopoverSection">
          <input
            className="versionTagsPopoverInput"
            value={tagFilter}
            onChange={(e) => setFilterState({ key: digestKey, value: e.target.value })}
            placeholder="过滤标签…"
          />
        </div>
      ) : null}

      <div className="versionTagsPopoverSection">
        <div className="label">同 digest 的 tags</div>
        {!digestNorm ? (
          <div className="muted">digest 未知，无法聚合标签</div>
        ) : digestTags == null ? (
          <div className="muted">加载中…</div>
        ) : loadError ? (
          <>
            <div className="muted">加载失败：{loadError}</div>
          </>
        ) : digestTags.length === 0 ? (
          <div className="muted">未找到同 digest 的标签</div>
        ) : (
          <>
            {digestTagStats ? (
              <div className="muted">
                共 {digestTagStats.total} 个标签（semver {digestTagStats.semverTotal} · 其他 {digestTagStats.otherTotal}）
              </div>
            ) : null}

            {tagFilter.trim().length > 0 ? (
              <div className="muted">
                匹配 {filteredDigestTags.length} / {allDigestTags.length}
              </div>
            ) : null}
            <pre className="versionTagsPopoverCode mono">{filteredDigestTags.join('\n')}</pre>
            <div className="versionTagsPopoverActions">
              {tagFilter.trim().length > 0 ? (
                <button
                  type="button"
                  className="versionTagsPopoverAction"
                  onClick={() => copyText(filteredDigestTags.join('\n'))}
                  disabled={filteredDigestTags.length === 0}
                >
                  复制（匹配）
                </button>
              ) : null}
              <button
                type="button"
                className="versionTagsPopoverAction"
                onClick={() => copyText(allDigestTags.join('\n'))}
                disabled={allDigestTags.length === 0}
              >
                复制（全部）
              </button>
            </div>
          </>
        )}
      </div>

      <div className="versionTagsPopoverSection">
        <div className="label">镜像所有标签{repoTagStats ? `（${repoTagStats.total}）` : ''}</div>
        {repoTags == null ? (
          <div className="muted">加载中…</div>
        ) : loadError ? (
          <div className="muted">加载失败：{loadError}</div>
        ) : allRepoTags.length === 0 ? (
          <div className="muted">未找到镜像标签</div>
        ) : (
          <>
            {tagFilter.trim().length > 0 ? (
              <div className="muted">
                匹配 {filteredRepoTags.length} / {allRepoTags.length}
              </div>
            ) : null}
            <pre className="versionTagsPopoverCode mono">{filteredRepoTags.join('\n')}</pre>
            <div className="versionTagsPopoverActions">
              {tagFilter.trim().length > 0 ? (
                <button
                  type="button"
                  className="versionTagsPopoverAction"
                  onClick={() => copyText(filteredRepoTags.join('\n'))}
                  disabled={filteredRepoTags.length === 0}
                >
                  复制（匹配）
                </button>
              ) : null}
              <button
                type="button"
                className="versionTagsPopoverAction"
                onClick={() => copyText(allRepoTags.join('\n'))}
                disabled={allRepoTags.length === 0}
              >
                复制（全部）
              </button>
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
          setPinned((prev) => {
            const next = !prev
            pinnedRef.current = next
            return next
          })
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
