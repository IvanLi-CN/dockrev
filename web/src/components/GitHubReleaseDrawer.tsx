import { useCallback, useEffect, useId, useMemo, useRef, useState, type FocusEvent } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'

import {
  ApiError,
  getServiceGitHubReleases,
  type GitHubReleaseAuthMode,
  type ServiceGitHubReleaseItem,
  type ServiceGitHubReleaseLocateResponse,
  type ServiceGitHubReleaseLocateStatus,
  type ServiceGitHubReleasesResponse,
  type ServiceGitHubReleasesStatus,
} from '../api'
import {
  buildReleaseLocateNotFoundResponse,
  RELEASE_DRAWER_LOCATE_LIMIT,
  shouldContinueReleaseLocateSearch,
} from '../githubReleaseDrawerState'
import { navigate } from '../routes'
import { closeGitHubReleaseDrawer } from '../releaseDrawer'
import { requestSettingsFocus } from '../settingsFocus'
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
const TARGET_HIGHLIGHT_MS = 2200
const RELEASE_ROW_GAP = 12

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

function preferredReleaseTimestamp(item: ServiceGitHubReleaseItem): string | null {
  return item.publishedAt?.trim() || item.createdAt?.trim() || null
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
  status: ServiceGitHubReleasesStatus | ServiceGitHubReleaseLocateStatus | 'info',
): 'info' | 'warning' | 'danger' | 'success' {
  if (status === 'found') return 'success'
  if (status === 'outsideWindow' || status === 'notFound' || status === 'info') return 'warning'
  if (status === 'permissionDenied' || status === 'rateLimited' || status === 'upstreamError') return 'danger'
  return 'info'
}

function authModeLabel(authMode: GitHubReleaseAuthMode | null | undefined): string {
  return authMode === 'pat' ? 'PAT' : authMode === 'anonymous' ? '匿名' : '未知'
}

function shouldOfferSettingsAction(
  status: ServiceGitHubReleasesStatus | ServiceGitHubReleaseLocateStatus | null | undefined,
  authMode: GitHubReleaseAuthMode | null | undefined,
  message: string | null | undefined,
): boolean {
  if (status === 'permissionDenied' || status === 'rateLimited') return true
  if (status !== 'upstreamError') return false
  if (!message?.trim()) return false
  return authMode === 'anonymous'
    ? message.includes('配置 GitHub PAT')
    : message.includes('GitHub PAT') || message.includes('token 权限')
}

function fallbackReleaseErrorMessage(error: unknown): string {
  if (error instanceof ApiError && error.status === 404) {
    return '该服务不存在或已被删除，无法读取 GitHub 发布记录。'
  }
  if (error instanceof Error) {
    const message = error.message.trim()
    if (message) return message
  }
  return 'GitHub Releases 拉取失败，请稍后重试。'
}

function buildListFailureResponse(
  error: unknown,
  page: number,
  perPage: number,
): ServiceGitHubReleasesResponse {
  return {
    status: 'upstreamError',
    authMode: 'anonymous',
    repo: null,
    page,
    perPage,
    hasMore: false,
    items: [],
    message: fallbackReleaseErrorMessage(error),
  }
}

function buildLocateFailureResponse(
  error: unknown,
  version: string,
): ServiceGitHubReleaseLocateResponse {
  return {
    status: 'upstreamError',
    authMode: 'anonymous',
    repo: null,
    version,
    searchedCount: 0,
    matchedTag: null,
    page: null,
    indexWithinPage: null,
    absoluteIndex: null,
    message: fallbackReleaseErrorMessage(error),
  }
}

