import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import {
  ApiError,
  getServiceReleaseNotes,
  type JobListItem,
  type ReleaseNotesView,
  type Service,
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
import {
  isDockrevService,
  rollbackVersionLabel,
  shortDigest,
} from '../pages/serviceDetailUtils'
import { Button, ExternalLinkIcon, Mono, Pill } from '../ui'

const RELEASES_PER_PAGE = 20
const RELEASE_ROW_GAP = 14
const BODY_COLLAPSE_LINE_COUNT = 10

type AnchorState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'found'; absoluteIndex: number }
  | { status: 'notFound'; message: string }
  | { status: 'unavailable'; message: string }

type ServiceVersionsSectionProps = {
  service: Service
  jobs: JobListItem[]
  rollbackTarget: ServiceRollbackTargetResponse | null
  rollbackTargetRefreshing: boolean
  busy: boolean
  updateActiveJob: { jobId: string; status: string } | null
  updateSubmitting: boolean
  rollbackActiveJobId: string | null
  rollbackActiveJobStatus: string | null
  onApplyUpdate: () => void
  onRollback: () => void
}

function formatReleaseDate(value: string | null | undefined): {
  dateLine: string
  timeLine: string | null
} {
  const trimmed = (value ?? '').trim()
  if (!trimmed) return { dateLine: '时间未知', timeLine: null }
  const parsed = new Date(trimmed)
  if (Number.isNaN(parsed.valueOf())) return { dateLine: trimmed, timeLine: null }
  return {
    dateLine: new Intl.DateTimeFormat(undefined, {
      dateStyle: 'medium',
    }).format(parsed),
    timeLine: new Intl.DateTimeFormat(undefined, {
      timeStyle: 'short',
    }).format(parsed),
  }
}

function preferredReleaseTimestamp(item: ServiceReleaseNoteItem): string | null {
  return item.publishedAt?.trim() || item.createdAt?.trim() || null
}

function normalizeVersion(value: string | null | undefined): string {
  return (value ?? '').trim().toLowerCase()
}

function safeHttpUrl(value: string | null | undefined): string {
  const trimmed = (value ?? '').trim()
  if (!trimmed) return ''
  try {
    const url = new URL(trimmed)
    return url.protocol === 'http:' || url.protocol === 'https:' ? url.toString() : ''
  } catch {
    return ''
  }
}

