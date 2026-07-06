import { useCallback, useEffect, useId, useMemo, useRef, useState, type FocusEvent } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'

import {
  ApiError,
  getServiceReleaseNotes,
  type ReleaseNotesView,
  type ServiceReleaseNoteItem,
  type ServiceReleaseNotesResponse,
  type ServiceReleaseNotesStatus,
} from '../api'
import { navigate } from '../routes'
import { closeGitHubReleaseDrawer } from '../releaseDrawer'
import {
  Button,
  Drawer,
  DrawerClose,
  DrawerContent,
  DrawerDescription,
  DrawerHeader,
  DrawerTitle,
  ExternalLinkIcon,
  Mono,
  ScrollArea,
} from '../ui'
import { cn } from '../lib/utils'

const RELEASES_PER_PAGE = 20
const RELEASE_DRAWER_LOCATE_LIMIT = 50
const TARGET_HIGHLIGHT_MS = 2200
const RELEASE_ROW_GAP = 12

type ReleaseLocateStatus = 'found' | 'notFound' | 'outsideWindow' | 'unsupportedRepo' | 'upstreamError'

type ReleaseLocateResponse = {
  status: ReleaseLocateStatus
  version: string
  searchedCount: number
  matchedTag?: string | null
  page?: number | null
  indexWithinPage?: number | null
  absoluteIndex?: number | null
  message?: string | null
}

function InfoIcon(props: { className?: string }) {
  return (
    <svg className={props.className} viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <circle cx="8" cy="8" r="6" />
      <path d="M8 7.2v3.5" />
      <path d="M8 5.1h.01" />
    </svg>
  )
}

function CloseIcon(props: { className?: string }) {
  return (
    <svg className={props.className} viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <path d="M4 4l8 8" />
      <path d="M12 4 4 12" />
    </svg>
  )
}

function formatReleaseDate(value: string | null | undefined): string {
  const trimmed = (value ?? '').trim()
  if (!trimmed) return '时间未知'
  const parsed = new Date(trimmed)
  if (Number.isNaN(parsed.valueOf())) return trimmed
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(parsed)
}

