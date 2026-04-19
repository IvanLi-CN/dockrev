import type {
  CleanupApplyRequest,
  CleanupScanRequest,
  GitHubReleaseAuthMode,
  JobDetail,
  JobListItem,
  ServiceGitHubReleaseItem,
  ServiceGitHubReleaseLocateResponse,
  ServiceGitHubReleaseLocateStatus,
  ServiceGitHubReleasesResponse,
  ServiceGitHubReleasesStatus,
  ServiceGitHubRepoRef,
  ServiceRollbackTargetResponse,
  StackDetail,
} from '../../../api'
import { isDockrevImageRef } from '../../../runtimeConfig'
import { serviceRowStatus } from '../../../updateStatus'
import {
  buildCleanupMockScanResponse,
  isCleanupMockScenario,
  resolveCleanupMockApply,
  type CleanupMockRuntimeState,
} from '../cleanupMockData'
import type { MockRouteContext } from './context'
import { buildMockDiscoveryTimeline as buildMockDiscoveryTimelineResponse } from './discoveryTimeline'
import { buildFixture } from './fixturesMisc'
import { handleGhcrRoutes } from './handlers/ghcr'
import { handleServiceStateRoutes } from './handlers/serviceState'
import { applyRollbackTargetRaceAfterUpdate, maybeServeRollbackTargetRaceResponse, type RollbackTargetRaceState } from './rollbackRace'
import type {
  DockrevApiScenario,
  DockrevMockApiOptions,
  DockrevMockGitHubReleasesDataset,
} from './shared'
import {
  MockEventSource,
  getBoolean,
  getString,
  hashString,
  isRecord,
  json,
  makeMockDebug,
  nowIso,
  offsetMockVersion,
  parseJsonBody,
  realFetch,
  summarizeVersionInferenceRows,
  type VersionInferenceOverviewMock,
} from './shared'

