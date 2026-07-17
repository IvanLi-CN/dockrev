import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useVirtualizer, type VirtualItem } from '@tanstack/react-virtual'
import {
  ApiError,
  getServiceReleaseNotes,
  type JobListItem,
  type ReleaseNotesView,
  type Service,
  type ServiceBackupRecordItem,
  type ServiceReleaseNoteItem,
  type ServiceReleaseNotesResponse,
  type ServiceReleaseNotesStatus,
  type ServiceRollbackTargetResponse,
} from '../api'
import { useConfirm } from '../confirm'
import { cn } from '../lib/utils'
import { navigate } from '../routes'
import { blockedReasonFor, serviceRowStatus } from '../updateStatus'
import {
  compareStrictSemverTags,
  formatCandidateTagDisplay,
  formatCurrentTagDisplay,
} from '../versionDisplay'
import {
  releaseVersionForServiceOperation,
  selectServiceOperationJobs,
} from './RecentUpdateRecords'
import { summarizeServiceOperationBackups } from './serviceOperationBackupSummary'
import {
  describeDockrevVersionCardAction,
  type DockrevSelfUpgradeActionDescriptor,
  isDockrevService,
  rollbackVersionLabel,
  shortDigest,
} from '../pages/serviceDetailUtils'
import { Button, GitHubIcon, IconLink, Mono, OctoRillIcon } from '../ui'
import { ServiceVersionCard } from './ServiceVersionCard'
import {
  formatVersionDirectoryTimeLabel,
  mergeReleaseNoteItems,
  normalizeVersion,
  preferredReleaseTimestamp,
  releaseMatchesVersion,
  safeHttpUrl,
} from './serviceVersionsSectionUtils'

const RELEASES_PER_PAGE = 20
const RELEASE_ROW_GAP = 14
const VERSION_INDEX_ROW_HEIGHT = 54
const DESKTOP_VERSION_INDEX_QUERY = '(min-width: 1101px)'
const EMPTY_VIRTUAL_ITEMS: VirtualItem[] = []

type AnchorState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'found'; absoluteIndex: number }
  | { status: 'notFound'; message: string }
  | { status: 'unavailable'; message: string }

type ServiceVersionsSectionProps = {
  service: Service
  jobs: JobListItem[]
  backupRecords: ServiceBackupRecordItem[]
  rollbackTarget: ServiceRollbackTargetResponse | null
  rollbackTargetRefreshing: boolean
  busy: boolean
  dockrevSelfUpgradeAction: DockrevSelfUpgradeActionDescriptor | null
  updateActiveJob: { jobId: string; status: string } | null
  updateSubmitting: boolean
  rollbackActiveJobId: string | null
  rollbackActiveJobStatus: string | null
  onApplyUpdate: () => void
  onRollback: () => void
}

function releaseBodyForView(item: ServiceReleaseNoteItem, view: ReleaseNotesView): { body: string; missing: boolean } {
  const original = (item.originalBody ?? '').trim()
  if (view === 'translated') {
    const translated = (item.translatedBody ?? '').trim()
    return translated ? { body: translated, missing: false } : { body: original, missing: true }
  }
  if (view === 'smart') {
    const smart = (item.smartBody ?? '').trim()
    return smart ? { body: smart, missing: false } : { body: original, missing: true }
  }
  return { body: original, missing: false }
}

function viewLabel(view: ReleaseNotesView): string {
  if (view === 'original') return '原文'
  if (view === 'translated') return '翻译'
  return '润色'
}

function sourceLabel(response: ServiceReleaseNotesResponse | null | undefined): string {
  if (!response) return '未知'
  return response.source === 'octoRill' ? 'OctoRill' : 'GitHub Releases'
}

function shouldOfferSettingsAction(response: ServiceReleaseNotesResponse | null | undefined): boolean {
  const fallbackReason = response?.fallback?.reason
  if (fallbackReason === 'notConfigured' || fallbackReason === 'unauthorized') return true
  const message = response?.message ?? response?.fallback?.message ?? ''
  return message.includes('GitHub PAT') || message.includes('token 权限') || message.includes('OctoRill')
}

function fallbackReleaseErrorMessage(error: unknown): string {
  if (error instanceof ApiError && error.status === 404) {
    return '该服务不存在或已被删除，无法读取发布记录。'
  }
  if (error instanceof Error) {
    const message = error.message.trim()
    if (message) return message
  }
  return '发布记录拉取失败，请稍后重试。'
}

function buildListFailureResponse(
  error: unknown,
  cursor: string | null,
  limit: number,
): ServiceReleaseNotesResponse {
  return {
    status: 'upstreamError',
    source: 'gitHub',
    repo: null,
    cursor,
    limit,
    nextCursor: null,
    hasMore: false,
    defaultView: 'smart',
    items: [],
    message: fallbackReleaseErrorMessage(error),
  }
}