function preferredReleaseTimestamp(item: ServiceReleaseNoteItem): string | null {
  return item.publishedAt?.trim() || item.createdAt?.trim() || null
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

function hasLongBody(body: string | null | undefined): boolean {
  const normalized = (body ?? '').trim()
  if (!normalized) return false
  return normalized.length > 240 || normalized.split(/\r?\n/).length > 4
}

function normalizeVersion(value: string | null | undefined): string {
  return (value ?? '').trim().toLowerCase()
}

function statusTone(
  status: ServiceReleaseNotesStatus | ReleaseLocateStatus | 'info',
): 'info' | 'warning' | 'danger' | 'success' {
  if (status === 'found') return 'success'
  if (status === 'notFound' || status === 'outsideWindow' || status === 'info') return 'warning'
  if (status === 'upstreamError') return 'danger'
  return 'info'
}

function sourceLabel(response: ServiceReleaseNotesResponse | null | undefined): string {
  if (!response) return '未知'
  return response.source === 'octoRill' ? 'OctoRill' : 'GitHub Releases'
}

function viewLabel(view: ReleaseNotesView): string {
  if (view === 'original') return '原文'
  if (view === 'translated') return '翻译'
  return '润色'
}

function shouldOfferSettingsAction(response: ServiceReleaseNotesResponse | null | undefined): boolean {
  const fallbackReason = response?.fallback?.reason
  if (fallbackReason === 'notConfigured' || fallbackReason === 'unauthorized') return true
  const message = response?.message ?? response?.fallback?.message ?? ''
  return message.includes('GitHub PAT') || message.includes('token 权限') || message.includes('OctoRill')
}

function fallbackReleaseErrorMessage(error: unknown): string {
  if (error instanceof ApiError && error.status === 404) {
    return '该服务不存在或已被删除，无法读取 GitHub 发布记录。'
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

type GitHubReleaseDrawerProps = {
  open: boolean
  serviceId: string | null
  version?: string | null
  onOpenChange: (open: boolean) => void
}

export function GitHubReleaseDrawer(props: GitHubReleaseDrawerProps) {
  const serviceId = props.serviceId?.trim() || null
  const drawerInfoId = useId()
  const targetVersion = props.version?.trim() || null
  const sessionKey = props.open && serviceId ? `${serviceId}::${targetVersion ?? ''}` : null
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const activeSessionRef = useRef<string | null>(sessionKey)
  const loadedPagesRef = useRef(0)
  const nextCursorRef = useRef<string | null>(null)
  const inFlightPagesRef = useRef<Map<string, Promise<ServiceReleaseNotesResponse | null>>>(new Map())
  const hasMoreRef = useRef(false)
  const loadingMoreRef = useRef(false)
  const targetScrollKeyRef = useRef<string | null>(null)
  const highlightTimerRef = useRef<number | null>(null)
  const infoCloseTimerRef = useRef<number | null>(null)

  const [initialLoadState, setInitialLoadState] = useState<'idle' | 'loading' | 'ready'>('idle')
  const [listResponse, setListResponse] = useState<ServiceReleaseNotesResponse | null>(null)
  const [locateState, setLocateState] = useState<'idle' | 'loading' | 'ready'>('idle')
  const [locateResponse, setLocateResponse] = useState<ReleaseLocateResponse | null>(null)
  const [items, setItems] = useState<ServiceReleaseNoteItem[]>([])
  const [expandedIds, setExpandedIds] = useState<Set<string>>(() => new Set())
  const [loadingMore, setLoadingMore] = useState(false)
  const [loadMoreFailure, setLoadMoreFailure] = useState<ServiceReleaseNotesResponse | null>(null)
  const [highlightedId, setHighlightedId] = useState<string | null>(null)
  const [infoPanelOpen, setInfoPanelOpen] = useState(false)
  const [viewMode, setViewMode] = useState<ReleaseNotesView>('smart')

  useEffect(() => {
    activeSessionRef.current = sessionKey
  }, [sessionKey])

  useEffect(() => {
    return () => {
      if (highlightTimerRef.current != null) {
        window.clearTimeout(highlightTimerRef.current)
      }
      if (infoCloseTimerRef.current != null) {
        window.clearTimeout(infoCloseTimerRef.current)
      }
    }
  }, [])

  const resetState = useCallback(() => {
    loadedPagesRef.current = 0
    nextCursorRef.current = null
    inFlightPagesRef.current.clear()
    hasMoreRef.current = false
    loadingMoreRef.current = false
    targetScrollKeyRef.current = null
    if (highlightTimerRef.current != null) {
      window.clearTimeout(highlightTimerRef.current)
      highlightTimerRef.current = null
    }
    if (infoCloseTimerRef.current != null) {
      window.clearTimeout(infoCloseTimerRef.current)
      infoCloseTimerRef.current = null
    }
    setInitialLoadState('idle')
    setListResponse(null)
    setLocateState('idle')
    setLocateResponse(null)
    setItems([])
    setExpandedIds(new Set())
    setLoadingMore(false)
    setLoadMoreFailure(null)
    setHighlightedId(null)
    setInfoPanelOpen(false)
    setViewMode('smart')
  }, [])

  const fetchPage = useCallback(
    async (expectedSession: string, targetServiceId: string, cursor: string | null) => {
      const isFirstPage = cursor == null
      const requestKey = `${expectedSession}:${cursor ?? 'first'}`
      const existing = inFlightPagesRef.current.get(requestKey)
      if (existing) {
        return await existing
      }

      const request = (async () => {
        let response: ServiceReleaseNotesResponse
        try {
          response = await getServiceReleaseNotes(targetServiceId, {
            cursor,
            limit: RELEASES_PER_PAGE,
          })
        } catch (error) {
          response = buildListFailureResponse(error, cursor, RELEASES_PER_PAGE)
        }
        if (activeSessionRef.current !== expectedSession) return null

        if (isFirstPage) {
          setListResponse(response)
          setInitialLoadState('ready')
          setViewMode(response.source === 'gitHub' ? 'original' : response.defaultView)
        }

        if (response.status !== 'ready') {
          if (isFirstPage) {
            setItems([])
            loadedPagesRef.current = 0
            nextCursorRef.current = null
            hasMoreRef.current = false
          } else {
            setLoadMoreFailure(response)
          }
          return response
        }

        loadedPagesRef.current = isFirstPage ? 1 : loadedPagesRef.current + 1
        nextCursorRef.current = response.nextCursor ?? null
        hasMoreRef.current = response.hasMore && nextCursorRef.current != null
        setLoadMoreFailure(null)
        setItems((prev) => {
          if (isFirstPage) return response.items
          return [...prev, ...response.items]
        })
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
    [],
  )

  const locateAcrossPages = useCallback(
    async (
      expectedSession: string,
      targetServiceId: string,
      version: string,
      initialResponse: ServiceReleaseNotesResponse | null,
    ): Promise<ReleaseLocateResponse | null> => {
      let response = initialResponse
      let searchedCount = 0
      if (response && response.status !== 'ready') {
        return {
          status: response.status === 'unsupportedRepo' ? 'unsupportedRepo' : 'upstreamError',
          version,
          searchedCount,
          message: response.message ?? response.fallback?.message ?? '无法定位发布记录。',
        }
      }
      while (response && response.status === 'ready') {
        const remainingBudget = RELEASE_DRAWER_LOCATE_LIMIT - searchedCount
        if (remainingBudget <= 0) {
          return {
            status: response.hasMore ? 'outsideWindow' : 'notFound',
            version,
            searchedCount,
            message: response.hasMore
              ? `已扫描前 ${searchedCount} 条发布记录，${version} 不在当前定位窗口内。`
              : `在前 ${searchedCount} 条发布记录中未找到 ${version}。`,
          }
        }
        const scanCount = Math.min(response.items.length, remainingBudget)
        const scanItems = response.items.slice(0, scanCount)
        const matchedIndex = scanItems.findIndex((item) => releaseMatchesVersion(item, version))
        if (matchedIndex >= 0) {
          return {
            status: 'found',
            version,
            searchedCount: searchedCount + scanCount,
            matchedTag: scanItems[matchedIndex]?.tagName ?? version,
            page: Math.max(1, loadedPagesRef.current),
            indexWithinPage: matchedIndex,
            absoluteIndex: searchedCount + matchedIndex,
            message: null,
          }
        }
        searchedCount += scanCount
        if (!response.hasMore || searchedCount >= RELEASE_DRAWER_LOCATE_LIMIT) {
          return {
            status: response.hasMore ? 'outsideWindow' : 'notFound',
            version,
            searchedCount,
            message: response.hasMore
              ? `已扫描前 ${searchedCount} 条发布记录，${version} 不在当前定位窗口内。`
              : `在前 ${searchedCount} 条发布记录中未找到 ${version}。`,
          }
        }
        const nextCursor = response.nextCursor ?? nextCursorRef.current
        if (!nextCursor) {
          return {
            status: 'notFound',
            version,
            searchedCount,
            message: `在前 ${searchedCount} 条发布记录中未找到 ${version}。`,
          }
        }
        const nextResponse = await fetchPage(expectedSession, targetServiceId, nextCursor)
        if (!nextResponse) return null
        if (nextResponse.status !== 'ready') {
          return {
            status: nextResponse.status === 'unsupportedRepo' ? 'unsupportedRepo' : 'upstreamError',
            version,
            searchedCount,
            message: nextResponse.message ?? nextResponse.fallback?.message ?? '无法继续定位发布记录。',
          }
        }
        response = nextResponse
      }
      return {
        status: 'notFound',
        version,
        searchedCount,
        message: `在前 ${searchedCount} 条发布记录中未找到 ${version}。`,
      }
    },
    [fetchPage],
  )

  const loadNextPage = useCallback(async () => {
    if (!sessionKey || !serviceId) return
    if (loadingMoreRef.current || loadingMore || !hasMoreRef.current) return
    const nextCursor = nextCursorRef.current
    if (!nextCursor) return
    loadingMoreRef.current = true
    setLoadingMore(true)
    try {
      await fetchPage(sessionKey, serviceId, nextCursor)
    } finally {
      if (activeSessionRef.current === sessionKey) {
        loadingMoreRef.current = false
        setLoadingMore(false)
      }
    }
  }, [fetchPage, loadingMore, serviceId, sessionKey])

  useEffect(() => {
    if (!props.open || !serviceId || !sessionKey) {
      resetState()
      return
    }

    resetState()
    activeSessionRef.current = sessionKey
    setInitialLoadState('loading')
    setLocateState(targetVersion ? 'loading' : 'idle')

    let cancelled = false

    void (async () => {
      const pageResponse = await fetchPage(sessionKey, serviceId, null)
      if (cancelled || activeSessionRef.current !== sessionKey) return

      const resolvedLocateResponse = targetVersion
        ? await locateAcrossPages(sessionKey, serviceId, targetVersion, pageResponse)
        : null

      if (resolvedLocateResponse) {
        setLocateResponse(resolvedLocateResponse)
        setLocateState('ready')
      } else {
        setLocateState('idle')
      }
    })()

    return () => {
      cancelled = true
    }
  }, [fetchPage, locateAcrossPages, props.open, resetState, serviceId, sessionKey, targetVersion])

  const rowCount = items.length + ((hasMoreRef.current || loadingMore || loadMoreFailure) ? 1 : 0)
  const isReady = listResponse?.status === 'ready'

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 220,
    overscan: 6,
    gap: RELEASE_ROW_GAP,
    getItemKey: (index) => (index < items.length ? items[index]?.id ?? index : `loader:${index}`),
    measureElement: (element) => element.getBoundingClientRect().height,
  })

  useEffect(() => {
    virtualizer.measure()
  }, [expandedIds, items.length, locateResponse?.status, viewMode, virtualizer])

  const virtualItems = virtualizer.getVirtualItems()
  const listOffset = virtualItems[0]?.start ?? 0

  useEffect(() => {
    const lastItem = [...virtualItems].reverse()[0]
    if (!lastItem) return
    if (lastItem.index < items.length - 1) return
    if (!hasMoreRef.current || loadingMore || loadMoreFailure || initialLoadState !== 'ready') return
    void loadNextPage()
  }, [initialLoadState, items.length, loadMoreFailure, loadNextPage, loadingMore, virtualItems])

  useEffect(() => {
    if (!props.open || !sessionKey || !locateResponse || locateResponse.status !== 'found') return
    const absoluteIndex = locateResponse.absoluteIndex ?? null
    if (absoluteIndex == null || absoluteIndex < 0 || items.length <= absoluteIndex) return

    const key = `${sessionKey}:${absoluteIndex}`
    if (targetScrollKeyRef.current === key) return
    targetScrollKeyRef.current = key

    const targetItem = items[absoluteIndex]
    if (!targetItem) return

    const frame = window.requestAnimationFrame(() => {
      virtualizer.scrollToIndex(absoluteIndex, { align: 'center', behavior: 'smooth' })
      setHighlightedId(targetItem.id)
      if (highlightTimerRef.current != null) {
        window.clearTimeout(highlightTimerRef.current)
      }
      highlightTimerRef.current = window.setTimeout(() => {
        setHighlightedId((current) => (current === targetItem.id ? null : current))
      }, TARGET_HIGHLIGHT_MS)
    })

    return () => window.cancelAnimationFrame(frame)
  }, [items, locateResponse, props.open, sessionKey, virtualizer])

  const repo = listResponse?.repo ?? null
  const repoUrl = repo?.htmlUrl ?? null
  const locateBanner = useMemo(() => {
    if (!targetVersion || locateState !== 'ready' || !locateResponse) return null
    if (locateResponse.status === 'found') {
      return {
        tone: 'success' as const,
        message: `已定位到 ${locateResponse.matchedTag ?? targetVersion}，正在滚动到对应发布记录。`,
      }
    }
    if (!locateResponse.message) return null
    return {
      tone: statusTone(locateResponse.status),
      message: locateResponse.message,
    }
  }, [locateResponse, locateState, targetVersion])

  const listBanner = useMemo(() => {
    if (!listResponse || listResponse.status === 'ready' || !listResponse.message) return null
    return {
      tone: statusTone(listResponse.status),
      message: listResponse.message,
    }
  }, [listResponse])

  const surfaceBanner = isReady ? locateBanner : listBanner ?? locateBanner
  const fallbackBanner = listResponse?.fallback
    ? { tone: 'warning' as const, message: listResponse.fallback.message }
    : null
  const loaderVisible = initialLoadState === 'loading' && items.length === 0
  const unsupportedOrErrored = initialLoadState === 'ready' && listResponse && listResponse.status !== 'ready'
  const emptyReady = isReady && items.length === 0
  const showSettingsAction = shouldOfferSettingsAction(listResponse)

  const openSettings = () => {
    closeGitHubReleaseDrawer('replace')
    navigate({ name: 'settings' })
  }

  const cancelInfoPanelClose = useCallback(() => {
    if (infoCloseTimerRef.current != null) {
      window.clearTimeout(infoCloseTimerRef.current)
      infoCloseTimerRef.current = null
    }
  }, [])

  const openInfoPanel = useCallback(() => {
    cancelInfoPanelClose()
    setInfoPanelOpen(true)
  }, [cancelInfoPanelClose])

  const scheduleInfoPanelClose = useCallback(() => {
    cancelInfoPanelClose()
    infoCloseTimerRef.current = window.setTimeout(() => {
      setInfoPanelOpen(false)
      infoCloseTimerRef.current = null
    }, 120)
  }, [cancelInfoPanelClose])

  const handleInfoPanelBlur = (event: FocusEvent<HTMLDivElement>) => {
    const nextTarget = event.relatedTarget
    if (nextTarget instanceof Node && event.currentTarget.contains(nextTarget)) return
    scheduleInfoPanelClose()
  }

  const toggleExpanded = (id: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  return (
    <Drawer
      direction="right"
      modal
      open={props.open && Boolean(serviceId)}
      onOpenChange={props.onOpenChange}
    >
      <DrawerContent className="releaseDrawerContent" aria-describedby="github-release-drawer-description">
        <DrawerHeader className="releaseDrawerHeader">
          <div className="releaseDrawerHeaderTop">
            <div className="releaseDrawerHeaderText">
              <div className="releaseDrawerTitleRow">
                <DrawerTitle asChild>
                  <div className="modalTitle">发布记录</div>
                </DrawerTitle>
                {listResponse || targetVersion ? (
                  <div
                    className="releaseDrawerInfoInline"
                    onBlurCapture={handleInfoPanelBlur}
                    onFocusCapture={openInfoPanel}
                    onPointerEnter={openInfoPanel}
                    onPointerLeave={scheduleInfoPanelClose}
                  >
                    <button
                      type="button"
                      className="releaseDrawerInfoTrigger"
                      aria-controls={drawerInfoId}
                      aria-expanded={infoPanelOpen}
                      aria-label="查看扩展信息"
                      data-release-drawer-info-trigger="true"
                    >
                      <InfoIcon className="iconSm" />
                    </button>
                    {infoPanelOpen ? (
                      <div
                        id={drawerInfoId}
                        className="releaseDrawerInfoTooltip"
                        data-release-drawer-info-tooltip="true"
                        role="tooltip"
                      >
                        <div className="releaseDrawerInfoTooltipTitle">扩展信息</div>
                        {listResponse ? (
                          <div className="releaseDrawerInfoTooltipRow">
                            <span className="releaseDrawerInfoTooltipLabel">数据来源</span>
                            <span className="releaseDrawerInfoTooltipValue">{sourceLabel(listResponse)}</span>
                          </div>
                        ) : null}
                        {listResponse ? (
                          <div className="releaseDrawerInfoTooltipRow">
                            <span className="releaseDrawerInfoTooltipLabel">默认视图</span>
                            <span className="releaseDrawerInfoTooltipValue">{viewLabel(listResponse.defaultView)}</span>
                          </div>
                        ) : null}
                        {targetVersion ? (
                          <div className="releaseDrawerInfoTooltipRow">
                            <span className="releaseDrawerInfoTooltipLabel">定位版本</span>
                            <span className="releaseDrawerInfoTooltipValue">{targetVersion}</span>
                          </div>
                        ) : null}
                      </div>
                    ) : null}
                  </div>
                ) : null}
              </div>
              <DrawerDescription asChild>
                <div className="releaseDrawerDescription" id="github-release-drawer-description">
                  查看该服务对应仓库的发布记录，并可按版本号快速定位。
                </div>
              </DrawerDescription>
            </div>
            <div className="releaseDrawerHeaderActions">
              {repoUrl ? (
                <a
                  className="releaseDrawerIconLink"
                  href={repoUrl}
                  rel="noreferrer"
                  target="_blank"
                  title="打开 GitHub 仓库"
                >
                  <ExternalLinkIcon className="iconSm" />
                </a>
              ) : null}
              <DrawerClose asChild>
                <button
                  type="button"
                  className="releaseDrawerCloseButton"
                  aria-label="关闭发布记录抽屉"
                  data-release-drawer-close="true"
                  title="关闭"
                >
                  <CloseIcon className="iconSm" />
                </button>
              </DrawerClose>
            </div>
          </div>
          {repo ? (
            <div className="releaseDrawerHeaderMeta">
              <span className="releaseDrawerChip"><Mono>{repo.fullName}</Mono></span>
              {listResponse?.source ? (
                <span className="releaseDrawerChip">{sourceLabel(listResponse)}</span>
              ) : null}
            </div>
          ) : null}
          {isReady ? (
            <div className="releaseDrawerViewTabs" aria-label="发布说明视图">
              {(['smart', 'translated', 'original'] as const).map((view) => (
                <button
                  key={view}
                  type="button"
                  className={cn('releaseDrawerViewTab', viewMode === view && 'releaseDrawerViewTabActive')}
                  aria-pressed={viewMode === view}
                  onClick={() => setViewMode(view)}
                >
                  {viewLabel(view)}
                </button>
              ))}
            </div>
          ) : null}
          {fallbackBanner ? (
            <div className="releaseDrawerBanner releaseDrawerBanner-warning" data-release-drawer-banner="fallback">
              <span>{fallbackBanner.message}</span>
              {showSettingsAction ? (
                <Button variant="ghost" onClick={openSettings}>打开设置</Button>
              ) : null}
            </div>
          ) : null}
          {surfaceBanner ? (
            <div className={cn('releaseDrawerBanner', `releaseDrawerBanner-${surfaceBanner.tone}`)} data-release-drawer-banner={surfaceBanner.tone}>
              <span>{surfaceBanner.message}</span>
              {showSettingsAction ? (
                <Button variant="ghost" onClick={openSettings}>打开设置</Button>
              ) : null}
            </div>
          ) : null}
        </DrawerHeader>

        <div className="releaseDrawerBody">
          {loaderVisible ? (
            <div className="releaseDrawerState" data-release-drawer-state="loading">
              <span className="btnInlineSpinner" aria-hidden="true" />
              <span>正在加载 GitHub 发布记录…</span>
            </div>
          ) : null}

          {unsupportedOrErrored ? (
            <div className="releaseDrawerState releaseDrawerStateError" data-release-drawer-state={listResponse?.status}>
              <div className="releaseDrawerStateTitle">无法读取发布记录</div>
              <div className="releaseDrawerStateMessage">{listResponse?.message ?? '请稍后重试。'}</div>
              {showSettingsAction ? (
                <div className="releaseDrawerStateActions">
                  <Button variant="ghost" onClick={openSettings}>打开设置</Button>
                </div>
              ) : null}
            </div>
          ) : null}

          {emptyReady ? (
            <div className="releaseDrawerState" data-release-drawer-state="empty">
              <div className="releaseDrawerStateTitle">暂无发布记录</div>
              <div className="releaseDrawerStateMessage">该 GitHub 仓库当前没有可展示的 Releases。</div>
            </div>
          ) : null}

          {isReady && items.length > 0 ? (
            <div className="releaseDrawerScrollShell">
              <ScrollArea
                className="releaseDrawerScrollArea"
                type="always"
                viewportClassName="releaseDrawerScrollViewport"
                viewportRef={scrollRef}
              >
                <div
                  className="releaseDrawerList"
                  style={{ height: `${virtualizer.getTotalSize()}px` }}
                  data-release-drawer="true"
                >
                  <div
                    className="releaseDrawerListInner"
                    style={{ transform: `translateY(${listOffset}px)` }}
                  >
                    {virtualItems.map((virtualRow) => {
                      const isLoaderRow = virtualRow.index >= items.length
                      const item = items[virtualRow.index]
                      if (isLoaderRow || !item) {
                        return (
                          <div
                            key={virtualRow.key}
                            data-index={virtualRow.index}
                            ref={virtualizer.measureElement}
                            className="releaseDrawerVirtualRow"
                          >
                            <div className="releaseDrawerLoaderRow">
                              {loadingMore ? (
                                <>
                                  <span className="btnInlineSpinner" aria-hidden="true" />
                                  <span>正在加载更多发布记录…</span>
                                </>
                              ) : loadMoreFailure ? (
                                <>
                                  <span>{loadMoreFailure.message ?? '加载更多失败。'}</span>
                                  <Button variant="ghost" onClick={() => void loadNextPage()}>重试</Button>
                                </>
                              ) : hasMoreRef.current ? (
                                <span>继续下滑以加载更多发布记录</span>
                              ) : (
                                <span>已经到底了</span>
                              )}
                            </div>
                          </div>
                        )
                      }

                      const expanded = expandedIds.has(item.id)
                      const selectedBody = releaseBodyForView(item, viewMode)
                      const body = selectedBody.body
                      const showExpand = hasLongBody(body)
                      const publishedAt = preferredReleaseTimestamp(item)
                      const htmlUrl = safeHttpUrl(item.htmlUrl)
                      const matched = locateResponse?.status === 'found'
                        ? item.id === highlightedId || releaseMatchesVersion(item, targetVersion)
                        : item.id === highlightedId

                      return (
                        <article
                          key={virtualRow.key}
                          data-index={virtualRow.index}
                          data-release-tag={item.tagName}
                          data-release-highlighted={matched ? 'true' : 'false'}
                          ref={virtualizer.measureElement}
                          className="releaseDrawerVirtualRow"
                        >
                          <div className={cn('releaseDrawerItem', matched && 'releaseDrawerItemHighlighted')}>
                            <div className="releaseDrawerItemHeader">
                              <div className="releaseDrawerItemTitleWrap">
                                <div className="releaseDrawerItemTitleRow">
                                  <Mono>{item.tagName}</Mono>
                                  {item.name && item.name.trim() && item.name.trim() !== item.tagName ? (
                                    <span className="releaseDrawerItemName">{item.name}</span>
                                  ) : null}
                                </div>
                                <div className="releaseDrawerItemMeta">
                                  <span>{formatReleaseDate(publishedAt)}</span>
                                  {item.prerelease ? <span className="releaseDrawerBadge">prerelease</span> : null}
                                  {item.draft ? <span className="releaseDrawerBadge">draft</span> : null}
                                  {targetVersion && releaseMatchesVersion(item, targetVersion) ? (
                                    <span className="releaseDrawerBadge releaseDrawerBadgeTarget">目标版本</span>
                                  ) : null}
                                </div>
                              </div>
                              {htmlUrl ? (
                                <a
                                  className="releaseDrawerItemLink"
                                  href={htmlUrl}
                                  rel="noreferrer"
                                  target="_blank"
                                  title={`打开 ${item.tagName} 的发布记录`}
                                >
                                  <ExternalLinkIcon className="iconSm" />
                                </a>
                              ) : null}
                            </div>

                            {body ? (
                              <div className="releaseDrawerItemBodyWrap">
                                {selectedBody.missing ? (
                                  <div className="releaseDrawerItemViewFallback">
                                    {viewLabel(viewMode)}不可用，已显示原文。
                                  </div>
                                ) : null}
                                <pre className={cn('releaseDrawerItemBody', !expanded && 'releaseDrawerItemBodyCollapsed')}>
                                  {body}
                                </pre>
                                {showExpand ? (
                                  <button
                                    type="button"
                                    className="releaseDrawerExpandBtn"
                                    onClick={() => toggleExpanded(item.id)}
                                  >
                                    {expanded ? '收起说明' : '展开说明'}
                                  </button>
                                ) : null}
                              </div>
                            ) : (
                              <div className="releaseDrawerItemEmptyBody">暂无 Release 说明。</div>
                            )}
                          </div>
                        </article>
                      )
                    })}
                  </div>
                </div>
              </ScrollArea>
            </div>
          ) : null}
        </div>
      </DrawerContent>
    </Drawer>
  )
}
