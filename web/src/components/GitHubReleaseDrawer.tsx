import { useCallback, useEffect, useId, useMemo, useRef, useState, type FocusEvent } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'

import {
  type ServiceReleaseNoteItem,
  type ServiceReleaseNotesResponse,
} from '../api'
import {
  releaseNotesBodyForView,
  findReleaseNoteIndex,
  releaseNotesShouldOfferSettingsAction,
  releaseNotesSourceLabel,
  releaseNotesTagMatchesVersion,
  releaseNotesViewLabel,
} from '../releaseNotes'
import { navigate } from '../routes'
import { closeGitHubReleaseDrawer } from '../releaseDrawer'
import { useServiceReleaseNotesSession } from '../useServiceReleaseNotesSession'
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

function statusTone(
  status:
    | ServiceReleaseNotesResponse['status']
    | NonNullable<ServiceReleaseNotesResponse['anchor']>['status']
    | 'info',
): 'info' | 'warning' | 'danger' | 'success' {
  if (status === 'found') return 'success'
  if (status === 'notFound' || status === 'outsideWindow' || status === 'info') return 'warning'
  if (status === 'upstreamError') return 'danger'
  return 'info'
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
  const targetScrollKeyRef = useRef<string | null>(null)
  const highlightTimerRef = useRef<number | null>(null)
  const infoCloseTimerRef = useRef<number | null>(null)

  const {
    loadState,
    response: listResponse,
    items,
    viewMode,
    setViewMode,
    loadingOlder,
    loadingNewer,
    olderFailure,
    newerFailure,
    hasOlder,
    hasNewer,
    loadOlder,
    loadNewer,
  } = useServiceReleaseNotesSession({
    enabled: props.open && Boolean(serviceId),
    serviceId,
    targetVersion,
    locateTargetVersion: Boolean(targetVersion),
    limit: RELEASES_PER_PAGE,
  })
  const [expandedIds, setExpandedIds] = useState<Set<string>>(() => new Set())
  const [highlightedId, setHighlightedId] = useState<string | null>(null)
  const [infoPanelOpen, setInfoPanelOpen] = useState(false)

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

  useEffect(() => {
    targetScrollKeyRef.current = null
    setHighlightedId(null)
    setInfoPanelOpen(false)
    setExpandedIds(new Set())
  }, [sessionKey])

  const anchor = listResponse?.anchor ?? null
  const topLoaderVisible = hasNewer || loadingNewer || newerFailure != null
  const bottomLoaderVisible = hasOlder || loadingOlder || olderFailure != null
  const topLoaderOffset = topLoaderVisible ? 1 : 0
  const rowCount = items.length + (topLoaderVisible ? 1 : 0) + (bottomLoaderVisible ? 1 : 0)
  const isReady = listResponse?.status === 'ready'

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 220,
    overscan: 6,
    gap: RELEASE_ROW_GAP,
    getItemKey: (index) => {
      if (topLoaderVisible && index === 0) return 'loader:newer'
      const itemIndex = index - topLoaderOffset
      if (itemIndex >= 0 && itemIndex < items.length) return items[itemIndex]?.id ?? itemIndex
      return 'loader:older'
    },
    measureElement: (element) => element.getBoundingClientRect().height,
  })

  useEffect(() => {
    virtualizer.measure()
  }, [anchor?.status, expandedIds, items.length, topLoaderVisible, viewMode, virtualizer])

  const virtualItems = virtualizer.getVirtualItems()
  const listOffset = virtualItems[0]?.start ?? 0

  useEffect(() => {
    const firstItem = virtualItems[0]
    if (
      firstItem?.index === 0 &&
      topLoaderVisible &&
      hasNewer &&
      !loadingNewer &&
      !newerFailure &&
      loadState === 'ready'
    ) {
      void loadNewer()
    }
  }, [hasNewer, loadNewer, loadState, loadingNewer, newerFailure, topLoaderVisible, virtualItems])

  useEffect(() => {
    const lastItem = [...virtualItems].reverse()[0]
    if (!lastItem) return
    if (lastItem.index < rowCount - 1) return
    if (!bottomLoaderVisible || !hasOlder || loadingOlder || olderFailure || loadState !== 'ready') return
    void loadOlder()
  }, [bottomLoaderVisible, hasOlder, loadOlder, loadState, loadingOlder, olderFailure, rowCount, virtualItems])

  useEffect(() => {
    if (!props.open || !sessionKey || anchor?.status !== 'found') return
    const absoluteIndex = findReleaseNoteIndex(items, targetVersion)
    if (absoluteIndex < 0 || items.length <= absoluteIndex) return

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
  }, [anchor?.status, items, props.open, sessionKey, targetVersion, virtualizer])

  const repo = listResponse?.repo ?? null
  const repoUrl = repo?.htmlUrl ?? null
  const locateBanner = useMemo(() => {
    if (!targetVersion || !anchor) return null
    if (anchor.status === 'found') {
      return {
        tone: 'success' as const,
        message: `已定位到 ${anchor.matchedTag ?? targetVersion}，正在滚动到对应发布记录。`,
      }
    }
    if (!anchor.message) return null
    return {
      tone: statusTone(anchor.status),
      message: anchor.message,
    }
  }, [anchor, targetVersion])

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
  const loaderVisible = loadState === 'loading' && items.length === 0
  const unsupportedOrErrored = loadState === 'ready' && listResponse && listResponse.status !== 'ready'
  const emptyReady = isReady && items.length === 0
  const showSettingsAction = releaseNotesShouldOfferSettingsAction(listResponse)

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
                            <span className="releaseDrawerInfoTooltipValue">{releaseNotesSourceLabel(listResponse)}</span>
                          </div>
                        ) : null}
                        {listResponse ? (
                          <div className="releaseDrawerInfoTooltipRow">
                            <span className="releaseDrawerInfoTooltipLabel">默认视图</span>
                            <span className="releaseDrawerInfoTooltipValue">{releaseNotesViewLabel(listResponse.defaultView)}</span>
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
                <span className="releaseDrawerChip">{releaseNotesSourceLabel(listResponse)}</span>
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
                  {releaseNotesViewLabel(view)}
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
                      const isTopLoaderRow = topLoaderVisible && virtualRow.index === 0
                      const itemIndex = virtualRow.index - topLoaderOffset
                      const item = itemIndex >= 0 && itemIndex < items.length ? items[itemIndex] : null
                      if (isTopLoaderRow || !item) {
                        return (
                          <div
                            key={virtualRow.key}
                            data-index={virtualRow.index}
                            ref={virtualizer.measureElement}
                            className="releaseDrawerVirtualRow"
                          >
                            <div className="releaseDrawerLoaderRow">
                              {isTopLoaderRow ? (
                                loadingNewer ? (
                                  <>
                                    <span className="btnInlineSpinner" aria-hidden="true" />
                                    <span>正在加载更新发布记录…</span>
                                  </>
                                ) : newerFailure ? (
                                  <>
                                    <span>{newerFailure.message ?? '加载更新发布记录失败。'}</span>
                                    <Button variant="ghost" onClick={() => void loadNewer()}>重试</Button>
                                  </>
                                ) : hasNewer ? (
                                  <span>继续上滑以加载更新发布记录</span>
                                ) : (
                                  <span>已经到顶部了</span>
                                )
                              ) : (
                                loadingOlder ? (
                                  <>
                                    <span className="btnInlineSpinner" aria-hidden="true" />
                                    <span>正在加载更旧发布记录…</span>
                                  </>
                                ) : olderFailure ? (
                                  <>
                                    <span>{olderFailure.message ?? '加载更旧发布记录失败。'}</span>
                                    <Button variant="ghost" onClick={() => void loadOlder()}>重试</Button>
                                  </>
                                ) : hasOlder ? (
                                  <span>继续下滑以加载更旧发布记录</span>
                                ) : (
                                  <span>已经到底了</span>
                                )
                              )}
                            </div>
                          </div>
                        )
                      }

                      const expanded = expandedIds.has(item.id)
                      const selectedBody = releaseNotesBodyForView(item, viewMode)
                      const body = selectedBody.body
                      const showExpand = hasLongBody(body)
                      const publishedAt = preferredReleaseTimestamp(item)
                      const htmlUrl = safeHttpUrl(item.htmlUrl)
                      const matched = anchor?.status === 'found'
                        ? item.id === highlightedId || releaseNotesTagMatchesVersion(item, targetVersion)
                        : item.id === highlightedId

                      return (
                        <article
                          key={virtualRow.key}
                          data-index={itemIndex}
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
                                  {targetVersion && releaseNotesTagMatchesVersion(item, targetVersion) ? (
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
                                    {releaseNotesViewLabel(viewMode)}不可用，已显示原文。
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