function updateLockReason(input: {
  busy: boolean
  updateSubmitting: boolean
  updateActiveJob: { jobId: string; status: string } | null
  rollbackActiveJobId: string | null
  rollbackTargetRefreshing: boolean
}): string | null {
  if (input.updateSubmitting && !input.updateActiveJob) {
    return '更新任务提交中，同一服务的版本动作暂不可再次点击。'
  }
  if (input.updateActiveJob) {
    return '当前服务已有更新任务进行中，请先查看现有任务状态。'
  }
  if (input.rollbackActiveJobId) {
    return '当前服务已有回滚任务进行中，请先等待它完成。'
  }
  if (input.rollbackTargetRefreshing) {
    return '回滚目标刷新中，版本动作暂不可点击。'
  }
  if (input.busy) {
    return '服务动作处理中，请稍后再试。'
  }
  return null
}

function fallbackTone(
  status: ServiceReleaseNotesStatus | 'info',
): 'warning' | 'danger' | 'success' {
  if (status === 'upstreamError') return 'danger'
  if (status === 'ready') return 'success'
  return 'warning'
}

function activityLabel(status: string | null | undefined, kind: 'update' | 'rollback'): string {
  if (status === 'queued') return '排队中…'
  return kind === 'update' ? '更新中…' : '回滚中…'
}

function resolveVirtualOffset(
  value: number | readonly [number, string] | null | undefined,
): number {
  if (typeof value === 'number') return value
  if (Array.isArray(value)) return value[0]
  return 0
}

function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() => {
    if (typeof window === 'undefined') return false
    return window.matchMedia(query).matches
  })

  useEffect(() => {
    if (typeof window === 'undefined') return
    const mediaQuery = window.matchMedia(query)
    const handleChange = () => setMatches(mediaQuery.matches)
    handleChange()
    mediaQuery.addEventListener('change', handleChange)
    return () => mediaQuery.removeEventListener('change', handleChange)
  }, [query])

  return matches
}

