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
import { inferResolvedTagsFromSnapshot } from '../versionDisplay'
import {
  getDigestSnapshotInvalidationToken,
  invalidateDigestSnapshot,
  subscribeDigestSnapshotInvalidation,
} from '../digestSnapshotBus'
import {
  trackDigestSnapshotRefresh,
} from '../digestInferenceTracker'

function uniquePreserveOrder(
  values: Array<string | null | undefined> | null | undefined,
): string[] {
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

function stableJitterMs(seed: string, maxMs: number): number {
  if (maxMs <= 0) return 0
  let hash = 0
  for (let i = 0; i < seed.length; i += 1) {
    hash = (hash * 31 + seed.charCodeAt(i)) >>> 0
  }
  return hash % (maxMs + 1)
}

const FETCH_DEBOUNCE_MS = 220
const PREFETCH_JITTER_MAX_MS = 180
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

export function VersionTagsPopover(props: {
  serviceId: string
  candidateTag: string | null
  candidateDigest: string | null
  prefetchOnMount?: boolean
  onLocalResolvedTag?: (resolvedTag: string | null) => void
  children: ReactNode
}) {
  const {
    serviceId,
    candidateTag,
    candidateDigest,
    prefetchOnMount = false,
    onLocalResolvedTag,
    children,
  } = props
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

  const candidateDigestNorm = useMemo(
    () => normalizeDigest(candidateDigest),
    [candidateDigest],
  )
  const digestKey = useMemo(
    () => `${serviceId}:candidate:${candidateDigestNorm ?? ''}`,
    [candidateDigestNorm, serviceId],
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
  const candidateTagTrim = useMemo(
    () => (candidateTag ?? '').trim(),
    [candidateTag],
  )
  const preferredCandidateTagTrim =
    localDisplayTag.key === digestKey && localDisplayTag.value
      ? localDisplayTag.value.trim()
      : candidateTagTrim

  useEffect(() => {
    return () => {
      if (fetchTimer.current != null) {
        window.clearTimeout(fetchTimer.current)
        fetchTimer.current = null
      }
    }
  }, [])

  useEffect(() => {
    if (!candidateDigestNorm) return
    return subscribeDigestSnapshotInvalidation(digestKey, (token) => {
      if (ignoreInvalidationTokenRef.current === token) {
        ignoreInvalidationTokenRef.current = 0
        return
      }

      if (fetchTimer.current != null) {
        window.clearTimeout(fetchTimer.current)
        fetchTimer.current = null
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
  }, [candidateDigestNorm, digestKey])

  const triggerForceRefresh = useCallback(async () => {
    if (refreshing || !candidateDigestNorm) return
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
        candidateDigestNorm,
      )
      setRefreshNotice(
        resp.reason === 'running'
          ? '候选 digest 已有刷新任务在进行中。'
          : '已触发候选 digest 的强制刷新。',
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
      trackDigestSnapshotRefresh({
        serviceId,
        imageRepo: resp.imageRepo,
        digest: candidateDigestNorm,
        side: 'candidate',
      })
    } catch (e: unknown) {
      setRefreshError(e instanceof Error ? e.message : String(e))
    } finally {
      setRefreshing(false)
    }
  }, [candidateDigestNorm, digestKey, refreshing, serviceId])

  useEffect(() => {
    const shouldPollSnapshot =
      prefetchOnMount || open || snapshotPhaseRef.current === 'loading'
    if (!shouldPollSnapshot) return
    if (!candidateTagTrim) return

    // Digest tag listing is only meaningful when digest is known.
    if (!candidateDigestNorm) return
    // Only fetch when there's no snapshot data loaded yet. Retries should be explicit
    // (e.g. via re-pinning), not continuously driven by pinned+error state.
    if (digestTags != null) return
    if (prefetchOnMount && snapshotPhaseRef.current !== 'loading')
      setSnapshotPhase('loading')

    let alive = true
    const prefetchJitter =
      prefetchOnMount && !open && !pinned
        ? stableJitterMs(
            `${serviceId}:${candidateDigestNorm}`,
            PREFETCH_JITTER_MAX_MS,
          )
        : 0
    const delay = (pinned ? 0 : FETCH_DEBOUNCE_MS) + prefetchJitter
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
              const inferred = inferResolvedTagsFromSnapshot(
                data.tags,
                candidateTagTrim,
              )
              const inferredFirst = inferred[0] ?? null
              const failures = scanHasFailures(data.scan)
              const complete = scanIsComplete(data.scan)

              // Only clear inferred tags when the snapshot scan is successful; preserve last-known
              // good inference values for all_failed/error snapshots.
              setLocalDisplayTag({ key: digestKey, value: inferredFirst })
              if (isLocalRefresh && onLocalResolvedTag) {
                if (inferredFirst || (!failures && complete)) onLocalResolvedTag(inferredFirst)
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
              const isLocalRefresh = localRefreshKey === digestKey
              const isExternalRefresh = externalRefreshKey === digestKey
              if (isLocalRefresh || isExternalRefresh) {
                setLocalDisplayTag({ key: digestKey, value: null })
              }
              if (isLocalRefresh) setLocalRefreshKey(null)
              if (isExternalRefresh) setExternalRefreshKey(null)
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
            const isLocalRefresh = localRefreshKey === digestKey
            const isExternalRefresh = externalRefreshKey === digestKey
            if (isLocalRefresh || isExternalRefresh) {
              setLocalDisplayTag({ key: digestKey, value: null })
            }
            if (isLocalRefresh) setLocalRefreshKey(null)
            if (isExternalRefresh) setExternalRefreshKey(null)
            setSnapshotPhase('error')
          })
      }

      poll()
    }, delay)
    fetchTimer.current = timerId

    return () => {
      alive = false
      // Preserve server-directed retry timers (set after 202 pending) across re-renders.
      // Only cancel the debounce timer created by this effect instance.
      if (fetchTimer.current === timerId) {
        window.clearTimeout(timerId)
        fetchTimer.current = null
      }
    }
  }, [
    candidateDigestNorm,
    candidateTagTrim,
    digestKey,
    digestTags,
    localRefreshKey,
    open,
    pinned,
    snapshotFetchToken,
    prefetchOnMount,
    onLocalResolvedTag,
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
    // Only release the override once the parent actually reflects the same inferred value.
    // Otherwise we'd flash the new tag for a render and immediately snap back.
    if (localDisplayTag.key !== digestKey || !localDisplayTag.value) return
    if (typeof children !== 'string') return
    const t = children.trim()
    if (!t) return
    if (t !== localDisplayTag.value.trim()) return
    setLocalDisplayTag({ key: digestKey, value: null })
  }, [children, digestKey, localDisplayTag.key, localDisplayTag.value])

  const digestTagsUnique = useMemo(
    () => uniquePreserveOrder(digestTags),
    [digestTags],
  )
  const tagsPreview = useMemo(() => {
    const base = digestTagsUnique
    const pinnedCandidate = preferredCandidateTagTrim
      ? moveToFront(base, preferredCandidateTagTrim)
      : base
    return pinnedCandidate.slice(0, TAGS_PREVIEW_MAX)
  }, [digestTagsUnique, preferredCandidateTagTrim])
  const tagsMore = useMemo(
    () => Math.max(0, digestTagsUnique.length - tagsPreview.length),
    [digestTagsUnique.length, tagsPreview.length],
  )

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

  const effectiveCandidateTag =
    localDisplayTag.key === digestKey && localDisplayTag.value
      ? localDisplayTag.value
      : candidateTag
  const showLoadingTriggerLabel =
    Boolean(candidateDigestNorm && candidateTagTrim) &&
    snapshotPhase === 'loading' &&
    (open || !suppressLoadingLabelRef.current)
  const triggerClassName = showLoadingTriggerLabel
    ? 'versionTagsTrigger mono monoPrimary versionTagsTriggerLoading'
    : 'versionTagsTrigger mono monoPrimary'
  const triggerLabel = showLoadingTriggerLabel
    ? '加载中…'
    : localDisplayTag.key === digestKey && localDisplayTag.value
      ? localDisplayTag.value
      : children

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
        aria-label="Version tags"
        className="versionTagsPopover"
        forceMount
        sideOffset={8}
      >
        <div className="versionTagsPopoverHeader">
          <div className="versionTagsPopoverTitle">
            <span className="mono monoPrimary">
              {effectiveCandidateTag ?? '无候选版本'}
            </span>
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
              disabled={refreshing || !candidateDigestNorm}
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
          <div className="label">参考信息</div>
          {!candidateTag ? (
            <div className="muted">无候选版本</div>
          ) : !candidateDigestNorm ? (
            <>
              <div className="muted">
                digest 缺失，无法列出同 digest 的 tags
              </div>
              <div className="versionTagsPopoverActions">
                <button
                  type="button"
                  className="versionTagsPopoverAction"
                  onClick={() => copyText(candidateTag)}
                >
                  复制
                </button>
              </div>
            </>
          ) : missingSnapshot ? (
            <div className="muted">
              快照缺失：请先执行一次 check（本气泡不再实时扫描 registry）
            </div>
          ) : digestTags == null ? (
            <div className="muted">读取扫描快照中…</div>
          ) : loadError ? (
            <div className="muted">读取失败：{loadError}</div>
          ) : digestTags.length === 0 ? (
            <div className="muted">未找到同 digest 的标签</div>
          ) : (
            <>
              <div className="muted">共 {digestTagsUnique.length} 个 tags</div>

              {checkedAt ? (
                <div className="muted">
                  快照时间 <span className="mono">{checkedAt}</span>
                </div>
              ) : null}

              {scan &&
              candidateDigestNorm &&
              scan.repoTagsConsidered < scan.repoTagsTotal ? (
                <div className="muted">
                  注意：仅比对最近 {scan.repoTagsConsidered} /{' '}
                  {scan.repoTagsTotal} 个 tags，结果可能不完整
                </div>
              ) : null}

              {scan &&
              candidateDigestNorm &&
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

              {candidateTagTrim &&
              !digestTagsUnique.includes(candidateTagTrim) ? (
                <div className="muted">
                  注意：候选 tag 不在本次 digest tags 结果中（可能是扫描不完整或
                  digest/tag 不匹配）
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
                      className={`mono${t === preferredCandidateTagTrim ? ' monoPrimary' : ''}`}
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
