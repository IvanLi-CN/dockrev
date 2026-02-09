import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { listServiceDigestTags, type ServiceDigestTagsResponse } from '../api'
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
  repoTags: string[] | null
  error: string | null
  scan: ServiceDigestTagsResponse['scan'] | null
}

type FilterState = {
  key: string
  value: string
}

type ExpandState = {
  key: string
  expanded: boolean | null
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
  const [filterState, setFilterState] = useState<FilterState>(() => ({ key: digestKey, value: '' }))
  const tagFilter = filterState.key === digestKey ? filterState.value : ''

  const [repoListState, setRepoListState] = useState<ExpandState>(() => ({ key: digestKey, expanded: null }))
  const repoListExpanded = repoListState.key === digestKey ? repoListState.expanded : null

  const [digestState, setDigestState] = useState<DigestTagsState>(() => ({
    key: digestKey,
    tags: null,
    repoTags: null,
    error: null,
    scan: null,
  }))
  const digestTags = digestState.key === digestKey ? digestState.tags : null
  const repoTags = digestState.key === digestKey ? digestState.repoTags : null
  const loadError = digestState.key === digestKey ? digestState.error : null
  const scan = digestState.key === digestKey ? digestState.scan : null

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

    // Always fetch the repo tag list (debug value) even when digest is missing.
    // Digest-matching tags are only available when digest is known.
    if (candidateDigestNorm) {
      if (digestTags != null && repoTags != null) return
    } else {
      if (repoTags != null) return
    }

    let alive = true
    const delay = pinned ? 0 : FETCH_DEBOUNCE_MS
    if (fetchTimer.current != null) window.clearTimeout(fetchTimer.current)
    fetchTimer.current = window.setTimeout(() => {
      setDigestState({ key: digestKey, tags: null, repoTags: null, error: null, scan: null })
      listServiceDigestTags(serviceId, candidateDigestNorm ?? '')
        .then((data) => {
          if (!alive) return
          setDigestState({ key: digestKey, tags: data.tags, repoTags: data.repoTags ?? null, error: null, scan: data.scan })
        })
        .catch((e: unknown) => {
          if (!alive) return
          setDigestState({
            key: digestKey,
            tags: [],
            repoTags: null,
            error: e instanceof Error ? e.message : String(e),
            scan: null,
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
  }, [candidateDigestNorm, candidateTag, digestKey, digestTags, open, pinned, repoTags, serviceId])

  const allTags = useMemo(() => {
    if (!candidateTag) return []
    if (!candidateDigestNorm) return [candidateTag]
    if (digestTags == null) return []
    const sorted = sortTagsForDisplay(digestTags)
    return sorted.includes(candidateTag) ? [candidateTag, ...sorted.filter((t) => t !== candidateTag)] : sorted
  }, [candidateDigestNorm, candidateTag, digestTags])

  const allRepoTags = useMemo(() => {
    if (!candidateTag) return []
    if (repoTags == null) return []
    const sorted = sortTagsForDisplay(repoTags)
    return sorted.includes(candidateTag) ? [candidateTag, ...sorted.filter((t) => t !== candidateTag)] : sorted
  }, [candidateTag, repoTags])

  const candidateInDigestTags = useMemo(() => {
    const ct = (candidateTag ?? '').trim()
    if (!ct) return null
    if (!candidateDigestNorm) return null
    if (digestTags == null) return null
    return digestTags.includes(ct)
  }, [candidateDigestNorm, candidateTag, digestTags])

  const candidateInRepoTags = useMemo(() => {
    const ct = (candidateTag ?? '').trim()
    if (!ct) return null
    if (repoTags == null) return null
    return repoTags.includes(ct)
  }, [candidateTag, repoTags])

  const tagStats = useMemo(() => {
    if (!candidateTag) return null
    const total = allTags.length
    const semverTotal = allTags.filter(isStrictSemverTag).length
    return { total, semverTotal, otherTotal: total - semverTotal }
  }, [allTags, candidateTag])

  const repoTagStats = useMemo(() => {
    if (!candidateTag) return null
    if (repoTags == null) return null
    const total = allRepoTags.length
    const semverTotal = allRepoTags.filter(isStrictSemverTag).length
    return { total, semverTotal, otherTotal: total - semverTotal }
  }, [allRepoTags, candidateTag, repoTags])

  const repoListExpandedEffective = useMemo(() => {
    if (tagFilter.trim().length > 0) return true
    if (repoListExpanded != null) return repoListExpanded
    return false
  }, [repoListExpanded, tagFilter])

  const filteredTags = useMemo(() => {
    if (!candidateTag) return []
    const q = tagFilter.trim().toLowerCase()
    if (!q) return allTags
    return allTags.filter((t) => t.toLowerCase().includes(q))
  }, [allTags, candidateTag, tagFilter])

  const filteredRepoTags = useMemo(() => {
    if (!candidateTag) return []
    const q = tagFilter.trim().toLowerCase()
    if (!q) return allRepoTags
    return allRepoTags.filter((t) => t.toLowerCase().includes(q))
  }, [allRepoTags, candidateTag, tagFilter])

  const showFilter = Math.max(allTags.length, allRepoTags.length) > 20 || tagFilter.trim().length > 0

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
            <span className="mono muted" title={candidateDigestNorm}>
              {shortenDigest(candidateDigestNorm)}
            </span>
          ) : (
            <span className="mono muted">digest 未知</span>
          )}
        </div>
      </div>

      <div className="versionTagsPopoverSection">
        <div className="label">同 digest 的 tags</div>
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
            <div className="muted">加载失败：{loadError}</div>
          </>
        ) : allTags.length === 0 ? (
          <div className="muted">未找到同 digest 的标签</div>
        ) : (
          <>
            {tagStats ? (
              <div className="muted">
                共 {tagStats.total} 个标签（semver {tagStats.semverTotal} · 其他 {tagStats.otherTotal}）
              </div>
            ) : null}
            {candidateInDigestTags === false ? (
              <div className="muted">注意：候选标签未出现在 digest-tags 列表中（digest mismatch 或扫描不完整）</div>
            ) : null}

            {showFilter ? (
              <input
                className="versionTagsPopoverInput"
                value={tagFilter}
                onChange={(e) => setFilterState({ key: digestKey, value: e.target.value })}
                placeholder="过滤标签…"
              />
            ) : null}
            {tagFilter.trim().length > 0 ? (
              <div className="muted">
                匹配 {filteredTags.length} / {allTags.length}
              </div>
            ) : null}
            <pre className="versionTagsPopoverCode mono">{filteredTags.join('\n')}</pre>
            <div className="versionTagsPopoverActions">
              {tagFilter.trim().length > 0 ? (
                <button
                  type="button"
                  className="versionTagsPopoverAction"
                  onClick={() => copyText(filteredTags.join('\n'))}
                  disabled={filteredTags.length === 0}
                >
                  复制（匹配）
                </button>
              ) : null}
              <button
                type="button"
                className="versionTagsPopoverAction"
                onClick={() => copyText(allTags.join('\n'))}
                disabled={allTags.length === 0}
              >
                复制（全部）
              </button>
            </div>
          </>
        )}
      </div>

      <div className="versionTagsPopoverSection">
        <div className="label">镜像所有标签{repoTagStats ? `（${repoTagStats.total}）` : ''}</div>
        {!candidateTag ? (
          <div className="muted">无候选版本</div>
        ) : repoTags == null ? (
          <div className="muted">加载中…</div>
        ) : loadError ? (
          <div className="muted">加载失败：{loadError}</div>
        ) : allRepoTags.length === 0 ? (
          <div className="muted">未找到镜像标签</div>
        ) : (
          <>
            {scan && candidateDigestNorm ? (
              <div className="muted">
                扫描 {scan.repoTagsTotal} 个标签（成功 {scan.manifestsOk} · 超时 {scan.manifestsTimeout} · 错误 {scan.manifestsError}）
                {scan.manifestsTimeout + scan.manifestsError > 0 ? ' · 可能不完整' : ''}
              </div>
            ) : null}
            {candidateInRepoTags === false ? (
              <div className="muted">注意：候选标签未出现在 registry 标签列表中（list_tags 不完整或标签已删除）</div>
            ) : null}

            <div className="versionTagsPopoverActions">
              <button
                type="button"
                className="versionTagsPopoverAction"
                onClick={() => setRepoListState({ key: digestKey, expanded: !repoListExpandedEffective })}
              >
                {repoListExpandedEffective ? '收起列表' : `展开列表（${allRepoTags.length}）`}
              </button>
              <button
                type="button"
                className="versionTagsPopoverAction"
                onClick={() => copyText(allRepoTags.join('\n'))}
                disabled={allRepoTags.length === 0}
              >
                复制（全部）
              </button>
            </div>

            {repoListExpandedEffective ? (
              <>
                {showFilter ? (
                  <input
                    className="versionTagsPopoverInput"
                    value={tagFilter}
                    onChange={(e) => setFilterState({ key: digestKey, value: e.target.value })}
                    placeholder="过滤标签…"
                  />
                ) : null}
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
                </div>
              </>
            ) : null}
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