export function ServiceVersionsSection(props: ServiceVersionsSectionProps) {
  const confirm = useConfirm()
  const serviceId = props.service.id.trim()
  const listScrollRef = useRef<HTMLDivElement | null>(null)
  const indexScrollRef = useRef<HTMLDivElement | null>(null)
  const sessionKey = useMemo(() => {
    const anchorVersion = (props.service.image.resolvedTag ?? '').trim() || props.service.image.tag.trim()
    return `${serviceId}::${anchorVersion}`
  }, [props.service.image.resolvedTag, props.service.image.tag, serviceId])
  const showDesktopIndex = useMediaQuery(DESKTOP_VERSION_INDEX_QUERY)
  const currentVersion = useMemo(
    () => (props.service.image.resolvedTag ?? '').trim() || props.service.image.tag.trim() || null,
    [props.service.image.resolvedTag, props.service.image.tag],
  )
  const currentDisplayVersion = useMemo(
    () =>
      formatCurrentTagDisplay(
        props.service.image.tag,
        props.service.image.resolvedTag ?? null,
        props.service.versionInference?.status,
      ),
    [props.service.image.resolvedTag, props.service.image.tag, props.service.versionInference?.status],
  )
  const candidateVersion = useMemo(() => {
    const candidate = props.service.candidate
    if (!candidate) return null
    return (candidate.resolvedTag ?? '').trim() || candidate.tag.trim() || null
  }, [props.service.candidate])
  const candidateDisplayVersion = useMemo(() => {
    const candidate = props.service.candidate
    if (!candidate) return null
    return formatCandidateTagDisplay(
      candidate.tag,
      candidate.resolvedTag ?? null,
      props.service.versionInference?.status,
    )
  }, [props.service.candidate, props.service.versionInference?.status])
  const activeSessionRef = useRef<string | null>(sessionKey)
  const inFlightPagesRef = useRef<Map<string, Promise<ServiceReleaseNotesResponse | null>>>(new Map())
  const nextCursorRef = useRef<string | null>(null)
  const hasMoreRef = useRef(false)
  const loadingMoreRef = useRef(false)
  const initialCenterKeyRef = useRef<string | null>(null)

  const [initialLoadState, setInitialLoadState] = useState<'idle' | 'loading' | 'ready'>('idle')
  const [listResponse, setListResponse] = useState<ServiceReleaseNotesResponse | null>(null)
  const [items, setItems] = useState<ServiceReleaseNoteItem[]>([])
  const [expandedIds, setExpandedIds] = useState<Set<string>>(() => new Set())
  const [loadingMore, setLoadingMore] = useState(false)
  const [loadMoreFailure, setLoadMoreFailure] = useState<ServiceReleaseNotesResponse | null>(null)
  const [viewMode, setViewMode] = useState<ReleaseNotesView>('smart')
  const [selectedIndex, setSelectedIndex] = useState(0)
  const [anchorState, setAnchorState] = useState<AnchorState>(() =>
    currentVersion ? { status: 'loading' } : { status: 'idle' },
  )

  useEffect(() => {
    activeSessionRef.current = sessionKey
  }, [sessionKey])

  const resetState = useCallback(() => {
    inFlightPagesRef.current.clear()
    nextCursorRef.current = null
    hasMoreRef.current = false
    loadingMoreRef.current = false
    initialCenterKeyRef.current = null
    if (listScrollRef.current) listScrollRef.current.scrollTop = 0
    if (indexScrollRef.current) indexScrollRef.current.scrollTop = 0
    setInitialLoadState('idle')
    setListResponse(null)
    setItems([])
    setExpandedIds(new Set())
    setLoadingMore(false)
    setLoadMoreFailure(null)
    setViewMode('smart')
    setSelectedIndex(0)
    setAnchorState(currentVersion ? { status: 'loading' } : { status: 'idle' })
  }, [currentVersion])

  const requestPage = useCallback(
    async (expectedSession: string, cursor: string | null) => {
      const requestKey = `${expectedSession}:${cursor ?? 'first'}`
      const existing = inFlightPagesRef.current.get(requestKey)
      if (existing) return await existing

      const request = (async () => {
        let response: ServiceReleaseNotesResponse
        try {
          response = await getServiceReleaseNotes(serviceId, {
            cursor,
            limit: RELEASES_PER_PAGE,
          })
        } catch (error) {
          response = buildListFailureResponse(error, cursor, RELEASES_PER_PAGE)
        }
        if (activeSessionRef.current !== expectedSession) return null
        return response
      })()

      inFlightPagesRef.current.set(requestKey, request)
      try {
        return await request
      } finally {
        if (inFlightPagesRef.current.get(requestKey) === request) {
          inFlightPagesRef.current.delete(requestKey)
        }
      }
    },
    [serviceId],
  )

  useEffect(() => {
    resetState()
    if (!serviceId) return
    const expectedSession = sessionKey
    activeSessionRef.current = expectedSession
    setInitialLoadState('loading')

    let cancelled = false

    void (async () => {
      const firstResponse = await requestPage(expectedSession, null)
      if (!firstResponse || cancelled || activeSessionRef.current !== expectedSession) return

      setListResponse(firstResponse)
      if (firstResponse.status !== 'ready') {
        setInitialLoadState('ready')
        if (currentVersion) {
          setAnchorState({
            status: 'unavailable',
            message: firstResponse.message ?? '当前无法定位到服务版本，请稍后重试。',
          })
        }
        return
      }

      setViewMode(firstResponse.source === 'gitHub' ? 'original' : firstResponse.defaultView)

      let aggregated = [...firstResponse.items]
      let latestReadyResponse = firstResponse
      let foundIndex = currentVersion
        ? aggregated.findIndex((item) => releaseMatchesVersion(item, currentVersion))
        : -1

      setItems(aggregated)

      while (
        !cancelled &&
        activeSessionRef.current === expectedSession &&
        currentVersion &&
        foundIndex < 0 &&
        latestReadyResponse.hasMore &&
        latestReadyResponse.nextCursor
      ) {
        const nextResponse = await requestPage(expectedSession, latestReadyResponse.nextCursor)
        if (!nextResponse || cancelled || activeSessionRef.current !== expectedSession) return
        if (nextResponse.status !== 'ready') {
          setLoadMoreFailure(nextResponse)
          break
        }
        aggregated = mergeReleaseNoteItems(aggregated, nextResponse.items)
        latestReadyResponse = nextResponse
        foundIndex = aggregated.findIndex((item) => releaseMatchesVersion(item, currentVersion))
        setItems(aggregated)
      }

      nextCursorRef.current = latestReadyResponse.nextCursor ?? null
      hasMoreRef.current = latestReadyResponse.hasMore && nextCursorRef.current != null
      setInitialLoadState('ready')

      if (!currentVersion) {
        setAnchorState({
          status: 'unavailable',
          message: '当前服务版本暂不可定位，已从最新发布开始展示。',
        })
        return
      }

      if (foundIndex >= 0) {
        setAnchorState({ status: 'found', absoluteIndex: foundIndex })
        return
      }

      if (latestReadyResponse.hasMore && latestReadyResponse.nextCursor) {
        setAnchorState({
          status: 'unavailable',
          message: `当前版本 ${currentVersion} 定位被中断，请继续向下滚动加载更旧版本。`,
        })
        return
      }

      setAnchorState({
        status: 'notFound',
        message: `当前版本 ${currentVersion} 未在发布记录中找到，已从顶部开始浏览。`,
      })
    })()

    return () => {
      cancelled = true
    }
  }, [currentVersion, requestPage, resetState, serviceId, sessionKey])

  const rowCount = items.length + ((hasMoreRef.current || loadingMore || loadMoreFailure) ? 1 : 0)
  const listVirtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => listScrollRef.current,
    estimateSize: () => 360,
    overscan: 6,
    gap: RELEASE_ROW_GAP,
    getItemKey: (index) => (index < items.length ? items[index]?.id ?? index : `loader:${index}`),
    measureElement: (element) => element.getBoundingClientRect().height,
  })
  const indexVirtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => indexScrollRef.current,
    estimateSize: () => VERSION_INDEX_ROW_HEIGHT,
    overscan: 10,
    gap: 8,
    getItemKey: (index) => (index < items.length ? items[index]?.id ?? index : `index-loader:${index}`),
  })

  useEffect(() => {
    listVirtualizer.measure()
  }, [expandedIds, items.length, viewMode, listVirtualizer])

  useEffect(() => {
    const element = listScrollRef.current
    if (!element || typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(() => {
      listVirtualizer.measure()
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [listVirtualizer])

  const centerVersionCard = useCallback(
    (absoluteIndex: number, tagName: string, mode: 'initial' | 'interactive') => {
      const scrollElement = listScrollRef.current
      if (!scrollElement || !tagName) return
      let frameA = 0
      let frameB = 0
      let retryTimer = 0
      const key = `${sessionKey}:${absoluteIndex}`
      const centerCard = (attemptsRemaining: number) => {
        const element = listScrollRef.current
        if (!element) return
        listVirtualizer.measure()
        const renderedCard = Array.from(
          element.querySelectorAll<HTMLElement>('[data-service-version-card="true"]'),
        ).find((node) => node.getAttribute('data-release-tag') === tagName)
        let targetOffset = resolveVirtualOffset(
          listVirtualizer.getOffsetForIndex(absoluteIndex, 'center'),
        )
        if (renderedCard) {
          const viewportRect = element.getBoundingClientRect()
          const cardRect = renderedCard.getBoundingClientRect()
          targetOffset =
            element.scrollTop +
            (cardRect.top - viewportRect.top) -
            Math.max(0, (viewportRect.height - cardRect.height) / 2)
        }
        targetOffset = Math.max(0, targetOffset)
        element.scrollTo({ top: targetOffset })
        if (showDesktopIndex) {
          indexVirtualizer.scrollToIndex(absoluteIndex, {
            align: mode === 'initial' ? 'center' : 'auto',
          })
        }
        if (attemptsRemaining <= 0 || !renderedCard) {
          if (mode === 'initial') initialCenterKeyRef.current = key
          return
        }
        const viewportRect = element.getBoundingClientRect()
        const cardRect = renderedCard.getBoundingClientRect()
        const viewportCenter = viewportRect.top + viewportRect.height / 2
        const cardCenter = cardRect.top + cardRect.height / 2
        if (Math.abs(cardCenter - viewportCenter) <= Math.max(48, viewportRect.height * 0.18)) {
          if (mode === 'initial') initialCenterKeyRef.current = key
          return
        }
        retryTimer = window.setTimeout(() => {
          centerCard(attemptsRemaining - 1)
        }, 80)
      }

      frameA = window.requestAnimationFrame(() => {
        frameB = window.requestAnimationFrame(() => {
          centerCard(6)
        })
      })
      return () => {
        window.cancelAnimationFrame(frameA)
        window.cancelAnimationFrame(frameB)
        window.clearTimeout(retryTimer)
      }
    },
    [indexVirtualizer, listVirtualizer, sessionKey, showDesktopIndex],
  )

  useEffect(() => {
    if (initialLoadState !== 'ready') return
    if (anchorState.status !== 'found') return
    if (items.length <= anchorState.absoluteIndex) return
    const scrollElement = listScrollRef.current
    if (!scrollElement) return
    const key = `${sessionKey}:${anchorState.absoluteIndex}`
    if (initialCenterKeyRef.current === key && scrollElement.scrollTop > 0) return
    setSelectedIndex(anchorState.absoluteIndex)
    return centerVersionCard(
      anchorState.absoluteIndex,
      items[anchorState.absoluteIndex]?.tagName ?? '',
      'initial',
    )
  }, [anchorState, centerVersionCard, initialLoadState, items, sessionKey])

  const listVirtualItems = listVirtualizer.getVirtualItems()
  const indexVirtualItems = showDesktopIndex ? indexVirtualizer.getVirtualItems() : EMPTY_VIRTUAL_ITEMS
  const listOffset = listVirtualItems[0]?.start ?? 0
  const indexOffset = indexVirtualItems[0]?.start ?? 0

  const loadNextPage = useCallback(async () => {
    if (!serviceId || loadingMoreRef.current || loadingMore || !hasMoreRef.current) return
    const expectedSession = sessionKey
    const nextCursor = nextCursorRef.current
    if (!nextCursor) return

    loadingMoreRef.current = true
    setLoadingMore(true)
    try {
      const response = await requestPage(expectedSession, nextCursor)
      if (!response || activeSessionRef.current !== expectedSession) return
      if (response.status !== 'ready') {
        setLoadMoreFailure(response)
        return
      }
      nextCursorRef.current = response.nextCursor ?? null
      hasMoreRef.current = response.hasMore && nextCursorRef.current != null
      setLoadMoreFailure(null)
      const mergedItems = mergeReleaseNoteItems(items, response.items)
      setItems(mergedItems)
      if (currentVersion) {
        const foundIndex = mergedItems.findIndex((item) => releaseMatchesVersion(item, currentVersion))
        if (foundIndex >= 0) {
          setAnchorState({ status: 'found', absoluteIndex: foundIndex })
          setSelectedIndex(foundIndex)
        } else if (!response.hasMore || !response.nextCursor) {
          setAnchorState({
            status: 'notFound',
            message: `当前版本 ${currentVersion} 未在发布记录中找到，已从顶部开始浏览。`,
          })
        }
      }
    } finally {
      if (activeSessionRef.current === expectedSession) {
        loadingMoreRef.current = false
        setLoadingMore(false)
      }
    }
  }, [currentVersion, items, loadingMore, requestPage, serviceId, sessionKey])

  useEffect(() => {
    const lastListItem = [...listVirtualItems].reverse()[0]
    const lastIndexItem = [...indexVirtualItems].reverse()[0]
    const tailIndex = Math.max(lastListItem?.index ?? -1, lastIndexItem?.index ?? -1)
    if (tailIndex < items.length - 1) return
    if (!hasMoreRef.current || loadingMore || loadMoreFailure || initialLoadState !== 'ready') return
    void loadNextPage()
  }, [
    indexVirtualItems,
    initialLoadState,
    items.length,
    listVirtualItems,
    loadMoreFailure,
    loadNextPage,
    loadingMore,
  ])

  useEffect(() => {
    if (items.length === 0) return
    const scrollElement = listScrollRef.current
    if (!scrollElement) return
    const visibleRows = listVirtualItems.filter((row) => row.index < items.length)
    if (visibleRows.length === 0) return
    const viewportCenter = scrollElement.scrollTop + scrollElement.clientHeight / 2
    let nearestIndex = visibleRows[0]?.index ?? 0
    let nearestDistance = Number.POSITIVE_INFINITY
    for (const row of visibleRows) {
      const rowCenter = row.start + row.size / 2
      const distance = Math.abs(rowCenter - viewportCenter)
      if (distance < nearestDistance) {
        nearestDistance = distance
        nearestIndex = row.index
      }
    }
    setSelectedIndex((prev) => (prev === nearestIndex ? prev : nearestIndex))
  }, [items.length, listVirtualItems])

  useEffect(() => {
    if (!showDesktopIndex) return
    if (selectedIndex >= items.length) return
    indexVirtualizer.scrollToIndex(selectedIndex, { align: 'auto' })
  }, [indexVirtualizer, items.length, selectedIndex, showDesktopIndex])

  const currentVersionNorm = normalizeVersion(currentVersion)
  const deployedHistoricalVersions = useMemo(() => {
    const versions = new Set<string>()
    for (const job of selectServiceOperationJobs(props.jobs, serviceId)) {
      if (job.status !== 'success' && job.status !== 'rolled_back') continue
      const version = releaseVersionForServiceOperation(job, serviceId)
      const normalized = normalizeVersion(version)
      if (normalized) versions.add(normalized)
    }
    const rollbackTargetVersion = normalizeVersion(props.rollbackTarget?.targetDisplayTag)
    if (rollbackTargetVersion) versions.add(rollbackTargetVersion)
    versions.delete(currentVersionNorm)
    return versions
  }, [currentVersionNorm, props.jobs, props.rollbackTarget?.targetDisplayTag, serviceId])

  const serviceActionLockReason = useMemo(
    () =>
      updateLockReason({
        busy: props.busy,
        updateSubmitting: props.updateSubmitting,
        updateActiveJob: props.updateActiveJob,
        rollbackActiveJobId: props.rollbackActiveJobId,
        rollbackTargetRefreshing: props.rollbackTargetRefreshing,
      }),
    [
      props.busy,
      props.rollbackActiveJobId,
      props.rollbackTargetRefreshing,
      props.updateActiveJob,
      props.updateSubmitting,
    ],
  )

  const showSettingsAction = shouldOfferSettingsAction(listResponse)
  const fallbackBanner = listResponse?.fallback
    ? { tone: 'warning' as const, message: listResponse.fallback.message }
    : null
  const anchorBanner =
    anchorState.status === 'notFound' || anchorState.status === 'unavailable'
      ? { tone: 'warning' as const, message: anchorState.message }
      : null
  const listBanner =
    listResponse && listResponse.status !== 'ready' && listResponse.message
      ? {
          tone: fallbackTone(listResponse.status),
          message: listResponse.message,
        }
      : null

  const activeTaskNotice = useMemo(() => {
    if (props.updateSubmitting && !props.updateActiveJob) {
      return {
        title: '更新任务提交中',
        detail: '同一服务只允许一个版本动作在执行，列表内按钮暂不可再次点击。',
        jobId: null,
      }
    }
    if (props.updateActiveJob) {
      return {
        title: activityLabel(props.updateActiveJob.status, 'update'),
        detail: '当前服务已有更新任务在执行，版本列表动作已锁定。',
        jobId: props.updateActiveJob.jobId,
      }
    }
    if (props.rollbackActiveJobId) {
      return {
        title: activityLabel(props.rollbackActiveJobStatus, 'rollback'),
        detail: '当前服务已有回滚任务在执行，版本列表动作已锁定。',
        jobId: props.rollbackActiveJobId,
      }
    }
    if (props.rollbackTargetRefreshing) {
      return {
        title: '回滚目标刷新中',
        detail: '等待后端重新解析可执行回滚目标期间，版本动作暂不可点击。',
        jobId: null,
      }
    }
    return null
  }, [
    props.rollbackActiveJobId,
    props.rollbackActiveJobStatus,
    props.rollbackTargetRefreshing,
    props.updateActiveJob,
    props.updateSubmitting,
  ])

  const toggleExpanded = useCallback((id: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }, [])

  const openRollbackExplanation = useCallback(
    async (item: ServiceReleaseNoteItem) => {
      const executableTarget =
        props.rollbackTarget?.targetDigest && props.rollbackTarget?.targetDisplayTag
          ? `${rollbackVersionLabel(props.rollbackTarget.targetDisplayTag, props.rollbackTarget.targetDigest)} · ${shortDigest(props.rollbackTarget.targetDigest)}`
          : null
      await confirm({
        title: `这个版本现在不能直接回滚到 ${item.tagName}`,
        body: (
          <>
            <div className="modalLead">Dockrev 当前只允许回滚到后端解析出的单一 rollback target，不支持从版本页直接挑任意历史版本创建任务。</div>
            <div className="modalKvGrid">
              <div className="modalKvLabel">点击版本</div>
              <div className="modalKvValue">
                <Mono>{item.tagName}</Mono>
              </div>
              <div className="modalKvLabel">当前版本</div>
              <div className="modalKvValue">
                <Mono>{currentDisplayVersion}</Mono>
              </div>
              <div className="modalKvLabel">可执行回滚目标</div>
              <div className="modalKvValue">
                {executableTarget ? <Mono>{executableTarget}</Mono> : <span className="muted">当前没有可执行的 rollback target</span>}
              </div>
              <div className="modalKvLabel">说明</div>
              <div className="modalKvValue">
                <span>这个入口只提供历史部署解释，不会创建任务。</span>
              </div>
            </div>
          </>
        ),
        confirmText: '知道了',
        cancelText: '关闭',
        confirmVariant: 'ghost',
        badgeText: null,
      })
    },
    [confirm, currentDisplayVersion, props.rollbackTarget],
  )

  const cards = useMemo(() => {
    const currentComparableVersion = (props.service.image.resolvedTag ?? '').trim() || props.service.image.tag.trim()
    const candidateComparableVersion = candidateVersion
    const rollbackTargetVersion = normalizeVersion(props.rollbackTarget?.targetDisplayTag)
    const dockrevService = isDockrevService(props.service)

    return items.map((item) => {
      const currentMatch = releaseMatchesVersion(item, currentVersion)
      const candidateMatch = releaseMatchesVersion(item, candidateComparableVersion)
      const semverComparison = compareStrictSemverTags(item.tagName, currentComparableVersion)
      const olderThanCurrent = semverComparison != null && semverComparison < 0
      const newerThanCurrent = semverComparison != null && semverComparison > 0
      const deployedHistorical = deployedHistoricalVersions.has(normalizeVersion(item.tagName))
      const rollbackTargetMatch = rollbackTargetVersion !== '' && normalizeVersion(item.tagName) === rollbackTargetVersion
      const showUpdate = dockrevService ? candidateMatch || newerThanCurrent : newerThanCurrent || candidateMatch
      const showRollback = !dockrevService && (deployedHistorical || rollbackTargetMatch)

      let updateDisabledReason: string | null = null
      let updateDisabled = false
      let updateActionLabel = '更新'
      let updateActionHint = '发起当前 candidate 对应的服务更新任务。'
      if (showUpdate) {
        if (dockrevService) {
          const dockrevAction = describeDockrevVersionCardAction({
            candidateMatch,
            candidateDisplayVersion,
            action: props.dockrevSelfUpgradeAction,
          })
          updateActionLabel = dockrevAction.label
          updateDisabled = dockrevAction.disabled
          updateDisabledReason = dockrevAction.disabledReason
          updateActionHint = dockrevAction.hint
        } else {
          updateDisabledReason =
            serviceActionLockReason ??
            (serviceRowStatus(props.service) === 'blocked'
              ? blockedReasonFor(props.service) ?? '当前服务已被阻止更新。'
              : !props.service.candidate
                ? '当前没有可执行的候选版本。'
                : props.service.candidate.archMatch === 'mismatch'
                  ? '架构不匹配（仅提示，不允许更新）。'
                  : !candidateMatch
                    ? '当前只允许部署现有 candidate 对应版本，不能直接从发布记录跨 tag 发起更新。'
                    : null)
          updateDisabled = updateDisabledReason != null
          updateActionHint = updateDisabledReason ?? '发起当前 candidate 对应的服务更新任务。'
        }
      }

      const rollbackDisabledReason = showRollback ? serviceActionLockReason : null
      const { body, missing } = releaseBodyForView(item, viewMode)
      return {
        item,
        body,
        bodyMissing: missing,
        currentMatch,
        candidateMatch,
        deployedHistorical,
        rollbackTargetMatch,
        olderThanCurrent,
        showUpdate,
        showRollback,
        updateDisabled,
        updateActionHint,
        updateActionLabel,
        updateDisabledReason,
        rollbackDisabledReason,
      }
    })
  }, [
    candidateDisplayVersion,
    candidateVersion,
    currentVersion,
    deployedHistoricalVersions,
    items,
    props.rollbackTarget?.targetDisplayTag,
    props.dockrevSelfUpgradeAction,
    props.service,
    serviceActionLockReason,
    viewMode,
  ])

  const renderedCardCount = listVirtualItems.filter((item) => item.index < cards.length).length
  const renderedIndexCount = indexVirtualItems.filter((item) => item.index < items.length).length
  const githubReleasesUrl = safeHttpUrl(listResponse?.externalLinks?.githubReleasesUrl)
  const octoRillReleasesUrl = safeHttpUrl(listResponse?.externalLinks?.octoRillReleasesUrl)
  const rollbackBackupSummaryByJobId = useMemo(
    () => summarizeServiceOperationBackups(props.backupRecords),
    [props.backupRecords],
  )
  const rollbackBackupSummary = useMemo(() => {
    const sourceJobId = (props.rollbackTarget?.sourceUpdateJobId ?? '').trim()
    if (!sourceJobId) return { state: 'empty' as const }
    return rollbackBackupSummaryByJobId.get(sourceJobId) ?? { state: 'empty' as const }
  }, [props.rollbackTarget?.sourceUpdateJobId, rollbackBackupSummaryByJobId])
  const openSettings = () => navigate({ name: 'settings' })

  return (
    <section className="serviceVersionsSection" data-service-detail-section-card="versions">
      <div className="serviceVersionsCard">
        <div className="serviceVersionsHeader">
          <div className="serviceVersionsHeaderText">
            <div className="title">版本</div>
            <div className="muted">
              以当前部署版本为锚点浏览统一 release notes；较新版本提供更新入口，只有当前 rollback target 保留回滚入口。
            </div>
          </div>
          <div className="serviceVersionsHeaderControls">
            {githubReleasesUrl ? (
              <IconLink
                className="serviceVersionsSourceLink"
                href={githubReleasesUrl}
                iconKind="github"
                linkKind="repo"
                title="打开 GitHub Releases"
              >
                <GitHubIcon className="brandIcon" />
              </IconLink>
            ) : null}
            {octoRillReleasesUrl ? (
              <IconLink
                className="serviceVersionsSourceLink"
                href={octoRillReleasesUrl}
                iconKind="octorill"
                linkKind="repo"
                title="打开 OctoRill Releases"
              >
                <OctoRillIcon className="brandIcon" />
              </IconLink>
            ) : null}
            {listResponse?.status === 'ready' ? (
              <div className="serviceVersionsViewTabs" aria-label="发布说明视图">
                {(['smart', 'translated', 'original'] as const).map((view) => (
                  <button
                    key={view}
                    type="button"
                    className={cn(
                      'serviceVersionsViewTab',
                      viewMode === view && 'serviceVersionsViewTabActive',
                    )}
                    aria-pressed={viewMode === view}
                    onClick={() => setViewMode(view)}
                  >
                    {viewLabel(view)}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
        </div>

        {activeTaskNotice ? (
          <div className="serviceVersionsActivityBanner" data-service-versions-banner="activity">
            <div>
              <div className="serviceVersionsActivityTitle">{activeTaskNotice.title}</div>
              <div className="muted">{activeTaskNotice.detail}</div>
            </div>
            {activeTaskNotice.jobId ? (
              <Button
                variant="ghost"
                onClick={() => navigate({ name: 'job', jobId: activeTaskNotice.jobId! })}
              >
                查看任务
              </Button>
            ) : null}
          </div>
        ) : null}

        {fallbackBanner ? (
          <div className="releaseDrawerBanner releaseDrawerBanner-warning" data-service-versions-banner="fallback">
            <span>{fallbackBanner.message}</span>
            {showSettingsAction ? (
              <Button variant="ghost" onClick={openSettings}>
                打开设置
              </Button>
            ) : null}
          </div>
        ) : null}

        {listBanner ? (
          <div
            className={cn('releaseDrawerBanner', `releaseDrawerBanner-${listBanner.tone}`)}
            data-service-versions-banner={listBanner.tone}
          >
            <span>{listBanner.message}</span>
            {showSettingsAction ? (
              <Button variant="ghost" onClick={openSettings}>
                打开设置
              </Button>
            ) : null}
          </div>
        ) : null}

        {anchorBanner ? (
          <div className="releaseDrawerBanner releaseDrawerBanner-warning" data-service-versions-banner="anchor">
            <span>{anchorBanner.message}</span>
          </div>
        ) : null}

        {initialLoadState === 'loading' && items.length === 0 ? (
          <div className="serviceVersionsState" data-service-versions-state="loading">
            <span className="btnInlineSpinner" aria-hidden="true" />
            <span>正在加载版本发布记录…</span>
          </div>
        ) : null}

        {initialLoadState === 'ready' && listResponse?.status !== 'ready' ? (
          <div className="serviceVersionsState serviceVersionsStateError" data-service-versions-state={listResponse?.status}>
            <div className="serviceVersionsStateTitle">无法读取版本发布记录</div>
            <div className="serviceVersionsStateMessage">{listResponse?.message ?? '请稍后重试。'}</div>
          </div>
        ) : null}

        {initialLoadState === 'ready' && listResponse?.status === 'ready' && items.length === 0 ? (
          <div className="serviceVersionsState" data-service-versions-state="empty">
            <div className="serviceVersionsStateTitle">暂无发布记录</div>
            <div className="serviceVersionsStateMessage">该仓库当前没有可展示的 Releases。</div>
          </div>
        ) : null}

        {listResponse?.status === 'ready' && items.length > 0 ? (
          <div
            className="serviceVersionsBodyLayout"
            data-service-versions="true"
            data-service-versions-layout={showDesktopIndex ? 'desktop' : 'mobile'}
            data-service-versions-total-count={items.length}
            data-service-versions-list-visible-count={renderedCardCount}
            data-service-versions-index-visible-count={renderedIndexCount}
            data-service-versions-view={viewMode}
          >
            {showDesktopIndex ? (
              <aside
                className="serviceVersionsIndexRail"
                data-service-versions-index="true"
                aria-label="版本目录"
              >
                <div className="serviceVersionsIndexViewport" ref={indexScrollRef}>
                  <div className="serviceVersionsIndexList" style={{ height: `${indexVirtualizer.getTotalSize()}px` }}>
                    <div className="serviceVersionsIndexListInner" style={{ transform: `translateY(${indexOffset}px)` }}>
                      {indexVirtualItems.map((virtualRow) => {
                        if (virtualRow.index >= items.length) {
                          return (
                            <div
                              key={virtualRow.key}
                              className="serviceVersionsIndexRow serviceVersionsIndexRowLoader"
                              data-index={virtualRow.index}
                            >
                              <div className="serviceVersionsIndexLoader">
                                {loadMoreFailure
                                  ? '目录加载失败，继续向下滚动右侧列表可重试。'
                                  : loadingMore
                                    ? '继续加载中…'
                                    : hasMoreRef.current
                                      ? '继续加载更旧版本…'
                                      : ''}
                              </div>
                            </div>
                          )
                        }

                        const item = items[virtualRow.index]!
                        const currentMatch = releaseMatchesVersion(item, currentVersion)
                        const candidateMatch = releaseMatchesVersion(item, candidateVersion)
                        const selected = virtualRow.index === selectedIndex
                        return (
                          <div
                            key={virtualRow.key}
                            className="serviceVersionsIndexRow"
                            data-index={virtualRow.index}
                          >
                            <button
                              type="button"
                              className={cn(
                                'serviceVersionsIndexItem',
                                selected && 'serviceVersionsIndexItemActive',
                                currentMatch && 'serviceVersionsIndexItemCurrent',
                              )}
                              aria-pressed={selected}
                              data-service-versions-index-item="true"
                              data-release-tag={item.tagName}
                              data-service-versions-index-selected={selected ? 'true' : 'false'}
                              onClick={() => {
                                setSelectedIndex(virtualRow.index)
                                void centerVersionCard(virtualRow.index, item.tagName, 'interactive')
                              }}
                            >
                              <span className="serviceVersionsIndexVersion">
                                <Mono>{item.tagName}</Mono>
                              </span>
                              <span className="serviceVersionsIndexMeta">
                                <span>{formatVersionDirectoryTimeLabel(preferredReleaseTimestamp(item))}</span>
                                {currentMatch ? (
                                  <span className="serviceVersionsIndexFlag">当前</span>
                                ) : candidateMatch ? (
                                  <span className="serviceVersionsIndexFlag">候选</span>
                                ) : null}
                              </span>
                            </button>
                          </div>
                        )
                      })}
                    </div>
                  </div>
                </div>
              </aside>
            ) : null}

            <div className="serviceVersionsScrollShell">
              <div className="serviceVersionsScrollViewport" ref={listScrollRef}>
                <div className="serviceVersionsList" style={{ height: `${listVirtualizer.getTotalSize()}px` }}>
                  <div className="serviceVersionsListInner" style={{ transform: `translateY(${listOffset}px)` }}>
                    {listVirtualItems.map((virtualRow) => {
                      if (virtualRow.index >= cards.length) {
                        return (
                          <div
                            key={virtualRow.key}
                            data-index={virtualRow.index}
                            className="serviceVersionsVirtualRow serviceVersionsVirtualRowLoader"
                            ref={listVirtualizer.measureElement}
                          >
                            {loadMoreFailure ? (
                              <div className="serviceVersionsLoaderCard">
                                <div className="serviceVersionsLoaderTitle">加载更旧版本失败</div>
                                <div className="serviceVersionsLoaderMessage">{loadMoreFailure.message ?? '请稍后重试。'}</div>
                                <div>
                                  <Button variant="ghost" onClick={() => void loadNextPage()}>
                                    重试
                                  </Button>
                                </div>
                              </div>
                            ) : hasMoreRef.current || loadingMore ? (
                              <div className="serviceVersionsLoaderRow">
                                <span className="btnInlineSpinner" aria-hidden="true" />
                                <span>{loadingMore ? '正在继续加载更旧版本…' : '继续加载更旧版本…'}</span>
                              </div>
                            ) : null}
                          </div>
                        )
                      }

                      const card = cards[virtualRow.index]!
                      const expanded = expandedIds.has(card.item.id)

                      return (
                        <div
                          key={virtualRow.key}
                          data-index={virtualRow.index}
                          ref={listVirtualizer.measureElement}
                          className="serviceVersionsVirtualRow"
                        >
                          <ServiceVersionCard
                            card={card}
                            candidateDisplayVersion={candidateDisplayVersion}
                            rollbackTarget={props.rollbackTarget}
                            rollbackBackupSummary={rollbackBackupSummary}
                            viewLabel={viewLabel(viewMode)}
                            sourceLabel={sourceLabel(listResponse)}
                            expanded={expanded}
                            onToggleExpanded={toggleExpanded}
                            onApplyUpdate={() => {
                              if (isDockrevService(props.service)) {
                                props.dockrevSelfUpgradeAction?.open()
                                return
                              }
                              props.onApplyUpdate()
                            }}
                            onRollback={props.onRollback}
                            onOpenRollbackExplanation={(item) => {
                              void openRollbackExplanation(item)
                            }}
                          />
                        </div>
                      )
                    })}
                  </div>
                </div>
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </section>
  )
}