function releaseMatchesVersion(item: ServiceGitHubReleaseItem, version: string | null | undefined): boolean {
  const normalizedVersion = normalizeVersion(version)
  if (!normalizedVersion) return false
  const normalizedTag = normalizeVersion(item.tagName)
  if (normalizedTag === normalizedVersion) return true
  if (normalizedTag === `v${normalizedVersion}`) return true
  if (normalizedVersion.startsWith('v') && normalizedTag === normalizedVersion.slice(1)) return true
  return false
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
  const inFlightPagesRef = useRef<Map<string, Promise<ServiceGitHubReleasesResponse | null>>>(new Map())
  const hasMoreRef = useRef(false)
  const loadingMoreRef = useRef(false)
  const targetScrollKeyRef = useRef<string | null>(null)
  const highlightTimerRef = useRef<number | null>(null)
  const infoCloseTimerRef = useRef<number | null>(null)

  const [initialLoadState, setInitialLoadState] = useState<'idle' | 'loading' | 'ready'>('idle')
  const [listResponse, setListResponse] = useState<ServiceGitHubReleasesResponse | null>(null)
  const [locateState, setLocateState] = useState<'idle' | 'loading' | 'ready'>('idle')
  const [locateResponse, setLocateResponse] = useState<ServiceGitHubReleaseLocateResponse | null>(null)
  const [items, setItems] = useState<ServiceGitHubReleaseItem[]>([])
  const [expandedIds, setExpandedIds] = useState<Set<number>>(() => new Set())
  const [loadingMore, setLoadingMore] = useState(false)
  const [loadMoreFailure, setLoadMoreFailure] = useState<ServiceGitHubReleasesResponse | null>(null)
  const [highlightedId, setHighlightedId] = useState<number | null>(null)
  const [infoPanelOpen, setInfoPanelOpen] = useState(false)

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
  }, [])

  const fetchPage = useCallback(
    async (expectedSession: string, targetServiceId: string, page: number) => {
      const requestKey = `${expectedSession}:${page}`
      const existing = inFlightPagesRef.current.get(requestKey)
      if (existing) {
        return await existing
      }

      const request = (async () => {
        let response: ServiceGitHubReleasesResponse
        try {
          response = await getServiceGitHubReleases(targetServiceId, {
            page,
            perPage: RELEASES_PER_PAGE,
          })
        } catch (error) {
          response = buildListFailureResponse(error, page, RELEASES_PER_PAGE)
        }
        if (activeSessionRef.current !== expectedSession) return null

        if (page === 1) {
          setListResponse(response)
          setInitialLoadState('ready')
        }

        if (response.status !== 'ready') {
          if (page === 1) {
            setItems([])
            loadedPagesRef.current = 0
            hasMoreRef.current = false
          } else {
            setLoadMoreFailure(response)
          }
          return response
        }

        loadedPagesRef.current = Math.max(loadedPagesRef.current, page)
        hasMoreRef.current = response.hasMore
        setLoadMoreFailure(null)
        setItems((prev) => {
          if (page === 1) return response.items
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
      initialResponse: ServiceGitHubReleasesResponse | null,
    ): Promise<ServiceGitHubReleaseLocateResponse | null> => {
      let response = initialResponse
      let searchedCount = 0
      while (response && response.status === 'ready') {
        const remainingBudget = RELEASE_DRAWER_LOCATE_LIMIT - searchedCount
        if (remainingBudget <= 0) {
          return buildReleaseLocateNotFoundResponse(response, version, searchedCount)
        }
        const scanCount = Math.min(response.items.length, remainingBudget)
        const scanItems = response.items.slice(0, scanCount)
        const matchedIndex = scanItems.findIndex((item) => releaseMatchesVersion(item, version))
        if (matchedIndex >= 0) {
          return {
            status: 'found',
            authMode: response.authMode,
            repo: response.repo ?? null,
            version,
            searchedCount: searchedCount + scanCount,
            matchedTag: scanItems[matchedIndex]?.tagName ?? version,
            page: response.page,
            indexWithinPage: matchedIndex,
            absoluteIndex: searchedCount + matchedIndex,
            message: null,
          }
        }
        searchedCount += scanCount
        if (!shouldContinueReleaseLocateSearch(response, searchedCount)) {
          return buildReleaseLocateNotFoundResponse(response, version, searchedCount)
        }
        const nextPage = response.page + 1
        const nextResponse = await fetchPage(expectedSession, targetServiceId, nextPage)
        if (!nextResponse) return null
        if (nextResponse.status !== 'ready') {
          return buildLocateFailureResponse(new Error(nextResponse.message ?? 'load failed'), version)
        }
        response = nextResponse
      }
      return buildReleaseLocateNotFoundResponse(initialResponse, version, searchedCount)
    },
    [fetchPage],
  )

  const loadNextPage = useCallback(async () => {
    if (!sessionKey || !serviceId) return
    if (loadingMoreRef.current || loadingMore || !hasMoreRef.current) return
    const nextPage = loadedPagesRef.current + 1
    if (nextPage <= 1) return
    loadingMoreRef.current = true
    setLoadingMore(true)
    try {
      await fetchPage(sessionKey, serviceId, nextPage)
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
      const pageResponse = await fetchPage(sessionKey, serviceId, 1)
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
  }, [expandedIds, items.length, locateResponse?.status, virtualizer])

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

  const authMode = listResponse?.authMode ?? locateResponse?.authMode ?? null
  const repo = listResponse?.repo ?? locateResponse?.repo ?? null
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
  const loaderVisible = initialLoadState === 'loading' && items.length === 0
  const unsupportedOrErrored = initialLoadState === 'ready' && listResponse && listResponse.status !== 'ready'
  const emptyReady = isReady && items.length === 0
  const showSettingsAction =
    shouldOfferSettingsAction(listResponse?.status, listResponse?.authMode, listResponse?.message) ||
    shouldOfferSettingsAction(locateResponse?.status, locateResponse?.authMode, locateResponse?.message)

  const openSettings = () => {
    closeGitHubReleaseDrawer('replace')
    requestSettingsFocus('ghcr-webhook')
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

  const toggleExpanded = (id: number) => {
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
                  <div className="modalTitle">GitHub Releases</div>
                </DrawerTitle>
                {authMode || targetVersion ? (
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
                        {authMode ? (
                          <div className="releaseDrawerInfoTooltipRow">
                            <span className="releaseDrawerInfoTooltipLabel">访问身份</span>
                            <span className="releaseDrawerInfoTooltipValue">{authModeLabel(authMode)}</span>
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
                  查看该服务对应 GitHub 仓库的发布记录，并可按版本号快速定位。
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
                      const body = (item.body ?? '').trim()
                      const showExpand = hasLongBody(body)
                      const publishedAt = preferredReleaseTimestamp(item)
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
                              <a
                                className="releaseDrawerItemLink"
                                href={item.htmlUrl}
                                rel="noreferrer"
                                target="_blank"
                                title={`打开 ${item.tagName} 的 GitHub Release`}
                              >
                                <ExternalLinkIcon className="iconSm" />
                              </a>
                            </div>

                            {body ? (
                              <div className="releaseDrawerItemBodyWrap">
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