function releaseMatchesVersion(item: ServiceReleaseNoteItem, version: string | null | undefined): boolean {
  const normalizedVersion = normalizeVersion(version)
  if (!normalizedVersion) return false
  const normalizedTag = normalizeVersion(item.tagName)
  if (normalizedTag === normalizedVersion) return true
  if (normalizedTag === `v${normalizedVersion}`) return true
  if (normalizedVersion.startsWith('v') && normalizedTag === normalizedVersion.slice(1)) return true
  return false
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

function collapseBody(body: string, expanded: boolean): {
  visibleBody: string
  totalLines: number
  isCollapsible: boolean
} {
  const trimmed = body.trim()
  if (!trimmed) {
    return { visibleBody: '', totalLines: 0, isCollapsible: false }
  }
  const lines = trimmed.split(/\r?\n/)
  if (expanded || lines.length <= BODY_COLLAPSE_LINE_COUNT) {
    return { visibleBody: trimmed, totalLines: lines.length, isCollapsible: lines.length > BODY_COLLAPSE_LINE_COUNT }
  }
  return {
    visibleBody: lines.slice(0, BODY_COLLAPSE_LINE_COUNT).join('\n'),
    totalLines: lines.length,
    isCollapsible: true,
  }
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

export function ServiceVersionsSection(props: ServiceVersionsSectionProps) {
  const confirm = useConfirm()
  const serviceId = props.service.id.trim()
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const sessionKey = useMemo(() => {
    const anchorVersion = (props.service.image.resolvedTag ?? '').trim() || props.service.image.tag.trim()
    return `${serviceId}::${anchorVersion}`
  }, [props.service.image.resolvedTag, props.service.image.tag, serviceId])
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
    if (scrollRef.current) scrollRef.current.scrollTop = 0
    setInitialLoadState('idle')
    setListResponse(null)
    setItems([])
    setExpandedIds(new Set())
    setLoadingMore(false)
    setLoadMoreFailure(null)
    setViewMode('smart')
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
        aggregated = [...aggregated, ...nextResponse.items]
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
  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 360,
    overscan: 6,
    gap: RELEASE_ROW_GAP,
    getItemKey: (index) => (index < items.length ? items[index]?.id ?? index : `loader:${index}`),
    measureElement: (element) => element.getBoundingClientRect().height,
  })

  useEffect(() => {
    virtualizer.measure()
  }, [expandedIds, items.length, viewMode, virtualizer])

  useEffect(() => {
    const element = scrollRef.current
    if (!element || typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(() => {
      virtualizer.measure()
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [virtualizer])

  useEffect(() => {
    if (initialLoadState !== 'ready') return
    if (anchorState.status !== 'found') return
    if (items.length <= anchorState.absoluteIndex) return
    const scrollElement = scrollRef.current
    if (!scrollElement) return
    const key = `${sessionKey}:${anchorState.absoluteIndex}`
    if (initialCenterKeyRef.current === key && scrollElement.scrollTop > 0) return

    let frameA = 0
    let frameB = 0
    let retryTimer = 0
    const currentCardInView = (element: HTMLDivElement): boolean => {
      const currentCard = element.querySelector<HTMLElement>('[data-version-card-current="true"]')
      if (!currentCard) return false
      const viewportRect = element.getBoundingClientRect()
      const cardRect = currentCard.getBoundingClientRect()
      const viewportCenter = viewportRect.top + viewportRect.height / 2
      const cardCenter = cardRect.top + cardRect.height / 2
      return Math.abs(cardCenter - viewportCenter) <= Math.max(48, viewportRect.height * 0.18)
    }

    const centerCurrentCard = (attemptsRemaining: number) => {
      const element = scrollRef.current
      if (!element) return
      virtualizer.measure()
      const currentCard = element.querySelector<HTMLElement>('[data-version-card-current="true"]')
      let targetOffset = resolveVirtualOffset(
        virtualizer.getOffsetForIndex(anchorState.absoluteIndex, 'center'),
      )
      if (currentCard) {
        const viewportRect = element.getBoundingClientRect()
        const cardRect = currentCard.getBoundingClientRect()
        targetOffset =
          element.scrollTop +
          (cardRect.top - viewportRect.top) -
          Math.max(0, (viewportRect.height - cardRect.height) / 2)
      }
      targetOffset = Math.max(0, targetOffset)
      element.scrollTo({ top: targetOffset })
      if (currentCardInView(element) || attemptsRemaining <= 0) {
        initialCenterKeyRef.current = key
        return
      }
      retryTimer = window.setTimeout(() => {
        centerCurrentCard(attemptsRemaining - 1)
      }, 80)
    }

    frameA = window.requestAnimationFrame(() => {
      frameB = window.requestAnimationFrame(() => {
        centerCurrentCard(6)
      })
    })
    return () => {
      window.cancelAnimationFrame(frameA)
      window.cancelAnimationFrame(frameB)
      window.clearTimeout(retryTimer)
    }
  }, [anchorState, initialLoadState, items.length, sessionKey, virtualizer])

  const virtualItems = virtualizer.getVirtualItems()
  const listOffset = virtualItems[0]?.start ?? 0

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
      let nextItems: ServiceReleaseNoteItem[] = []
      setItems((prev) => {
        nextItems = [...prev, ...response.items]
        return nextItems
      })
      if (currentVersion) {
        const foundIndex = nextItems.findIndex((item) => releaseMatchesVersion(item, currentVersion))
        if (foundIndex >= 0) {
          setAnchorState({ status: 'found', absoluteIndex: foundIndex })
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
  }, [currentVersion, loadingMore, requestPage, serviceId, sessionKey])

  useEffect(() => {
    const lastItem = [...virtualItems].reverse()[0]
    if (!lastItem) return
    if (lastItem.index < items.length - 1) return
    if (!hasMoreRef.current || loadingMore || loadMoreFailure || initialLoadState !== 'ready') return
    void loadNextPage()
  }, [initialLoadState, items.length, loadMoreFailure, loadNextPage, loadingMore, virtualItems])

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
      const showUpdate = (!dockrevService && newerThanCurrent) || candidateMatch
      const showRollback = !dockrevService && (deployedHistorical || rollbackTargetMatch)

      let updateDisabledReason: string | null = null
      if (showUpdate) {
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
        updateDisabledReason,
        rollbackDisabledReason,
      }
    })
  }, [
    candidateVersion,
    currentVersion,
    deployedHistoricalVersions,
    items,
    props.rollbackTarget?.targetDisplayTag,
    props.service,
    serviceActionLockReason,
    viewMode,
  ])

  const renderedCardCount = virtualItems.filter((item) => item.index < cards.length).length
  const openSettings = () => navigate({ name: 'settings' })

  return (
    <section className="serviceVersionsSection" data-service-detail-section-card="versions">
      <div className="serviceVersionsCard">
        <div className="serviceVersionsHeader">
          <div className="serviceVersionsHeaderText">
            <div className="title">版本</div>
            <div className="muted">
              以当前部署版本为锚点浏览统一 release notes；较新版本提供更新入口，已部署历史版本保留回滚语义说明。
            </div>
          </div>
          <div className="serviceVersionsHeaderControls">
            {listResponse?.repo?.htmlUrl ? (
              <a
                className="serviceVersionsRepoLink"
                href={listResponse.repo.htmlUrl}
                rel="noreferrer"
                target="_blank"
                title="打开仓库"
              >
                <ExternalLinkIcon className="iconSm" />
                <span>
                  <Mono>{listResponse.repo.fullName}</Mono>
                </span>
              </a>
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

        {listResponse?.repo ? (
          <div className="serviceVersionsHeaderMeta">
            <span className="serviceVersionsChip">
              <Mono>{listResponse.repo.fullName}</Mono>
            </span>
            <span className="serviceVersionsChip">{sourceLabel(listResponse)}</span>
            <span className="serviceVersionsChip">{`当前 ${currentDisplayVersion}`}</span>
            {candidateDisplayVersion ? (
              <span className="serviceVersionsChip">{`候选 ${candidateDisplayVersion}`}</span>
            ) : null}
          </div>
        ) : null}

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
            className="serviceVersionsScrollShell"
            data-service-versions="true"
            data-service-versions-total-count={items.length}
            data-service-versions-visible-count={renderedCardCount}
            data-service-versions-view={viewMode}
          >
            <div className="serviceVersionsScrollViewport" ref={scrollRef}>
              <div className="serviceVersionsList" style={{ height: `${virtualizer.getTotalSize()}px` }}>
                <div className="serviceVersionsListInner" style={{ transform: `translateY(${listOffset}px)` }}>
                  {virtualItems.map((virtualRow) => {
                    if (virtualRow.index >= cards.length) {
                      return (
                        <div
                          key={virtualRow.key}
                          data-index={virtualRow.index}
                          className="serviceVersionsVirtualRow serviceVersionsVirtualRowLoader"
                          ref={virtualizer.measureElement}
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
                    const linkUrl = safeHttpUrl(card.item.htmlUrl)
                    const expanded = expandedIds.has(card.item.id)
                    const bodyState = collapseBody(card.body, expanded)
                    const releaseDate = formatReleaseDate(preferredReleaseTimestamp(card.item))
                    const canExecuteRollback =
                      card.rollbackTargetMatch &&
                      props.rollbackTarget?.available &&
                      Boolean(props.rollbackTarget?.targetDigest)
                    const showCandidateStatus = Boolean(card.candidateMatch && candidateDisplayVersion)
                    const showRollbackDigestStatus = Boolean(
                      card.rollbackTargetMatch && props.rollbackTarget?.targetDigest,
                    )
                    const rollbackTargetDigest = props.rollbackTarget?.targetDigest ?? null
                    const showHistoricalStatus = Boolean(
                      card.deployedHistorical && !card.rollbackTargetMatch,
                    )
                    const showCardAside =
                      card.showUpdate ||
                      card.showRollback ||
                      showCandidateStatus ||
                      showRollbackDigestStatus ||
                      showHistoricalStatus
                    const titleText = (card.item.name ?? '').trim() || card.item.tagName
                    const titleUsesTag = titleText === card.item.tagName

                    return (
                      <div
                        key={virtualRow.key}
                        data-index={virtualRow.index}
                        ref={virtualizer.measureElement}
                        className="serviceVersionsVirtualRow"
                      >
                      <article
                        className={cn(
                          'serviceVersionCard',
                          card.olderThanCurrent && 'serviceVersionCardOlder',
                          card.currentMatch && 'serviceVersionCardCurrent',
                        )}
                        data-service-version-card="true"
                        data-release-tag={card.item.tagName}
                        data-version-card-current={card.currentMatch ? 'true' : 'false'}
                        data-version-card-older={card.olderThanCurrent ? 'true' : 'false'}
                        data-version-card-has-actions={
                          card.showUpdate || card.showRollback ? 'true' : 'false'
                        }
                        data-version-card-has-aside={showCardAside ? 'true' : 'false'}
                      >
                        <div className="serviceVersionCardMeta">
                          <div className="serviceVersionHeading">
                              <div className="serviceVersionTagRow">
                                <div className="serviceVersionTagText">
                                  <Mono>{card.item.tagName}</Mono>
                                </div>
                                <div className="serviceVersionBadges">
                                  {card.currentMatch ? <Pill tone="ok">当前部署</Pill> : null}
                                  {card.candidateMatch ? <Pill tone="info">候选</Pill> : null}
                                  {card.deployedHistorical ? <Pill tone="muted">已部署历史</Pill> : null}
                                  {card.rollbackTargetMatch ? <Pill tone="warn">可执行回滚</Pill> : null}
                                  {card.item.prerelease ? <Pill tone="muted">预发布</Pill> : null}
                                </div>
                              </div>
                            </div>

                            <dl className="serviceVersionFacts">
                              <div>
                                <dt>发布时间</dt>
                                <dd className="serviceVersionDateValue">
                                  <span>{releaseDate.dateLine}</span>
                                  {releaseDate.timeLine ? <span>{releaseDate.timeLine}</span> : null}
                                </dd>
                              </div>
                              <div>
                                <dt>来源</dt>
                                <dd>{sourceLabel(listResponse)}</dd>
                              </div>
                              <div>
                                <dt>视图</dt>
                                <dd>{viewLabel(viewMode)}</dd>
                              </div>
                              <div>
                                <dt>状态</dt>
                                <dd>{card.olderThanCurrent ? '相对当前更旧' : card.currentMatch ? '当前部署中' : '发布记录'}</dd>
                              </div>
                            </dl>

                            {linkUrl ? (
                              <a
                                className="serviceVersionLinkRow"
                                href={linkUrl}
                                rel="noreferrer"
                                target="_blank"
                              >
                                Release
                              </a>
                            ) : null}
                          </div>

                          <div className="serviceVersionCardBody">
                            <div className="serviceVersionBodyShell">
                              {!titleUsesTag ? (
                                <div className="serviceVersionBodyTitle">{titleText}</div>
                              ) : null}
                              {bodyState.visibleBody ? (
                                <div
                                  className="serviceVersionBody"
                                  data-service-version-body-expanded={expanded ? 'true' : 'false'}
                                >
                                  {bodyState.visibleBody}
                                </div>
                              ) : (
                                <div className="serviceVersionBodyEmpty">该版本没有可展示的正文。</div>
                              )}
                            </div>
                            {card.bodyMissing || bodyState.isCollapsible ? (
                              <div className="serviceVersionBodyFoot">
                                {card.bodyMissing ? (
                                  <span className="serviceVersionBodyHint">当前视图缺少专用内容，已回退原文。</span>
                                ) : null}
                                {bodyState.isCollapsible ? (
                                  <button
                                    type="button"
                                    className="serviceVersionExpandButton"
                                    onClick={() => toggleExpanded(card.item.id)}
                                  >
                                    {expanded ? '收起' : '展开'}
                                  </button>
                                ) : null}
                              </div>
                            ) : null}
                          </div>

                          {showCardAside ? (
                          <div className="serviceVersionCardAside">
                            <div className="serviceVersionStatusStack">
                              {showCandidateStatus ? (
                                <div className="serviceVersionStatusBlock">
                                  <div className="serviceVersionStatusLabel">当前候选</div>
                                  <div className="serviceVersionStatusValue">
                                    <Mono>{candidateDisplayVersion}</Mono>
                                  </div>
                                </div>
                              ) : null}
                              {showRollbackDigestStatus ? (
                                <div className="serviceVersionStatusBlock">
                                  <div className="serviceVersionStatusLabel">回滚目标摘要</div>
                                  <div className="serviceVersionStatusValue">
                                    <Mono>{shortDigest(rollbackTargetDigest!)}</Mono>
                                  </div>
                                </div>
                              ) : null}
                              {showHistoricalStatus ? (
                                <div className="serviceVersionStatusBlock">
                                  <div className="serviceVersionStatusLabel">历史语义</div>
                                  <div className="serviceVersionStatusValue">已部署过，但不一定是当前可执行 rollback target。</div>
                                </div>
                              ) : null}
                            </div>

                            {card.showUpdate || card.showRollback ? (
                              <div className="serviceVersionActionStack">
                              {card.showUpdate ? (
                                <div
                                  className="serviceVersionActionBlock"
                                  data-service-version-action="update"
                                  data-release-tag={card.item.tagName}
                                >
                                  <Button
                                    variant="primary"
                                    disabled={Boolean(card.updateDisabledReason)}
                                    hint={card.updateDisabledReason ?? undefined}
                                    onClick={props.onApplyUpdate}
                                  >
                                    更新
                                  </Button>
                                  {card.updateDisabledReason ? (
                                    <div className="serviceVersionActionHint">{card.updateDisabledReason}</div>
                                  ) : (
                                    <div className="serviceVersionActionHint">发起当前 candidate 对应的服务更新任务。</div>
                                  )}
                                </div>
                              ) : null}

                              {card.showRollback ? (
                                <div
                                  className="serviceVersionActionBlock"
                                  data-service-version-action="rollback"
                                  data-release-tag={card.item.tagName}
                                >
                                  <Button
                                    variant={canExecuteRollback ? 'danger' : 'ghost'}
                                    disabled={Boolean(card.rollbackDisabledReason)}
                                    hint={card.rollbackDisabledReason ?? undefined}
                                    onClick={() => {
                                      if (canExecuteRollback) {
                                        props.onRollback()
                                        return
                                      }
                                      void openRollbackExplanation(card.item)
                                    }}
                                  >
                                    回滚
                                  </Button>
                                  <div className="serviceVersionActionHint">
                                    {card.rollbackDisabledReason
                                      ? card.rollbackDisabledReason
                                      : canExecuteRollback
                                        ? '这个版本正对应后端当前可执行的 rollback target。'
                                        : '会进入解释性提示，不会直接创建回滚任务。'}
                                  </div>
                                </div>
                              ) : null}
                              </div>
                            ) : null}
                          </div>
                          ) : null}
                        </article>
                      </div>
                    )
                  })}
                </div>
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </section>
  )
}
