import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import {
  ApiError,
  forceRefreshServiceVersionInference,
  getServiceDigestTagsSnapshot,
  isServiceDigestTagsSnapshotPending,
  type ServiceDigestTagsScanSummary,
} from '../api'
import { normalizeDigest, shortenDigest } from './digest'
import { useHoverPinnedPopover } from './HoverPinnedPopover'
import { inferResolvedTagsFromSnapshot, isStrictSemverTag } from '../versionDisplay'
import {
  getDigestSnapshotInvalidationToken,
  invalidateDigestSnapshot,
  subscribeDigestSnapshotInvalidation,
} from '../digestSnapshotBus'

type TagSeries = {
  major: number
  minor: number | null
  patch: number | null
  precision: 1 | 2 | 3
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

function inferredTagForDisplay(
  tag: string,
  resolvedTag: string | null | undefined,
): string {
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

function scanHasFailures(scan: ServiceDigestTagsScanSummary | null | undefined): boolean {
  if (!scan) return false
  return scan.manifestsTimeout > 0 || scan.manifestsError > 0
}

function scanIsComplete(scan: ServiceDigestTagsScanSummary | null | undefined): boolean {
  if (!scan) return false
  return scan.repoTagsConsidered >= scan.repoTagsTotal
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
  onLocalResolvedTags?: (update: {
    resolvedTag: string | null
    resolvedTags: string[] | null
  }) => void
  preferSource?: 'resolvedTag' | 'rawTag'
  // When true, treat the current display as a loading state caused by version inference pending.
  // This is intentionally separate from the digest snapshot pending/loading phase.
  inferenceLoading?: boolean
  triggerClassName?: string
  children?: ReactNode
}) {
  const {
    children,
    imageTag,
    imageDigest,
    onLocalResolvedTags,
    resolvedTag,
    serviceId,
  } = props
  const preferSource = props.preferSource ?? 'resolvedTag'
  const fetchTimer = useRef<number | null>(null)
  const {
    close,
    contentProps,
    open,
    pinned,
    popoverProps,
    togglePinned,
    triggerProps,
  } = useHoverPinnedPopover()

  const digestNorm = useMemo(() => normalizeDigest(imageDigest), [imageDigest])

  const displayTag = useMemo(() => {
    const explicit = props.displayTag.trim()
    if (explicit) return explicit
    return inferredTagForDisplay(imageTag, resolvedTag)
  }, [imageTag, props.displayTag, resolvedTag])

  const resolvedTagTrim = useMemo(
    () => (resolvedTag ?? '').trim(),
    [resolvedTag],
  )

  const rawSeries = useMemo(() => parseTagSeries(imageTag), [imageTag])

  const digestKey = useMemo(
    () => `${serviceId}:${digestNorm ?? ''}`,
    [digestNorm, serviceId],
  )
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
  const missingSnapshot =
    digestState.key === digestKey ? digestState.missingSnapshot : false
  const loadError = digestState.key === digestKey ? digestState.error : null
  const [snapshotPhase, setSnapshotPhase] = useState<SnapshotFetchPhase>('idle')
  const snapshotPhaseRef = useRef<SnapshotFetchPhase>(snapshotPhase)
  snapshotPhaseRef.current = snapshotPhase
  const ignoreInvalidationTokenRef = useRef<number>(0)
  const suppressLoadingLabelRef = useRef(false)
  const [refreshing, setRefreshing] = useState(false)
  const [refreshNotice, setRefreshNotice] = useState<string | null>(null)
  const [refreshError, setRefreshError] = useState<string | null>(null)
  const [localRefreshKey, setLocalRefreshKey] = useState<string | null>(null)
  const [externalRefreshKey, setExternalRefreshKey] = useState<string | null>(
    null,
  )
  const [snapshotFetchToken, setSnapshotFetchToken] = useState(0)
  const [localDisplayTag, setLocalDisplayTag] = useState<{
    key: string
    value: string | null
  }>({
    key: digestKey,
    value: null,
  })

  const digestTagsList = useMemo(
    () => uniquePreserveOrder(digestTags),
    [digestTags],
  )
  const snapshotInferredDisplayTag =
    localDisplayTag.key === digestKey && localDisplayTag.value
      ? localDisplayTag.value
      : null
  const snapshotDisplayOverride =
    preferSource !== 'rawTag' && snapshotInferredDisplayTag
      ? snapshotInferredDisplayTag
      : null
  const preferredResolvedTagTrim = snapshotInferredDisplayTag ?? resolvedTagTrim

  const effectiveTags = useMemo(() => {
    if (!digestNorm) return []
    return preferredResolvedTagTrim
      ? moveToFront(digestTagsList, preferredResolvedTagTrim)
      : digestTagsList
  }, [digestNorm, digestTagsList, preferredResolvedTagTrim])

  const tagsPreview = useMemo(
    () => effectiveTags.slice(0, TAGS_PREVIEW_MAX),
    [effectiveTags],
  )
  const tagsMore = useMemo(
    () => Math.max(0, effectiveTags.length - tagsPreview.length),
    [effectiveTags.length, tagsPreview.length],
  )

  useEffect(() => {
    return () => {
      if (fetchTimer.current != null) {
        window.clearTimeout(fetchTimer.current)
        fetchTimer.current = null
      }
    }
  }, [])

  useEffect(() => {
    if (!digestNorm) return
    return subscribeDigestSnapshotInvalidation(digestKey, (token) => {
      if (ignoreInvalidationTokenRef.current === token) {
        ignoreInvalidationTokenRef.current = 0
        return
      }

      suppressLoadingLabelRef.current = true
      setDigestState((prev) => {
        if (prev.key !== digestKey) return prev
        return {
          key: digestKey,
          tags: null,
          scan: null,
          checkedAt: null,
          missingSnapshot: false,
          error: null,
        }
      })
      setExternalRefreshKey(digestKey)
      setSnapshotFetchToken((value) => value + 1)
      setSnapshotPhase('loading')
    })
  }, [digestKey, digestNorm])

  const triggerForceRefresh = useCallback(async () => {
    if (refreshing || !digestNorm) return
    setRefreshing(true)
    setRefreshError(null)
    setRefreshNotice(null)

    if (fetchTimer.current != null) {
      window.clearTimeout(fetchTimer.current)
      fetchTimer.current = null
    }

    try {
      const resp = await forceRefreshServiceVersionInference(
        serviceId,
        digestNorm,
      )
      setRefreshNotice(
        resp.reason === 'running'
          ? '当前 digest 已有刷新任务在进行中。'
          : '已触发当前 digest 的强制刷新。',
      )
      setLocalRefreshKey(digestKey)
      setLocalDisplayTag({ key: digestKey, value: null })
      setSnapshotFetchToken((value) => value + 1)
      setDigestState({
        key: digestKey,
        tags: null,
        scan: null,
        checkedAt: null,
        missingSnapshot: false,
        error: null,
      })
      suppressLoadingLabelRef.current = false
      setSnapshotPhase('loading')
      const nextToken = getDigestSnapshotInvalidationToken(digestKey) + 1
      ignoreInvalidationTokenRef.current = nextToken
      invalidateDigestSnapshot(digestKey)
    } catch (e: unknown) {
      setRefreshError(e instanceof Error ? e.message : String(e))
    } finally {
      setRefreshing(false)
    }
  }, [digestKey, digestNorm, refreshing, serviceId])

  useEffect(() => {
    const shouldPollSnapshot = open || snapshotPhaseRef.current === 'loading'
    if (!shouldPollSnapshot) return
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
        getServiceDigestTagsSnapshot(serviceId, digestNorm)
          .then((data) => {
            if (!alive) return
            if (isServiceDigestTagsSnapshotPending(data)) {
              setSnapshotPhase('loading')
              const retryAfterMs = Math.max(
                200,
                Math.min(5000, Number(data.retryAfterMs) || FETCH_DEBOUNCE_MS),
              )
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
            const isLocalRefresh = localRefreshKey === digestKey
            const isExternalRefresh = externalRefreshKey === digestKey
            if (isLocalRefresh || isExternalRefresh) {
              const inferred = inferResolvedTagsFromSnapshot(data.tags, imageTag)
              const inferredFirst = inferred[0] ?? null
              const failures = scanHasFailures(data.scan)
              const complete = scanIsComplete(data.scan)

              // Only clear inferred tags when the snapshot scan is successful; preserve last-known
              // good inference values for all_failed/error snapshots.
              if (inferredFirst) {
                setLocalDisplayTag({
                  key: digestKey,
                  value: inferredFirst,
                })
              }
              if (isLocalRefresh && onLocalResolvedTags) {
                if (inferredFirst || (!failures && complete)) {
                  onLocalResolvedTags({
                    resolvedTag: inferredFirst,
                    resolvedTags: inferred.length > 1 ? inferred : null,
                  })
                }
              }
              if (isLocalRefresh) setLocalRefreshKey(null)
              if (isExternalRefresh) setExternalRefreshKey(null)
            }
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
              if (localRefreshKey === digestKey) setLocalRefreshKey(null)
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
            if (localRefreshKey === digestKey) setLocalRefreshKey(null)
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
  }, [
    digestKey,
    digestNorm,
    digestTags,
    imageTag,
    localRefreshKey,
    open,
    snapshotFetchToken,
    pinned,
    preferSource,
    onLocalResolvedTags,
    serviceId,
    externalRefreshKey,
  ])

  useEffect(() => {
    setSnapshotPhase('idle')
    setLocalRefreshKey(null)
    setLocalDisplayTag({ key: digestKey, value: null })
    setExternalRefreshKey(null)
    suppressLoadingLabelRef.current = false
  }, [digestKey])

  useEffect(() => {
    // The popover-local snapshot-derived display tag is only meant as a temporary UX bridge.
    // Only release the override once the parent actually reflects the same inferred value.
    // Otherwise we'd flash the new tag for a render and immediately snap back to the old one.
    if (localDisplayTag.key !== digestKey || !localDisplayTag.value) return
    const base =
      typeof children === 'string' ? children.trim() : displayTag.trim()
    if (!base) return
    if (base !== localDisplayTag.value.trim()) return
    setLocalDisplayTag({ key: digestKey, value: null })
  }, [children, digestKey, displayTag, localDisplayTag.key, localDisplayTag.value])

  const inferenceBlock = useMemo<ReactNode>(() => {
    const rawTrim = (imageTag ?? '').trim()
    const resolved = resolvedTagTrim

    if (snapshotInferredDisplayTag) {
      return (
        <div className="muted" style={{ display: 'grid', gap: 4 }}>
          <div>
            推测 semver:{' '}
            <span className="mono">{snapshotInferredDisplayTag}</span>
            {' · '}来源: <span className="mono">digest snapshot</span>
          </div>
        </div>
      )
    }

    const canUseResolvedSemver = Boolean(
      resolved && isStrictSemverTag(resolved),
    )
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
              {rawTrim
                ? `（raw tag 非 semver：${rawTrim}）`
                : '（raw tag 为空）'}
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
              {resolved
                ? `（resolvedTag 非 semver：${resolved}）`
                : '（resolvedTag 缺失）'}
            </div>
          </div>
        )
      }
    }

    const reasons: string[] = []
    if (!resolved) reasons.push('resolvedTag 缺失')
    else if (!isStrictSemverTag(resolved))
      reasons.push(`resolvedTag 非严格 semver（${resolved}）`)

    if (!rawTrim) reasons.push('raw tag 为空')
    else if (!isStrictSemverTag(rawTrim)) {
      if (rawSeries && rawSeries.precision !== 3) {
        const series =
          rawSeries.precision === 1 || rawSeries.minor == null
            ? `${rawSeries.major}`
            : `${rawSeries.major}.${rawSeries.minor}`
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
      lines.push(<div key="l2">原因:</div>)
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
  }, [
    digestNorm,
    imageTag,
    preferSource,
    rawSeries,
    resolvedTagTrim,
    snapshotInferredDisplayTag,
  ])

  const handleTriggerClick = () => {
    if (pinned) {
      close()
      return
    }
    const next = togglePinned()
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
  }

  const showInferenceLoadingStyle =
    preferSource !== 'rawTag' && Boolean(props.inferenceLoading)
  const effectiveDisplayTag = showInferenceLoadingStyle
    ? displayTag
    : (snapshotDisplayOverride ?? displayTag)
  const showSnapshotLoadingTriggerLabel =
    preferSource !== 'rawTag' &&
    Boolean(digestNorm) &&
    snapshotPhase === 'loading' &&
    (open || !suppressLoadingLabelRef.current)
  const showLoadingStyle =
    showSnapshotLoadingTriggerLabel || showInferenceLoadingStyle
  const triggerClassNameBase =
    props.triggerClassName ?? 'versionTagsTrigger mono monoPrimary'
  const triggerClassName = showLoadingStyle
    ? `${triggerClassNameBase} versionTagsTriggerLoading`
    : triggerClassNameBase
  const triggerLabel = showSnapshotLoadingTriggerLabel
    ? '加载中…'
    : (children ?? effectiveDisplayTag)

  return (
    <Popover {...popoverProps}>
      <PopoverTrigger asChild>
        <button
          {...triggerProps}
          type="button"
          className={triggerClassName}
          aria-haspopup="dialog"
          onClick={handleTriggerClick}
        >
          {triggerLabel}
        </button>
      </PopoverTrigger>
      <PopoverContent
        {...contentProps}
        align="start"
        aria-label="Current version"
        className="versionTagsPopover"
        forceMount
        sideOffset={8}
      >
        <div className="versionTagsPopoverHeader">
          <div className="versionTagsPopoverTitle">
            <span className="mono monoPrimary">{effectiveDisplayTag}</span>
            {digestNorm ? (
              <span className="mono muted">{shortenDigest(digestNorm)}</span>
            ) : (
              <span className="mono muted">digest 未知</span>
            )}
          </div>
          <div className="versionTagsPopoverActions">
            <button
              type="button"
              className="versionTagsPopoverAction"
              disabled={refreshing || !digestNorm}
              onClick={() => {
                void triggerForceRefresh()
              }}
            >
              {refreshing ? '强制刷新中…' : '强制刷新'}
            </button>
          </div>
        </div>

        {refreshNotice ? <div className="muted">{refreshNotice}</div> : null}
        {refreshError ? (
          <div className="muted">触发失败：{refreshError}</div>
        ) : null}

        <div className="versionTagsPopoverSection">
          <div className="label">推测</div>
          {inferenceBlock}
        </div>

        <div className="versionTagsPopoverSection">
          <div className="label">当前镜像</div>
          <div className="muted">
            raw tag{' '}
            <span className="mono">
              {imageTag.trim() ? imageTag : '（空）'}
            </span>
          </div>
          <div className="muted">
            resolvedTag{' '}
            <span className="mono">{resolvedTagTrim || '（缺失）'}</span>
          </div>
        </div>

        <div className="versionTagsPopoverSection">
          <div className="label">同 digest 的 tags</div>
          {!digestNorm && effectiveTags.length === 0 ? (
            <div className="muted">digest 未知，暂无 tags 信息</div>
          ) : digestNorm && missingSnapshot ? (
            <div className="muted">
              快照缺失：请先执行一次 check（本气泡不再实时扫描 registry）
            </div>
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

              {scan &&
              digestNorm &&
              scan.repoTagsConsidered < scan.repoTagsTotal ? (
                <div className="muted">
                  注意：仅比对最近 {scan.repoTagsConsidered} /{' '}
                  {scan.repoTagsTotal} 个 tags，结果可能不完整
                </div>
              ) : null}

              {scan &&
              digestNorm &&
              (scan.manifestsTimeout > 0 || scan.manifestsError > 0) ? (
                <div className="muted">
                  注意：digest tags 可能不完整（ok {scan.manifestsOk} /{' '}
                  {scan.repoTagsConsidered}
                  {scan.manifestsTimeout > 0
                    ? ` · timeout ${scan.manifestsTimeout}`
                    : ''}
                  {scan.manifestsError > 0
                    ? ` · error ${scan.manifestsError}`
                    : ''}
                  ）
                </div>
              ) : null}

              <div className="muted">
                tags 预览：
                {tagsMore > 0
                  ? `显示 ${tagsPreview.length}，另有 ${tagsMore} 个`
                  : '全部'}
              </div>
              <div className="versionTagsPopoverChips">
                {tagsPreview.map((t) => (
                  <span key={t} className="versionTagsChip">
                    <span
                      className={`mono${t === preferredResolvedTagTrim ? ' monoPrimary' : ''}`}
                    >
                      {t}
                    </span>
                  </span>
                ))}
              </div>
            </>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}
