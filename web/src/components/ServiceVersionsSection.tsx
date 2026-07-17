import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useVirtualizer, type VirtualItem } from '@tanstack/react-virtual'
import {
  type JobListItem,
  type Service,
  type ServiceBackupRecordItem,
  type ServiceReleaseNoteItem,
  type ServiceReleaseNotesResponse,
  type ServiceRollbackTargetResponse,
} from '../api'
import { useConfirm } from '../confirm'
import { cn } from '../lib/utils'
import { navigate } from '../routes'
import {
  findReleaseNoteIndex,
  releaseNotesBodyForView,
  releaseNotesShouldOfferSettingsAction,
  releaseNotesSourceLabel,
  releaseNotesTagMatchesVersion,
  releaseNotesViewLabel,
} from '../releaseNotes'
import { useServiceReleaseNotesSession } from '../useServiceReleaseNotesSession'
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
  normalizeVersion,
  preferredReleaseTimestamp,
  safeHttpUrl,
} from './serviceVersionsSectionUtils'

const RELEASES_PER_PAGE = 20
const RELEASE_ROW_GAP = 14
const VERSION_INDEX_ROW_HEIGHT = 54
const DESKTOP_VERSION_INDEX_QUERY = '(min-width: 1101px)'
const EMPTY_VIRTUAL_ITEMS: VirtualItem[] = []

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
  status: ServiceReleaseNotesResponse['status'] | 'info',
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
  const initialCenterKeyRef = useRef<string | null>(null)

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
    enabled: Boolean(serviceId),
    serviceId,
    targetVersion: currentVersion,
    locateTargetVersion: Boolean(currentVersion),
    limit: RELEASES_PER_PAGE,
  })
  const [expandedIds, setExpandedIds] = useState<Set<string>>(() => new Set())
  const [selectedIndex, setSelectedIndex] = useState(0)
  const anchorState = listResponse?.anchor ?? null
  const anchorState = listResponse?.anchor ?? null

  useEffect(() => {
    initialCenterKeyRef.current = null
    if (listScrollRef.current) listScrollRef.current.scrollTop = 0
    if (indexScrollRef.current) indexScrollRef.current.scrollTop = 0
    setExpandedIds(new Set())
    setSelectedIndex(0)
  }, [sessionKey])
  const topLoaderVisible = hasNewer || loadingNewer || newerFailure != null
  const bottomLoaderVisible = hasOlder || loadingOlder || olderFailure != null
  const topLoaderOffset = topLoaderVisible ? 1 : 0
  const rowCount = items.length + (topLoaderVisible ? 1 : 0) + (bottomLoaderVisible ? 1 : 0)
  const listVirtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => listScrollRef.current,
    estimateSize: () => 360,
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
  const indexVirtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => indexScrollRef.current,
    estimateSize: () => VERSION_INDEX_ROW_HEIGHT,
    overscan: 10,
    gap: 8,
    getItemKey: (index) => {
      if (topLoaderVisible && index === 0) return 'index-loader:newer'
      const itemIndex = index - topLoaderOffset
      if (itemIndex >= 0 && itemIndex < items.length) return items[itemIndex]?.id ?? itemIndex
      return `index-loader:older:${index}`
    },
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
          listVirtualizer.getOffsetForIndex(absoluteIndex + topLoaderOffset, 'center'),
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
          indexVirtualizer.scrollToIndex(absoluteIndex + topLoaderOffset, {
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
    [indexVirtualizer, listVirtualizer, sessionKey, showDesktopIndex, topLoaderOffset],
  )

  useEffect(() => {
    if (loadState !== 'ready') return
    if (anchorState?.status !== 'found') return
    const anchorIndex = findReleaseNoteIndex(items, currentVersion)
    if (anchorIndex < 0 || items.length <= anchorIndex) return
    const scrollElement = listScrollRef.current
    if (!scrollElement) return
    const key = `${sessionKey}:${anchorIndex}`
    if (initialCenterKeyRef.current === key && scrollElement.scrollTop > 0) return
    setSelectedIndex(anchorIndex)
    return centerVersionCard(
      anchorIndex,
      items[anchorIndex]?.tagName ?? '',
      'initial',
    )
  }, [anchorState, centerVersionCard, currentVersion, items, loadState, sessionKey])

  const listVirtualItems = listVirtualizer.getVirtualItems()
  const indexVirtualItems = showDesktopIndex ? indexVirtualizer.getVirtualItems() : EMPTY_VIRTUAL_ITEMS
  const listOffset = listVirtualItems[0]?.start ?? 0
  const indexOffset = indexVirtualItems[0]?.start ?? 0

  useEffect(() => {
    const firstListItem = listVirtualItems[0]
    const firstIndexItem = indexVirtualItems[0]
    if (
      (firstListItem?.index === 0 || firstIndexItem?.index === 0) &&
      topLoaderVisible &&
      hasNewer &&
      !loadingNewer &&
      !newerFailure &&
      loadState === 'ready'
    ) {
      void loadNewer()
    }
  }, [
    hasNewer,
    indexVirtualItems,
    listVirtualItems,
    loadNewer,
    loadState,
    loadingNewer,
    newerFailure,
    topLoaderVisible,
  ])

  useEffect(() => {
    const lastListItem = [...listVirtualItems].reverse()[0]
    const lastIndexItem = [...indexVirtualItems].reverse()[0]
    const tailIndex = Math.max(lastListItem?.index ?? -1, lastIndexItem?.index ?? -1)
    if (tailIndex < rowCount - 1) return
    if (!bottomLoaderVisible || !hasOlder || loadingOlder || olderFailure || loadState !== 'ready') return
    void loadOlder()
  }, [
    bottomLoaderVisible,
    hasOlder,
    indexVirtualItems,
    listVirtualItems,
    loadOlder,
    loadState,
    loadingOlder,
    olderFailure,
    rowCount,
  ])

  useEffect(() => {
    if (items.length === 0) return
    const scrollElement = listScrollRef.current
    if (!scrollElement) return
    const visibleRows = listVirtualItems
      .map((row) => ({ row, itemIndex: row.index - topLoaderOffset }))
      .filter(({ itemIndex }) => itemIndex >= 0 && itemIndex < items.length)
    if (visibleRows.length === 0) return
    const viewportCenter = scrollElement.scrollTop + scrollElement.clientHeight / 2
    let nearestIndex = visibleRows[0]?.itemIndex ?? 0
    let nearestDistance = Number.POSITIVE_INFINITY
    for (const { row, itemIndex } of visibleRows) {
      const rowCenter = row.start + row.size / 2
      const distance = Math.abs(rowCenter - viewportCenter)
      if (distance < nearestDistance) {
        nearestDistance = distance
        nearestIndex = itemIndex
      }
    }
    setSelectedIndex((prev) => (prev === nearestIndex ? prev : nearestIndex))
  }, [items.length, listVirtualItems, topLoaderOffset])

  useEffect(() => {
    if (!showDesktopIndex) return
    if (selectedIndex >= items.length) return
    indexVirtualizer.scrollToIndex(selectedIndex + topLoaderOffset, { align: 'auto' })
  }, [indexVirtualizer, items.length, selectedIndex, showDesktopIndex, topLoaderOffset])

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

  const showSettingsAction = releaseNotesShouldOfferSettingsAction(listResponse)
  const fallbackBanner = listResponse?.fallback
    ? { tone: 'warning' as const, message: listResponse.fallback.message }
    : null
  const anchorBanner =
    anchorState?.status === 'notFound' || anchorState?.status === 'outsideWindow' || anchorState?.status === 'unavailable'
      ? { tone: 'warning' as const, message: anchorState?.message ?? '' }
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
      const currentMatch = releaseNotesTagMatchesVersion(item, currentVersion)
      const candidateMatch = releaseNotesTagMatchesVersion(item, candidateComparableVersion)
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
      let updateActionVariant: 'primary' | 'ghost' = 'primary'
      let updateActionPresentation: 'default' | 'candidateOnly' = 'default'
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
          updateActionVariant = dockrevAction.buttonVariant
          updateActionPresentation = dockrevAction.presentation === 'candidateOnly' ? 'candidateOnly' : 'default'
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
      const { body, missing } = releaseNotesBodyForView(item, viewMode)
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
        updateActionVariant,
        updateActionPresentation,
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

  const renderedCardCount = listVirtualItems.filter((item) => {
    const cardIndex = item.index - topLoaderOffset
    return cardIndex >= 0 && cardIndex < cards.length
  }).length
  const renderedIndexCount = indexVirtualItems.filter((item) => {
    const itemIndex = item.index - topLoaderOffset
    return itemIndex >= 0 && itemIndex < items.length
  }).length
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
                    {releaseNotesViewLabel(view)}
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

        {loadState === 'loading' && items.length === 0 ? (
          <div className="serviceVersionsState" data-service-versions-state="loading">
            <span className="btnInlineSpinner" aria-hidden="true" />
            <span>正在加载版本发布记录…</span>
          </div>
        ) : null}

        {loadState === 'ready' && listResponse?.status !== 'ready' ? (
          <div className="serviceVersionsState serviceVersionsStateError" data-service-versions-state={listResponse?.status}>
            <div className="serviceVersionsStateTitle">无法读取版本发布记录</div>
            <div className="serviceVersionsStateMessage">{listResponse?.message ?? '请稍后重试。'}</div>
          </div>
        ) : null}

        {loadState === 'ready' && listResponse?.status === 'ready' && items.length === 0 ? (
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
                        const isTopLoaderRow = topLoaderVisible && virtualRow.index === 0
                        const itemIndex = virtualRow.index - topLoaderOffset
                        if (isTopLoaderRow || itemIndex >= items.length) {
                          const loaderMessage = isTopLoaderRow
                            ? newerFailure
                              ? '加载更新版本失败，请回到顶部重试。'
                              : loadingNewer
                                ? '正在继续加载更新版本…'
                                : hasNewer
                                  ? '继续上滑以加载更新版本…'
                                  : ''
                            : olderFailure
                              ? '加载更旧版本失败，请继续向下滚动重试。'
                              : loadingOlder
                                ? '正在继续加载更旧版本…'
                                : hasOlder
                                  ? '继续下滑以加载更旧版本…'
                                  : ''
                          return (
                            <div
                              key={virtualRow.key}
                              className="serviceVersionsIndexRow serviceVersionsIndexRowLoader"
                              data-index={virtualRow.index}
                            >
                              {loaderMessage ? (
                                <div className="serviceVersionsIndexLoader">{loaderMessage}</div>
                              ) : null}
                            </div>
                          )
                        }

                        const item = items[itemIndex]!
                        const currentMatch = releaseNotesTagMatchesVersion(item, currentVersion)
                        const candidateMatch = releaseNotesTagMatchesVersion(item, candidateVersion)
                        const selected = itemIndex === selectedIndex
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
                                setSelectedIndex(itemIndex)
                                void centerVersionCard(itemIndex, item.tagName, 'interactive')
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
                      const isTopLoaderRow = topLoaderVisible && virtualRow.index === 0
                      const cardIndex = virtualRow.index - topLoaderOffset
                      if (isTopLoaderRow || cardIndex >= cards.length) {
                        return (
                          <div
                            key={virtualRow.key}
                            data-index={virtualRow.index}
                            className="serviceVersionsVirtualRow serviceVersionsVirtualRowLoader"
                            ref={listVirtualizer.measureElement}
                          >
                            {isTopLoaderRow ? (
                              newerFailure ? (
                                <div className="serviceVersionsLoaderCard">
                                  <div className="serviceVersionsLoaderTitle">加载更新版本失败</div>
                                  <div className="serviceVersionsLoaderMessage">{newerFailure.message ?? '请稍后重试。'}</div>
                                  <div>
                                    <Button variant="ghost" onClick={() => void loadNewer()}>
                                      重试
                                    </Button>
                                  </div>
                                </div>
                              ) : hasNewer || loadingNewer ? (
                                <div className="serviceVersionsLoaderRow">
                                  <span className="btnInlineSpinner" aria-hidden="true" />
                                  <span>{loadingNewer ? '正在继续加载更新版本…' : '继续上滑以加载更新版本…'}</span>
                                </div>
                              ) : null
                            ) : olderFailure ? (
                              <div className="serviceVersionsLoaderCard">
                                <div className="serviceVersionsLoaderTitle">加载更旧版本失败</div>
                                <div className="serviceVersionsLoaderMessage">{olderFailure.message ?? '请稍后重试。'}</div>
                                <div>
                                  <Button variant="ghost" onClick={() => void loadOlder()}>
                                    重试
                                  </Button>
                                </div>
                              </div>
                            ) : hasOlder || loadingOlder ? (
                              <div className="serviceVersionsLoaderRow">
                                <span className="btnInlineSpinner" aria-hidden="true" />
                                <span>{loadingOlder ? '正在继续加载更旧版本…' : '继续下滑以加载更旧版本…'}</span>
                              </div>
                            ) : null}
                          </div>
                        )
                      }

                      const card = cards[cardIndex]!
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
                            viewLabel={releaseNotesViewLabel(viewMode)}
                            sourceLabel={releaseNotesSourceLabel(listResponse)}
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