export function installDockrevMockApi(
  scenario: DockrevApiScenario,
  options: DockrevMockApiOptions = {},
) {
  const state = scenario === 'error' ? null : buildFixture(scenario)
  if (state) {
    for (const stack of Object.values(state.stackById)) {
      stack.services = stack.services.map((service) => {
        const override = options.serviceOverridesById?.[service.id]
        if (!override) return service
        const nextService = { ...service, ...override }
        state.serviceSettingsById[service.id] = nextService.settings
        return nextService
      })
    }
  }
  const ignoreSeqRef = { value: 0 }
  const jobSeqRef = { value: 0 }
  const digestSnapshotPendingAttempts = new Map<string, number>()
  const forcedDigestSnapshotPendingAttempts = new Map<string, number>()
  const jobsEventsSeqRef = { value: 4_000 }
  const queueProgressDemoSteps = [40, 44, 48, 52, 56, 60, 65, 70, 75, 80, 85, 90, 94, 97]
  let queueProgressDemoStep = 0
  let queueProgressDemoDirection = 1
  const cleanupRuntime: CleanupMockRuntimeState = {
    nextJobSeq: 0,
    staleApplyConsumed: false,
  }
  const rollbackTargetRaceByServiceId = new Map<string, RollbackTargetRaceState>()

  const advanceQueueProgressDemo = (): number | null => {
    if (!state || scenario !== 'queue-progress-smoothing') return null
    if (queueProgressDemoStep >= queueProgressDemoSteps.length - 1) queueProgressDemoDirection = -1
    if (queueProgressDemoStep <= 0) queueProgressDemoDirection = 1
    queueProgressDemoStep += queueProgressDemoDirection
    const completedPercent = queueProgressDemoSteps[queueProgressDemoStep]
    const plannedPercent = Math.min(100, completedPercent + 16)
    const updatedAt = nowIso()

    const patchProgress = (job: JobListItem | JobDetail | undefined) => {
      if (!job || job.id !== 'job-running' || !job.progress) return
      job.progress = {
        ...job.progress,
        phase: 'pulling',
        message: 'updating images',
        total: 100,
        plannedTotal: 100,
        current: completedPercent,
        percent: completedPercent,
        plannedCurrent: plannedPercent,
        plannedPercent,
        updatedAt,
      }
    }

    for (const job of state.jobs) patchProgress(job)
    patchProgress(state.jobById['job-running'])

    return completedPercent
  }

  globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug()
  if (typeof window !== 'undefined') {
    globalThis.EventSource = MockEventSource as unknown as typeof EventSource
  }
  MockEventSource.pollIntervalMs = scenario === 'queue-progress-smoothing' ? 700 : 4_000

  function findService(serviceId: string) {
    if (!state) return null
    for (const st of Object.values(state.stackById)) {
      const svc = st.services.find((s) => s.id === serviceId)
      if (svc) return { stack: st, svc }
    }
    return null
  }

  function normalizeDigestValue(value: string | null | undefined): string {
    const trimmed = (value ?? '').trim()
    if (!trimmed) return ''
    return trimmed.includes(':') ? trimmed : `sha256:${trimmed}`
  }

  function buildMockDigestTagData(
    serviceId: string,
    imageTag: string,
    digestNorm: string,
    refreshed: boolean,
  ): { repoTags: string[]; tags: string[] } {
    const isVersionTagsDemoScenario =
      scenario === 'version-tags-popover-demo' ||
      scenario === 'version-tags-popover-same-digest' ||
      scenario === 'version-tags-popover-snapshot-pending'
    const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

    const repoTags =
      serviceId === 'svc-prod-api'
        ? ['5.2.1', '5.2.3', '5.2.4', '5.3.0', 'v5.2.1', 'v5.2.3', 'stable', 'latest']
        : serviceId === 'svc-prod-web'
          ? (() => {
              const out: string[] = ['5.1', '5.1.10', '5.1.11', '5.1.12', '5.2', 'v5.2.1', 'stable', 'latest']
              for (let i = 0; i < 40; i++) out.push(`5.2.${i}`)
              return out
            })()
          : serviceId === 'svc-resolved-web'
            ? (() => {
                const out: string[] = ['5.1', '5.1.10', '5.1.11', '5.1.12', '5.2', 'v5.2.1', 'v5.2.3', 'stable', 'latest']
                for (let i = 0; i < 40; i++) out.push(`5.2.${i}`)
                return out
              })()
            : isVersionTagsDemoScenario && serviceId === 'svc-version-tags'
              ? ['v0.8.9-arm64', 'v0.8.8-arm64', 'v0.8.8', 'v0.8.7', '0.8.8', '0.8.7', 'stable', 'latest']
              : digestNorm === `sha256:${'a'.repeat(64)}`
                ? ['v0.1.8', '0.1.8']
                : [imageTag]

    const tags = !digestNorm
      ? []
      : serviceId === 'svc-version-tags' && isVersionTagsDemoScenario && digestNorm === d('a', 'b1')
        ? ['v0.8.7', '0.8.7', 'stable', 'latest']
        : serviceId === 'svc-version-tags' && isVersionTagsDemoScenario && digestNorm === d('b', '9f')
          ? refreshed
            ? ['v0.8.8', 'v0.8.8-arm64', '0.8.8', 'stable', 'latest']
            : ['v0.8.8-arm64', 'v0.8.8', '0.8.8', 'stable', 'latest']
          : digestNorm === d('c', 'c2')
            ? ['v5.2.1', '5.2.1', '5.2', 'stable', 'latest']
            : digestNorm === d('a', 'b1') && serviceId === 'svc-resolved-web'
              ? ['5.2.1', 'v5.2.1', 'stable', 'latest']
              : digestNorm === d('b', '9f') && serviceId === 'svc-resolved-web'
                ? ['5.2.3', 'v5.2.3']
                : digestNorm === d('a', 'b1')
                  ? ['5.2.1', 'v5.2.1']
                  : digestNorm === d('b', '9f') && serviceId === 'svc-prod-api'
                    ? ['5.2.3', 'v5.2.3', 'stable', 'latest']
                    : digestNorm === `sha256:${'a'.repeat(64)}`
                      ? ['v0.1.8', '0.1.8']
                      : [imageTag]

    return { repoTags, tags }
  }

  function parseMockGitHubRepoRef(input: string | null | undefined): ServiceGitHubRepoRef | null {
    const trimmed = (input ?? '').trim()
    if (!trimmed) return null
    const match = trimmed.match(/^https?:\/\/(?:www\.)?github\.com\/([^/]+)\/([^/?#]+?)(?:\.git)?(?:[/?#].*)?$/i)
    if (!match) return null
    const owner = match[1]?.trim()
    const repo = match[2]?.trim()
    if (!owner || !repo) return null
    return {
      fullName: `${owner}/${repo}`,
      htmlUrl: `https://github.com/${owner}/${repo}`,
    }
  }

  function mockGitHubReleaseErrorMessage(
    status: ServiceGitHubReleasesStatus,
    authMode: GitHubReleaseAuthMode,
  ): string | null {
    if (status === 'ready') return null
    if (status === 'unsupportedRepo') return '该服务尚未配置 GitHub repoUrl，当前只支持 GitHub Releases。'
    if (status === 'permissionDenied') {
      return authMode === 'pat'
        ? '当前 GitHub PAT 无法访问该仓库的 Releases，请检查权限范围或仓库可见性。'
        : '匿名请求无法访问该仓库的 Releases，请前往“设置 -> GitHub Packages”配置 PAT 后重试。'
    }
    if (status === 'rateLimited') {
      return authMode === 'pat'
        ? 'GitHub API 请求已达到速率限制，请稍后再试。'
        : '匿名请求已命中 GitHub API 速率限制，请前往“设置 -> GitHub Packages”配置 PAT 后重试。'
    }
    return '读取 GitHub Releases 失败，请稍后重试。'
  }

  function mapListStatusToLocateStatus(
    status: ServiceGitHubReleasesStatus,
  ): ServiceGitHubReleaseLocateStatus {
    if (status === 'unsupportedRepo') return 'unsupportedRepo'
    if (status === 'permissionDenied') return 'permissionDenied'
    if (status === 'rateLimited') return 'rateLimited'
    return 'upstreamError'
  }

  function mockGitHubReleaseLocateMessage(
    status: ServiceGitHubReleaseLocateStatus,
    authMode: GitHubReleaseAuthMode,
    version: string,
    searchedCount: number,
  ): string | null {
    if (status === 'found') return null
    if (status === 'outsideWindow') return `已定位到 ${version}，但它不在前 ${searchedCount} 条发布记录内。`
    if (status === 'notFound') return `在前 ${searchedCount} 条发布记录中未找到 ${version}。`
    return mockGitHubReleaseErrorMessage(
      status === 'unsupportedRepo'
        ? 'unsupportedRepo'
        : status === 'permissionDenied'
          ? 'permissionDenied'
          : status === 'rateLimited'
            ? 'rateLimited'
            : 'upstreamError',
      authMode,
    )
  }

  function mockGitHubReleaseTagVariants(version: string): string[] {
    const trimmed = version.trim()
    if (!trimmed) return []
    const set = new Set<string>()
    set.add(trimmed)
    if (trimmed.startsWith('v') && trimmed.length > 1) set.add(trimmed.slice(1))
    else set.add(`v${trimmed}`)
    return [...set]
  }

  function mockGitHubReleaseMatchesVersion(item: ServiceGitHubReleaseItem, version: string): boolean {
    const variants = mockGitHubReleaseTagVariants(version).map((value) => value.toLowerCase())
    return variants.includes(item.tagName.trim().toLowerCase())
  }

  function buildDefaultMockGitHubReleaseItems(serviceId: string): ServiceGitHubReleaseItem[] {
    const found = findService(serviceId)
    if (!found) return []
    const runningVersion =
      found.svc.image.resolvedTag?.trim() ||
      found.svc.image.tag?.trim() ||
      '1.0.0'
    const candidateVersion =
      found.svc.candidate?.resolvedTag?.trim() ||
      found.svc.candidate?.tag?.trim() ||
      offsetMockVersion(runningVersion, 1, '1.0.1')
    const versions = Array.from(
      new Set([
        candidateVersion,
        offsetMockVersion(candidateVersion, -1, runningVersion),
        offsetMockVersion(candidateVersion, -2, runningVersion),
        runningVersion,
      ]),
    )
    return versions.map((tagName, index) => ({
      id: 10_000 + index + hashString(`${serviceId}:${tagName}`),
      tagName,
      name: tagName,
      body:
        index === 0
          ? `Release notes for ${tagName}\\n\\n- Improve update visibility\\n- Keep discovery timeline linked to releases`
          : `Release notes for ${tagName}`,
      htmlUrl: `https://github.com/${(parseMockGitHubRepoRef(found.svc.settings.repoUrl)?.fullName ?? 'acme/example')}/releases/tag/${encodeURIComponent(tagName)}`,
      draft: false,
      prerelease: index > 0 && tagName.includes('rc'),
      publishedAt: nowIso(-(index + 1) * 36 * 60 * 1000),
      createdAt: nowIso(-(index + 1) * 40 * 60 * 1000),
    }))
  }

  function buildMockGitHubReleasesDataset(serviceId: string): DockrevMockGitHubReleasesDataset {
    const explicit = options.githubReleasesByServiceId?.[serviceId]
    if (explicit) {
      return {
        authMode: explicit.authMode ?? 'anonymous',
        repo: explicit.repo ?? null,
        listStatus: explicit.listStatus ?? 'ready',
        listMessage: explicit.listMessage ?? null,
        items: explicit.items?.map((item) => ({ ...item })) ?? [],
        locateByVersion: explicit.locateByVersion,
      }
    }

    const found = findService(serviceId)
    const repo = parseMockGitHubRepoRef(found?.svc.settings.repoUrl)
    if (!repo) {
      return {
        authMode: 'anonymous',
        repo: null,
        listStatus: 'unsupportedRepo',
        listMessage: mockGitHubReleaseErrorMessage('unsupportedRepo', 'anonymous'),
        items: [],
      }
    }

    return {
      authMode: 'anonymous',
      repo,
      listStatus: 'ready',
      listMessage: null,
      items: buildDefaultMockGitHubReleaseItems(serviceId),
    }
  }

  function buildMockGitHubReleasesResponse(
    serviceId: string,
    page: number,
    perPage: number,
  ): ServiceGitHubReleasesResponse {
    const dataset = buildMockGitHubReleasesDataset(serviceId)
    const status = dataset.listStatus ?? 'ready'
    const authMode = dataset.authMode ?? 'anonymous'
    const items = dataset.items ?? []
    if (status !== 'ready') {
      return {
        status,
        authMode,
        repo: dataset.repo ?? null,
        page,
        perPage,
        hasMore: false,
        items: [],
        message: dataset.listMessage ?? mockGitHubReleaseErrorMessage(status, authMode),
      }
    }

    const offset = (page - 1) * perPage
    const paged = items.slice(offset, offset + perPage)
    return {
      status: 'ready',
      authMode,
      repo: dataset.repo ?? null,
      page,
      perPage,
      hasMore: offset + perPage < items.length,
      items: paged.map((item) => ({ ...item })),
      message: dataset.listMessage ?? null,
    }
  }

  function buildMockGitHubReleaseLocateResponse(
    serviceId: string,
    version: string,
    perPage: number,
    limit: number,
  ): ServiceGitHubReleaseLocateResponse {
    const dataset = buildMockGitHubReleasesDataset(serviceId)
    const authMode = dataset.authMode ?? 'anonymous'
    const trimmedVersion = version.trim()
    const overrideEntry = Object.entries(dataset.locateByVersion ?? {}).find(
      ([key]) => key.trim().toLowerCase() === trimmedVersion.toLowerCase(),
    )
    if (overrideEntry) {
      const override = overrideEntry[1]
      const status = override.status ?? 'notFound'
      const searchedCount = override.searchedCount ?? Math.min(limit, dataset.items?.length ?? 0)
      return {
        status,
        authMode: override.authMode ?? authMode,
        repo: override.repo ?? dataset.repo ?? null,
        version: trimmedVersion,
        searchedCount,
        matchedTag: override.matchedTag ?? null,
        page: override.page ?? null,
        indexWithinPage: override.indexWithinPage ?? null,
        absoluteIndex: override.absoluteIndex ?? null,
        message:
          override.message ??
          mockGitHubReleaseLocateMessage(status, override.authMode ?? authMode, trimmedVersion, searchedCount),
      }
    }

    const listStatus = dataset.listStatus ?? 'ready'
    if (listStatus !== 'ready') {
      const status = mapListStatusToLocateStatus(listStatus)
      return {
        status,
        authMode,
        repo: dataset.repo ?? null,
        version: trimmedVersion,
        searchedCount: 0,
        matchedTag: null,
        page: null,
        indexWithinPage: null,
        absoluteIndex: null,
        message: mockGitHubReleaseLocateMessage(status, authMode, trimmedVersion, 0),
      }
    }

    const items = dataset.items ?? []
    const matchIndex = items.findIndex((item) => mockGitHubReleaseMatchesVersion(item, trimmedVersion))
    const searchedCount = Math.min(limit, items.length)

    if (matchIndex >= 0 && matchIndex < limit) {
      const page = Math.floor(matchIndex / perPage) + 1
      const indexWithinPage = matchIndex % perPage
      const scannedCount = Math.min(limit, Math.min(items.length, page * perPage))
      return {
        status: 'found',
        authMode,
        repo: dataset.repo ?? null,
        version: trimmedVersion,
        searchedCount: scannedCount,
        matchedTag: items[matchIndex]?.tagName ?? trimmedVersion,
        page,
        indexWithinPage,
        absoluteIndex: matchIndex,
        message: null,
      }
    }

    if (matchIndex >= limit) {
      const matchedTag = items[matchIndex]?.tagName ?? mockGitHubReleaseTagVariants(trimmedVersion)[0] ?? trimmedVersion
      return {
        status: 'outsideWindow',
        authMode,
        repo: dataset.repo ?? null,
        version: trimmedVersion,
        searchedCount,
        matchedTag,
        page: null,
        indexWithinPage: null,
        absoluteIndex: null,
        message: mockGitHubReleaseLocateMessage('outsideWindow', authMode, trimmedVersion, searchedCount),
      }
    }

    return {
      status: 'notFound',
      authMode,
      repo: dataset.repo ?? null,
      version: trimmedVersion,
      searchedCount,
      matchedTag: null,
      page: null,
      indexWithinPage: null,
      absoluteIndex: null,
      message: mockGitHubReleaseLocateMessage('notFound', authMode, trimmedVersion, searchedCount),
    }
  }

  function canApplyMockUpdate(service: StackDetail['services'][number]) {
    if (service.archived || isDockrevImageRef(service.image.ref)) return false
    const status = serviceRowStatus(service)
    return status === 'updatable' || status === 'hint'
  }

  function countMockUpdates(stack: StackDetail) {
    return stack.services.filter((service) => canApplyMockUpdate(service)).length
  }

  function syncStackListItem(stackId: string) {
    if (!state) return
    const detail = state.stackById[stackId]
    if (!detail) return
    const item = state.stacks.find((stack) => stack.id === stackId)
    if (!item) return
    item.updates = countMockUpdates(detail)
  }

  function selectUpdateServiceIds(scope: string, stackId: string | null, serviceId: string | null) {
    if (!state) return []
    if (scope === 'service') return serviceId ? [serviceId] : []
    if (scope === 'stack') {
      const stack = stackId ? state.stackById[stackId] : null
      return stack ? stack.services.filter((service) => canApplyMockUpdate(service)).map((service) => service.id) : []
    }
    return Object.values(state.stackById).flatMap((stack) =>
      stack.services.filter((service) => canApplyMockUpdate(service)).map((service) => service.id),
    )
  }

  function applyMockUpdateSettlement(
    serviceId: string,
    targetTag: string,
    targetDigest: string,
    pullTags: string[],
  ) {
    const found = findService(serviceId)
    if (!found || !found.svc.candidate) return

    const candidate = found.svc.candidate
    const previousDigest = found.svc.image.digest ?? ''
    const previousDisplayTag = found.svc.image.resolvedTag?.trim() || found.svc.image.tag?.trim() || null
    const nextTag = targetTag.trim()
    const nextDigest = targetDigest.trim()
    const nextResolvedTag = candidate.resolvedTag?.trim() || nextTag
    const normalizedPullTags = pullTags.map((tag) => tag.trim()).filter(Boolean)

    found.svc.image = {
      ...found.svc.image,
      tag: nextTag,
      digest: nextDigest,
      resolvedTag: nextResolvedTag,
      resolvedTags: Array.from(
        new Set([nextResolvedTag, ...normalizedPullTags, ...(found.svc.image.resolvedTags ?? [])].filter(Boolean)),
      ),
    }
    found.svc.candidate = null
    if (found.svc.versionInference) {
      found.svc.versionInference = {
        ...found.svc.versionInference,
        status: 'ready',
        checkedAt: nowIso(),
      }
    }
    syncStackListItem(found.stack.id)

    applyRollbackTargetRaceAfterUpdate({ rollbackTargets: state!.rollbackTargetByServiceId, raceByServiceId: rollbackTargetRaceByServiceId, scenario, serviceId, nextTag, nextDigest, nextResolvedTag, previousDigest, previousDisplayTag })
  }

  globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
    const method = (init?.method ?? (input instanceof Request ? input.method : 'GET')).toUpperCase()
    const urlString = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url
    const url = (() => {
      try {
        const baseHref = typeof window !== 'undefined' ? window.location.href : 'http://localhost'
        return new URL(urlString, baseHref)
      } catch {
        return null
      }
    })()
    const urlPath = url ? url.pathname : urlString
    const urlPathWithQuery = url ? `${url.pathname}${url.search}` : urlString

    if (
      scenario === 'settings-configured-load-slow' &&
      method === 'GET' &&
      (urlPath === '/api/settings' ||
        urlPath === '/api/notifications' ||
        urlPath === '/api/github-packages/settings')
    ) {
      await new Promise<void>((resolve) => {
        globalThis.setTimeout(() => resolve(), 550)
      })
    }

    if (urlPath === '/supervisor/health' && method === 'GET') {
      return json({ ok: true })
    }
    if (urlPath === '/supervisor/version' && method === 'GET') {
      // Use an existing repo tag so the version link in UI can be exercised in Storybook.
      return json({ version: '0.5.0' })
    }
    if (urlPath === '/supervisor/self-upgrade' && method === 'GET') {
      return json({
        state: 'idle',
        opId: 'sup_mock',
        target: { image: 'ghcr.io/ivanli-cn/dockrev', tag: '0.5.0', digest: null },
        previous: { tag: '0.0.0', digest: null },
        startedAt: nowIso(-60_000),
        updatedAt: nowIso(-30_000),
        progress: { step: 'done', message: 'idle' },
        logs: [],
      })
    }

    if (!urlPath.startsWith('/api/')) return realFetch(input, init)

    if (scenario === 'error') {
      return json({ error: 'mock error' }, { status: 500 })
    }

    if (!state) return json({ error: 'mock not initialized' }, { status: 500 })
    const f = state
    const routeCtx: MockRouteContext = {
      scenario,
      state: f,
      method,
      init,
      url,
      urlPath,
      urlPathWithQuery,
      urlString,
      json,
      parseJsonBody,
      getString,
      getBoolean,
      isRecord,
      nowIso,
      makeMockDebug,
      findService,
      normalizeDigestValue,
      buildMockDigestTagData,
      buildMockDiscoveryTimeline: (serviceId) =>
        buildMockDiscoveryTimelineResponse(serviceId, options, findService),
      buildMockGitHubReleasesResponse,
      buildMockGitHubReleaseLocateResponse,
      applyMockUpdateSettlement,
      selectUpdateServiceIds,
      syncStackListItem,
      advanceQueueProgressDemo,
      ignoreSeqRef,
      jobSeqRef,
      jobsEventsSeqRef,
      digestSnapshotPendingAttempts,
      forcedDigestSnapshotPendingAttempts,
      cleanupRuntime,
    }

    const ghcrResponse = await handleGhcrRoutes(routeCtx)
    if (ghcrResponse) return ghcrResponse

    if (urlPath === '/api/version' && method === 'GET') {
      // Use an existing repo tag so the version link in UI can be exercised in Storybook.
      return json({ version: '0.5.0' })
    }

    if (urlPath === '/api/version-inference/overview' && method === 'GET') {
      const params = url?.searchParams ?? new URLSearchParams()
      const page = Math.max(1, Number(params.get('page') ?? '1') || 1)
      const perPage = Math.min(200, Math.max(1, Number(params.get('perPage') ?? '50') || 50))
      const q = (params.get('q') ?? '').trim().toLowerCase()
      const status = (params.get('status') ?? '').trim().toLowerCase()
      const validStatus = new Set(['', 'all', 'queued', 'running', 'ready', 'stale', 'all_failed'])
      if (!validStatus.has(status)) return json({ error: 'invalid status filter' }, { status: 400 })

      const summary = summarizeVersionInferenceRows(f.versionInferenceOverview.rows)
      const rows = f.versionInferenceOverview.rows.filter((row) => {
        if (status && status !== 'all' && row.status.toLowerCase() !== status) return false
        if (!q) return true
        const haystack = `${row.imageRepo} ${row.hostPlatform} ${row.key}`.toLowerCase()
        return haystack.includes(q)
      })
      const offset = (page - 1) * perPage
      const pagedRows = rows.slice(offset, offset + perPage)

      return json({
        worker: f.versionInferenceOverview.worker,
        gc: f.versionInferenceOverview.gc,
        summary,
        tasks: f.versionInferenceOverview.tasks,
        rows: pagedRows,
        page,
        perPage,
        total: rows.length,
      } satisfies VersionInferenceOverviewMock)
    }

    if (urlPath === '/api/version-inference/events' && method === 'GET') {
      const params = url?.searchParams ?? new URLSearchParams()
      const afterId = Number(params.get('afterId') ?? '0') || 0
      const events = f.versionInferenceEvents.filter((evt) => evt.id > afterId).slice(0, 200)
      const body = events
        .map((evt) => `id: ${evt.id}\nevent: version_inference_event\ndata: ${JSON.stringify(evt.data)}\n\n`)
        .join('')
      return new Response(body || ': keep-alive\n\n', {
        status: 200,
        headers: {
          'Content-Type': 'text/event-stream',
          'Cache-Control': 'no-cache',
          'x-accel-buffering': 'no',
        },
      })
    }

    if (urlPath === '/api/jobs/events' && method === 'GET' && scenario === 'queue-progress-smoothing') {
      const params = url?.searchParams ?? new URLSearchParams()
      const afterId = Number(params.get('afterId') ?? '0') || 0
      const events: Array<{ id: number; data: Record<string, unknown> }> = []
      const nextCompletedPercent = advanceQueueProgressDemo()
      if (nextCompletedPercent !== null) {
        jobsEventsSeqRef.value += 1
        events.push({
          id: jobsEventsSeqRef.value,
          data: {
            type: 'job_progress',
            jobId: 'job-running',
            percent: nextCompletedPercent,
            ts: nowIso(),
          },
        })
      }
      const newEvents = events.filter((evt) => evt.id > afterId).slice(0, 200)
      const body = newEvents
        .map((evt) => `id: ${evt.id}\nevent: job_event\ndata: ${JSON.stringify(evt.data)}\n\n`)
        .join('')
      return new Response(body || ': keep-alive\n\n', {
        status: 200,
        headers: {
          'Content-Type': 'text/event-stream',
          'Cache-Control': 'no-cache',
          'x-accel-buffering': 'no',
        },
      })
    }

    if (isCleanupMockScenario(scenario) && method === 'POST' && urlPath === '/api/cleanups/scan') {
      if (scenario === 'cleanup-console-scan-pending') {
        return new Promise<Response>(() => {})
      }
      if (scenario === 'cleanup-console-scan-slow') {
        await new Promise<void>((resolve) => {
          globalThis.setTimeout(() => resolve(), 1600)
        })
      }
      const parsed = parseJsonBody(init?.body) as CleanupScanRequest | null
      const request: CleanupScanRequest = {
        reason: parsed?.reason === 'confirm' ? 'confirm' : 'page',
        preset:
          parsed?.preset === 'conservative' ||
          parsed?.preset === 'balanced' ||
          parsed?.preset === 'project_deep_clean' ||
          parsed?.preset === 'aggressive'
            ? parsed.preset
            : 'balanced',
        scope: parsed?.scope === 'stack' || parsed?.scope === 'service' ? parsed.scope : 'all',
        stackId: typeof parsed?.stackId === 'string' ? parsed.stackId : undefined,
        serviceId: typeof parsed?.serviceId === 'string' ? parsed.serviceId : undefined,
      }
      return json(buildCleanupMockScanResponse(scenario, request))
    }

    if (isCleanupMockScenario(scenario) && method === 'POST' && urlPath === '/api/cleanups/apply') {
      if (scenario === 'cleanup-console-apply-slow') {
        return new Promise<Response>(() => {})
      }
      const parsed = parseJsonBody(init?.body) as CleanupApplyRequest | null
      if (!parsed) {
        return json({ error: { code: 'invalid_argument', message: 'invalid cleanup apply payload', details: null } }, { status: 400 })
      }
      const result = resolveCleanupMockApply(scenario, parsed, cleanupRuntime)
      if (!result.ok) return json(result.body, { status: result.status })

      const createdAt = nowIso(-400)
      const jobId = result.jobId
      const job: JobListItem = {
        id: jobId,
        type: 'cleanup_apply',
        scope: parsed.scope,
        stackId: parsed.stackId ?? null,
        serviceId: parsed.serviceId ?? null,
        status: 'running',
        createdBy: 'ivan',
        reason: 'ui',
        createdAt,
        startedAt: nowIso(-200),
        finishedAt: null,
        allowArchMismatch: false,
        backupMode: 'inherit',
        summary: {
          preset: parsed.preset,
          scope: parsed.scope,
          reclaimedBytesEstimated: 0,
          deletedCountsByKind: {},
          skippedInUse: [],
          groupedTargets: [],
        },
      }
      f.jobs = [job, ...f.jobs]
      f.jobById[jobId] = {
        ...job,
        logs: [
          { ts: createdAt, level: 'info', msg: 'cleanup confirm accepted' },
          { ts: nowIso(-100), level: 'info', msg: 'cleanup job queued by UI mock' },
        ],
        logsLastId: 2,
      }
      return json({ jobId })
    }

    // stacks
    if (method === 'GET' && (urlPathWithQuery === '/api/stacks' || urlPathWithQuery.startsWith('/api/stacks?'))) {
      const query = url?.search ? url.search.slice(1) : urlPathWithQuery.includes('?') ? urlPathWithQuery.split('?')[1] : ''
      const params = new URLSearchParams(query)
      const archived = params.get('archived') ?? 'exclude'

      let stacks = f.stacks
      if (archived === 'only') stacks = stacks.filter((s) => Boolean(s.archived))
      if (archived === 'exclude') stacks = stacks.filter((s) => !s.archived)

      return json({ stacks })
    }
    if (method === 'GET' && urlPath.startsWith('/api/stacks/')) {
      const id = decodeURIComponent(urlPath.split('/').slice(3).join('/'))
      const st = f.stackById[id]
      if (!st) return json({ error: 'not found' }, { status: 404 })
      return json({ stack: st })
    }
    if (method === 'POST' && urlPath.startsWith('/api/stacks/') && urlPath.endsWith('/archive')) {
      const id = decodeURIComponent(urlPath.split('/').slice(3, -1).join('/'))
      const item = f.stacks.find((s) => s.id === id)
      if (item) item.archived = true
      if (item) item.archivedServices = f.stackById[id]?.services.filter((s) => Boolean(s.archived)).length ?? 0
      if (f.stackById[id]) f.stackById[id].archived = true
      return json({}, { status: 204 })
    }
    if (method === 'POST' && urlPath.startsWith('/api/stacks/') && urlPath.endsWith('/restore')) {
      const id = decodeURIComponent(urlPath.split('/').slice(3, -1).join('/'))
      const item = f.stacks.find((s) => s.id === id)
      if (item) item.archived = false
      if (f.stackById[id]) f.stackById[id].archived = false
      return json({}, { status: 204 })
    }

    // checks / updates
    if (method === 'POST' && urlPath === '/api/checks') return json({ checkId: `check-${Math.random().toString(16).slice(2)}` })
    if (method === 'POST' && urlPath === '/api/updates') {
      const body = typeof init?.body === 'string' ? init.body : ''
      const parsed = body ? (JSON.parse(body) as Record<string, unknown>) : {}
      const dbg = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
      dbg.lastUpdateRequest = parsed
      dbg.lastUpdateUrl = urlPath
      dbg.lastUpdateMethod = method
      const stackId = typeof parsed.stackId === 'string' ? parsed.stackId : null
      const serviceId = typeof parsed.serviceId === 'string' ? parsed.serviceId : null
      const scope = typeof parsed.scope === 'string' ? parsed.scope : 'service'
      const mode = typeof parsed.mode === 'string' ? parsed.mode : 'dry-run'
      const targetTag = typeof parsed.targetTag === 'string' ? parsed.targetTag.trim() : ''
      const targetDigest = typeof parsed.targetDigest === 'string' ? parsed.targetDigest.trim() : ''
      const pullTags = Array.isArray(parsed.pullTags)
        ? parsed.pullTags.map((tag) => (typeof tag === 'string' ? tag.trim() : '')).filter(Boolean)
        : null
      const targets = Array.isArray(parsed.targets)
        ? parsed.targets.map((item) => (item && typeof item === 'object' ? (item as Record<string, unknown>) : null))
        : null

      if (scope === 'service' && !serviceId) {
        return json(
          { error: { code: 'invalid_argument', message: 'serviceId is required for scope=service' } },
          { status: 400 },
        )
      }
      if (scope === 'stack' && !stackId) {
        return json(
          { error: { code: 'invalid_argument', message: 'stackId is required for scope=stack' } },
          { status: 400 },
        )
      }

      const affectedServiceIds = selectUpdateServiceIds(scope, stackId, serviceId)
      const targetsByService = new Map<string, { targetTag: string; targetDigest: string; pullTags: string[] }>()
      if (scope === 'service') {
        if (!targetTag || !targetDigest || pullTags == null) {
          return json(
            { error: { code: 'invalid_argument', message: 'targetTag/targetDigest/pullTags is required for scope=service' } },
            { status: 400 },
          )
        }
        targetsByService.set(serviceId!, { targetTag, targetDigest, pullTags })
      } else {
        if ('targetTag' in parsed || 'targetDigest' in parsed || 'pullTags' in parsed) {
          return json(
            { error: { code: 'invalid_argument', message: 'targetTag/targetDigest/pullTags is only supported for scope=service' } },
            { status: 400 },
          )
        }
        if (targets == null) {
          return json(
            { error: { code: 'invalid_argument', message: 'targets is required for scope=stack/all' } },
            { status: 400 },
          )
        }
        for (const item of targets) {
          const nextServiceId = typeof item?.serviceId === 'string' ? item.serviceId.trim() : ''
          const nextTargetTag = typeof item?.targetTag === 'string' ? item.targetTag.trim() : ''
          const nextTargetDigest = typeof item?.targetDigest === 'string' ? item.targetDigest.trim() : ''
          const nextPullTags = Array.isArray(item?.pullTags)
            ? item.pullTags.map((tag) => (typeof tag === 'string' ? tag.trim() : '')).filter(Boolean)
            : null
          if (!nextServiceId || !nextTargetTag || !nextTargetDigest || nextPullTags == null) {
            return json(
              { error: { code: 'invalid_argument', message: 'targets[*] must include serviceId/targetTag/targetDigest/pullTags' } },
              { status: 400 },
            )
          }
          if (targetsByService.has(nextServiceId)) {
            return json(
              { error: { code: 'invalid_argument', message: 'targets contains duplicate serviceId' } },
              { status: 400 },
            )
          }
          targetsByService.set(nextServiceId, {
            targetTag: nextTargetTag,
            targetDigest: nextTargetDigest,
            pullTags: nextPullTags,
          })
        }
        const missingServiceIds = affectedServiceIds.filter((id) => !targetsByService.has(id))
        const extraServiceIds = [...targetsByService.keys()].filter((id) => !affectedServiceIds.includes(id))
        if (missingServiceIds.length > 0 || extraServiceIds.length > 0) {
          return json(
            {
              error: {
                code: 'invalid_argument',
                message: 'targets must exactly cover the selected services for this scope',
                details: { missingServiceIds, extraServiceIds },
              },
            },
            { status: 400 },
          )
        }
      }

      jobSeqRef.value += 1
      const jobId = `job-ui-${jobSeqRef.value}`
      const job: JobListItem = {
        id: jobId,
        type: 'update',
        scope,
        stackId: stackId ?? undefined,
        serviceId: serviceId ?? undefined,
        status: 'running',
        createdBy: 'ivan',
        reason: 'ui',
        createdAt: nowIso(-2_000),
        startedAt: nowIso(-1_000),
        finishedAt: null,
        allowArchMismatch: Boolean(parsed.allowArchMismatch),
        backupMode: typeof parsed.backupMode === 'string' ? parsed.backupMode : 'inherit',
        summary: {},
      }
      f.jobs = [job, ...f.jobs]
      f.jobById[jobId] = {
        ...job,
        logs: [
          { ts: nowIso(-900), level: 'info', msg: 'Queued by UI.' },
          { ts: nowIso(-300), level: 'info', msg: mode === 'apply' ? 'Apply started...' : 'Dry run started...' },
        ],
        logsLastId: 2,
      }
      const updateFinishDelayMs = scenario === 'dashboard-demo-slow-update' ? 4_500 : 1_400
      const settleDelayMs = scenario === 'dashboard-demo-slow-update' ? 280 : 220
      window.setTimeout(() => {
        const live = f.jobById[jobId]
        if (!live || (live.status !== 'queued' && live.status !== 'running')) return
        const finishedAt = nowIso()
        const nextLogs = [...live.logs, { ts: finishedAt, level: 'info', msg: 'Mock job finished.' }]
        const finalJob: JobDetail = {
          ...live,
          status: 'success',
          finishedAt,
          logs: nextLogs,
          logsLastId: nextLogs.length,
        }
        f.jobById[jobId] = finalJob
        f.jobs = f.jobs.map((row) => (row.id === jobId ? { ...row, status: 'success', finishedAt } : row))

        window.setTimeout(() => {
          if (!state) return
          for (const affectedId of affectedServiceIds) {
            const target = targetsByService.get(affectedId)
            if (!target) continue
            applyMockUpdateSettlement(
              affectedId,
              target.targetTag,
              target.targetDigest,
              target.pullTags,
            )
          }
        }, settleDelayMs)
      }, updateFinishDelayMs)
      return json({ jobId })
    }
    if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/rollback-target')) {
      const serviceId = decodeURIComponent(urlPath.split('/').slice(3, -1).join('/'))
      const found = findService(serviceId)
      if (!found) return json({ error: { code: 'not_found', message: 'service not found' } }, { status: 404 })

      const currentDigest = found.svc.image.digest ?? ''
      const currentDisplayTag = found.svc.image.resolvedTag ?? found.svc.image.tag ?? null
      const rollbackRaceResponse = await maybeServeRollbackTargetRaceResponse(scenario, serviceId, rollbackTargetRaceByServiceId)
      if (rollbackRaceResponse) return json(rollbackRaceResponse satisfies ServiceRollbackTargetResponse)
      const target = state.rollbackTargetByServiceId[serviceId]
      if (!target) {
        return json({
          available: false,
          currentDigest,
          currentDisplayTag,
          targetDigest: null,
          targetDisplayTag: null,
          sourceUpdateJobId: null,
          sourceFinishedAt: null,
          unavailableReason: 'no_matching_update_history',
          activeJobId: null,
          activeJobStatus: null,
        } satisfies ServiceRollbackTargetResponse)
      }

      return json({
        ...target,
        currentDigest: target.currentDigest || currentDigest,
        currentDisplayTag: target.currentDisplayTag ?? currentDisplayTag,
      } satisfies ServiceRollbackTargetResponse)
    }
    if (method === 'POST' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/rollback')) {
      const serviceId = decodeURIComponent(urlPath.split('/').slice(3, -1).join('/'))
      const found = findService(serviceId)
      if (!found) return json({ error: { code: 'not_found', message: 'service not found' } }, { status: 404 })

      const target = f.rollbackTargetByServiceId[serviceId] ?? {
        available: false,
        currentDigest: found.svc.image.digest ?? '',
        currentDisplayTag: found.svc.image.resolvedTag ?? found.svc.image.tag ?? null,
        targetDigest: null,
        targetDisplayTag: null,
        sourceUpdateJobId: null,
        sourceFinishedAt: null,
        unavailableReason: 'no_matching_update_history',
        activeJobId: null,
        activeJobStatus: null,
      } satisfies ServiceRollbackTargetResponse

      if (!target.available || !target.targetDigest) {
        return json(
          {
            error: {
              code: 'conflict',
              message: 'service rollback is unavailable',
              details: {
                reason: target.unavailableReason ?? 'no_matching_update_history',
                existingJobId: target.activeJobId ?? null,
              },
            },
          },
          { status: 409 },
        )
      }

      jobSeqRef.value += 1
      const jobId = `job-rollback-ui-${jobSeqRef.value}`
      const createdAt = nowIso(-1_500)
      const startedAt = nowIso(-1_000)
      const job: JobListItem = {
        id: jobId,
        type: 'rollback',
        scope: 'service',
        stackId: found.stack.id,
        serviceId,
        status: 'running',
        createdBy: 'ivan',
        reason: 'ui',
        createdAt,
        startedAt,
        finishedAt: null,
        allowArchMismatch: false,
        backupMode: 'inherit',
        summary: {
          mode: 'rollback',
          currentDigest: target.currentDigest,
          currentDisplayTag: target.currentDisplayTag ?? null,
          targetDigest: target.targetDigest,
          targetDisplayTag: target.targetDisplayTag ?? null,
          sourceUpdateJobId: target.sourceUpdateJobId ?? null,
          sourceFinishedAt: target.sourceFinishedAt ?? null,
        },
      }
      f.jobs = [job, ...f.jobs]
      f.jobById[jobId] = {
        ...job,
        logs: [
          { ts: createdAt, level: 'info', msg: 'Rollback queued by UI.' },
          { ts: startedAt, level: 'info', msg: 'Rollback started...' },
        ],
        logsLastId: 2,
      }
      f.rollbackTargetByServiceId[serviceId] = {
        ...target,
        available: false,
        activeJobId: jobId,
        activeJobStatus: 'running',
        unavailableReason: 'rollback_in_progress',
      }

      window.setTimeout(() => {
        const live = f.jobById[jobId]
        if (!live || (live.status !== 'queued' && live.status !== 'running')) return

        const finishedAt = nowIso()
        found.svc.image = {
          ...found.svc.image,
          digest: target.targetDigest!,
          resolvedTag: target.targetDisplayTag ?? found.svc.image.resolvedTag ?? found.svc.image.tag,
          tag: target.targetDisplayTag ?? found.svc.image.tag,
          resolvedTags: target.targetDisplayTag
            ? Array.from(new Set([target.targetDisplayTag, ...(found.svc.image.resolvedTags ?? [])].filter(Boolean)))
            : found.svc.image.resolvedTags ?? null,
        }
        found.svc.candidate = null
        syncStackListItem(found.stack.id)

        const nextLogs = [...live.logs, { ts: finishedAt, level: 'info', msg: 'Rollback finished.' }]
        f.jobById[jobId] = {
          ...live,
          status: 'rolled_back',
          finishedAt,
          logs: nextLogs,
          logsLastId: nextLogs.length,
        }
        f.jobs = f.jobs.map((row) => (row.id === jobId ? { ...row, status: 'rolled_back', finishedAt } : row))
        f.rollbackTargetByServiceId[serviceId] = {
          available: false,
          currentDigest: target.targetDigest!,
          currentDisplayTag: target.targetDisplayTag ?? found.svc.image.resolvedTag ?? found.svc.image.tag ?? null,
          targetDigest: null,
          targetDisplayTag: null,
          sourceUpdateJobId: target.sourceUpdateJobId ?? null,
          sourceFinishedAt: target.sourceFinishedAt ?? finishedAt,
          unavailableReason: 'no_matching_update_history',
          activeJobId: null,
          activeJobStatus: null,
        }
      }, 1_200)

      return json({ jobId })
    }

    const serviceStateResponse = await handleServiceStateRoutes(routeCtx)
    if (serviceStateResponse) return serviceStateResponse

    return json({ error: `unhandled mock route: ${method} ${urlString}` }, { status: 501 })
  }
}
