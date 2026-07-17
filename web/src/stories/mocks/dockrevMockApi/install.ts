import type {
  CleanupApplyRequest,
  CleanupScanRequest,
  CleanupScanResponse,
  HomepageNavResponse,
  JobDetail,
  JobListItem,
  ServiceGitHubRepoRef,
  ServiceRollbackTargetResponse,
  StackDetail,
  StackSettings,
} from '../../../api'
import { isDockrevImageRef } from '../../../runtimeConfig'
import { serviceRowStatus } from '../../../updateStatus'
import { buildCleanupMockScanResponse, isCleanupMockScenario, resolveCleanupMockApply, type CleanupMockScenario, type CleanupMockRuntimeState } from '../cleanupMockData'
import type { MockRouteContext } from './context'
import { buildMockDiscoveryTimeline as buildMockDiscoveryTimelineResponse } from './discoveryTimeline'
import { buildFixture } from './fixturesMisc'
import { buildMockGitHubReleaseLocateResponse, buildMockGitHubReleasesResponse } from './githubReleases'
import { handleGhcrRoutes } from './handlers/ghcr'
import { handleServiceStateRoutes } from './handlers/serviceState'
import { applyRollbackTargetRaceAfterUpdate, maybeServeRollbackTargetRaceResponse, type RollbackTargetRaceState } from './rollbackRace'
import type { DockrevApiScenario, DockrevMockApiOptions } from './shared'
import {
  MockEventSource,
  buildResourceHistorySamples,
  type Fixture,
  getBoolean,
  getString,
  isRecord,
  json,
  makeMockDebug,
  nowIso,
  parseJsonBody,
  realFetch,
  summarizeVersionInferenceRows,
  type VersionInferenceOverviewMock,
} from './shared'

function cloneFixture(fixture: Fixture): Fixture {
  if (typeof structuredClone === 'function') {
    return structuredClone(fixture)
  }
  return JSON.parse(JSON.stringify(fixture)) as Fixture
}

function nextNumericSuffix(value: string): number {
  const match = value.match(/(\d+)(?!.*\d)/)
  if (!match) return 0
  const parsed = Number.parseInt(match[1] ?? '0', 10)
  return Number.isFinite(parsed) ? parsed : 0
}

function seedIgnoreSequence(fixture: Fixture | null): number {
  if (!fixture) return 0
  return fixture.ignores.reduce((max, rule) => Math.max(max, nextNumericSuffix(rule.id)), 0)
}

function seedJobSequence(fixture: Fixture | null): number {
  if (!fixture) return 0
  return fixture.jobs.reduce((max, job) => Math.max(max, nextNumericSuffix(job.id)), 0)
}

export function installDockrevMockApi(
  scenario: DockrevApiScenario,
  options: DockrevMockApiOptions = {},
) {
  const state = scenario === 'error' ? null : cloneFixture(options.initialFixture ?? buildFixture(scenario))
  const cleanupScenario: CleanupMockScenario | null =
    options.cleanupScenario ?? (isCleanupMockScenario(scenario) ? scenario : null)
  if (state) {
    if (options.jobsOverride) {
      state.jobs = options.jobsOverride
      state.jobById = Object.fromEntries(
        options.jobsOverride.map((job) => [
          job.id,
          { ...job, logs: [], logsLastId: 0 } satisfies JobDetail,
        ]),
      )
    }
    for (const stack of Object.values(state.stackById)) {
      stack.services = stack.services.map((service) => {
        const override = options.serviceOverridesById?.[service.id]
        if (!override) return service
        const nextService = { ...service, ...override }
        state.serviceSettingsById[service.id] = nextService.settings
        return nextService
      })
    }
    if (options.serviceTagSuggestionsById) {
      state.serviceTagSuggestionsById = {
        ...state.serviceTagSuggestionsById,
        ...options.serviceTagSuggestionsById,
      }
    }
    if (options.serviceBackupRecordsById) {
      state.serviceBackupRecordsById = {
        ...state.serviceBackupRecordsById,
        ...options.serviceBackupRecordsById,
      }
    }
    if (options.serviceLogsByServiceId) {
      state.serviceLogsByServiceId = {
        ...state.serviceLogsByServiceId,
        ...options.serviceLogsByServiceId,
      }
    }
    if (options.deployCheckReportOverride) {
      state.deployCheckReport = {
        ...state.deployCheckReport,
        ...options.deployCheckReportOverride,
      }
    }
    if (options.deployWelcomeOverride) {
      state.deployWelcome = {
        ...state.deployWelcome,
        ...options.deployWelcomeOverride,
      }
    }
  }
  let lastSerializedState = state ? JSON.stringify(state) : null
  const persistState = () => {
    if (!state || !options.onStateChange) return
    const nextSerializedState = JSON.stringify(state)
    if (nextSerializedState === lastSerializedState) return
    lastSerializedState = nextSerializedState
    options.onStateChange(cloneFixture(state))
  }
  if (state && options.onStateChange) {
    options.onStateChange(cloneFixture(state))
  }
  const initialJobSeq = seedJobSequence(state)
  const ignoreSeqRef = { value: seedIgnoreSequence(state) }
  const jobSeqRef = { value: initialJobSeq }
  const digestSnapshotPendingAttempts = new Map<string, number>()
  const forcedDigestSnapshotPendingAttempts = new Map<string, number>()
  const jobsEventsSeqRef = { value: 4_000 + initialJobSeq }
  const queueProgressDemoSteps = [40, 44, 48, 52, 56, 60, 65, 70, 75, 80, 85, 90, 94, 97]
  let queueProgressDemoStep = 0
  let queueProgressDemoDirection = 1
  const cleanupRuntime: CleanupMockRuntimeState = {
    nextJobSeq: 0,
    staleApplyConsumed: false,
    nextScanRunSeq: 0,
    scanRuns: new Map(),
  }

  const parseCleanupScanRequest = (body: unknown): CleanupScanRequest => {
    const parsed = parseJsonBody(body) as CleanupScanRequest | null
    return {
      reason: parsed?.reason === 'confirm' ? 'confirm' : 'page',
      refresh: parsed?.refresh !== false,
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
  }

  const partialCleanupResponse = (response: CleanupScanResponse): CleanupScanResponse => {
    const firstStack = response.stackGroups[0]
    const partialStacks = firstStack
      ? [
          {
            ...firstStack,
            services: firstStack.services.slice(0, 1),
            stackOrphans: firstStack.stackOrphans.slice(0, 1),
          },
        ]
      : []
    return {
      ...response,
      status: 'pending',
      refreshing: true,
      retryAfterMs: 450,
      serverDiskUsage: null,
      stackGroups: partialStacks,
      unownedGroup: null,
      confirmationFingerprint: null,
    }
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

  function buildHomepageNavResponse(f: Fixture): HomepageNavResponse {
    const generatedAt = new Date().toISOString()
    const staleAfterSeconds = Math.max(60, f.settings.resourceMonitor.sampleIntervalSeconds * 2)
    const resourceSummary = {
      enabled: f.settings.resourceMonitor.enabled,
      window: '1h',
      generatedAt,
      staleAfterSeconds,
      services: Object.values(f.stackById)
        .filter((stack) => !stack.archived)
        .flatMap((stack) => stack.services.filter((service) => !service.archived))
        .map((service) => {
          const samples = buildResourceHistorySamples(service.id, 60 * 60)
          const latest = samples[samples.length - 1] ?? null
          const previous = samples[samples.length - 2] ?? null
          const prevTs = previous ? Date.parse(previous.sampledAt) : Number.NaN
          const nextTs = latest ? Date.parse(latest.sampledAt) : Number.NaN
          const seconds = Number.isFinite(prevTs) && Number.isFinite(nextTs) ? (nextTs - prevTs) / 1000 : 0
          const rate = (prev: number | null | undefined, next: number | null | undefined) =>
            seconds > 0 && prev != null && next != null && next >= prev ? (next - prev) / seconds : null
          const sampledAtMs = latest ? Date.parse(latest.sampledAt) : Number.NaN
          return {
            serviceId: service.id,
            sampledAt: latest?.sampledAt ?? null,
            cpuPercent: latest?.cpuPercent ?? null,
            memUsedBytes: latest?.memUsedBytes ?? null,
            memLimitBytes: latest?.memLimitBytes ?? null,
            netRxRateBps: rate(previous?.netRxBytes, latest?.netRxBytes),
            netTxRateBps: rate(previous?.netTxBytes, latest?.netTxBytes),
            stale: !Number.isFinite(sampledAtMs) || Date.now() - sampledAtMs > staleAfterSeconds * 1000,
            sampleCount: samples.length,
          }
        }),
    }
    return {
      generatedAt,
      lastCheckAt: f.stacks.map((stack) => stack.lastCheckAt).sort().at(-1) ?? null,
      resourceSummary,
      items: Object.values(f.stackById)
        .filter((stack) => !stack.archived)
        .flatMap((stack) =>
          stack.services
            .filter((service) => !service.archived && service.homepage?.href)
            .map((service) => ({
              stackId: stack.id,
              stackName: stack.name,
              serviceId: service.id,
              serviceName: service.name,
              imageRef: service.image.ref,
              imageTag: service.image.tag,
              imageDigest: service.image.digest ?? null,
              imageResolvedTag: service.image.resolvedTag ?? null,
              imageResolvedTags: service.image.resolvedTags ?? null,
              isDockrev: isDockrevImageRef(service.image.ref),
              homepage: service.homepage!,
              candidate: service.candidate ?? null,
              ignore: service.ignore ?? null,
              versionInference: service.versionInference ?? null,
              newVersionDiscoveryCount: service.newVersionDiscoveryCount ?? null,
              settings: service.settings,
              archived: service.archived,
              resource:
                resourceSummary.services.find((item) => item.serviceId === service.id) ?? {
                  serviceId: service.id,
                  sampledAt: null,
                  cpuPercent: null,
                  memUsedBytes: null,
                  memLimitBytes: null,
                  netRxRateBps: null,
                  netTxRateBps: null,
                  stale: true,
                  sampleCount: 0,
                },
            })),
        ),
    }
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
      if (options.supervisorSelfUpgradeResponse) {
        return json(options.supervisorSelfUpgradeResponse.body, {
          status: options.supervisorSelfUpgradeResponse.status,
        })
      }
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
    try {
      if (
        scenario === 'overview-homepage-slow-refresh' &&
        method === 'GET' &&
        urlPath === '/api/homepage/nav'
      ) {
        await new Promise<void>((resolve) => {
          globalThis.setTimeout(() => resolve(), 900)
        })
      }
      if (method === 'GET' && urlPath === '/api/homepage/nav') {
        if (scenario === 'overview-resource-monitor-error') {
          return json(
            { error: { code: 'upstream_error', message: 'homepage nav unavailable' } },
            { status: 503 },
          )
        }
        return json(buildHomepageNavResponse(f))
      }
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
        buildMockGitHubReleasesResponse: (serviceId, page, perPage) =>
          buildMockGitHubReleasesResponse(
            serviceId,
            page,
            perPage,
            options,
            findService,
            parseMockGitHubRepoRef,
          ),
        buildMockGitHubReleaseLocateResponse: (serviceId, version, perPage, limit) =>
          buildMockGitHubReleaseLocateResponse(
            serviceId,
            version,
            perPage,
            limit,
            options,
            findService,
            parseMockGitHubRepoRef,
          ),
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

    if (urlPath === '/api/jobs/events' && method === 'GET' && options.jobsEventsPayload != null) {
      const debug = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
      debug.jobsEventsCalls += 1
      return new Response(options.jobsEventsPayload ?? ': keep-alive\n\n', {
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

    if (cleanupScenario && method === 'POST' && urlPath === '/api/cleanups/scan-runs') {
      const request = parseCleanupScanRequest(init?.body)
      cleanupRuntime.nextScanRunSeq += 1
      const scanId = `mock-cleanup-scan-${cleanupRuntime.nextScanRunSeq}`
      const previousSnapshot =
        cleanupScenario === 'cleanup-console-scan-pending' ? null : buildCleanupMockScanResponse(cleanupScenario, request, 1)
      const ready = buildCleanupMockScanResponse(cleanupScenario, request, cleanupScenario === 'cleanup-console-stale' ? 2 : 1)
      const holdPartial = cleanupScenario === 'cleanup-console-scan-slow' && cleanupRuntime.nextScanRunSeq > 1
      const events: Array<{ id: number; event: string; data: unknown }> = [
        {
          id: 1,
          event: 'scan_started',
          data: { scanId, phase: 'scan_started', response: previousSnapshot },
        },
      ]
      if (cleanupScenario !== 'cleanup-console-scan-pending') {
        events.push({
          id: 2,
          event: 'scan_partial',
          data: { scanId, phase: 'scan_partial', response: partialCleanupResponse(ready) },
        })
        if (!holdPartial) {
          events.push({
            id: 3,
            event: 'scan_ready',
            data: { scanId, phase: 'scan_ready', response: ready },
          })
        }
      }
      cleanupRuntime.scanRuns.set(scanId, events)
      return json({ scanId, previousSnapshot, retryAfterMs: 450 })
    }

    if (cleanupScenario && method === 'GET' && urlPath.match(/^\/api\/cleanups\/scan-runs\/[^/]+\/events$/)) {
      const scanId = decodeURIComponent(urlPath.split('/')[4] ?? '')
      const afterId = Number.parseInt(url?.searchParams.get('afterId') ?? '0', 10)
      const events = (cleanupRuntime.scanRuns.get(scanId) ?? []).filter((event) => event.id > (Number.isFinite(afterId) ? afterId : 0))
      const body = events
        .map((event) => `id: ${event.id}\nevent: ${event.event}\ndata: ${JSON.stringify(event.data)}\n\n`)
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

    if (cleanupScenario && method === 'POST' && urlPath === '/api/cleanups/scan') {
      if (cleanupScenario === 'cleanup-console-scan-pending') {
        const request = parseCleanupScanRequest(init?.body)
        const ready = buildCleanupMockScanResponse('cleanup-console', request)
        if (request.reason === 'page') {
          return json(
            {
              ...ready,
              status: 'pending',
              refreshing: true,
              retryAfterMs: 450,
              scannedAt: null,
              estimatedReclaimableBytes: null,
              hasUnknownSize: false,
              serverDiskUsage: null,
              stackGroups: [],
              unownedGroup: null,
              confirmationFingerprint: null,
            },
            { status: 202 },
          )
        }
        return json(
          {
            ...ready,
            refreshing: true,
            retryAfterMs: 450,
          },
          { status: 200 },
        )
      }
      if (cleanupScenario === 'cleanup-console-scan-slow') {
        await new Promise<void>((resolve) => {
          globalThis.setTimeout(() => resolve(), 1600)
        })
      }
      const request = parseCleanupScanRequest(init?.body)
      return json(buildCleanupMockScanResponse(cleanupScenario, request))
    }

    if (cleanupScenario && method === 'POST' && urlPath === '/api/cleanups/apply') {
      if (cleanupScenario === 'cleanup-console-apply-slow') {
        return new Promise<Response>(() => {})
      }
      const parsed = parseJsonBody(init?.body) as CleanupApplyRequest | null
      if (!parsed) {
        return json({ error: { code: 'invalid_argument', message: 'invalid cleanup apply payload', details: null } }, { status: 400 })
      }
      const result = resolveCleanupMockApply(cleanupScenario, parsed, cleanupRuntime)
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
    if (method === 'GET' && urlPath.startsWith('/api/stacks/') && urlPath.endsWith('/settings')) {
      const id = decodeURIComponent(urlPath.split('/').slice(3, -1).join('/'))
      const settings = f.stackSettingsById[id] ?? { autoUpdatePolicy: { mode: 'override', enabled: false, rules: [] } }
      return json(settings)
    }
    if (method === 'PUT' && urlPath.startsWith('/api/stacks/') && urlPath.endsWith('/settings')) {
      const id = decodeURIComponent(urlPath.split('/').slice(3, -1).join('/'))
      const body = typeof init?.body === 'string' ? init.body : ''
      const parsed = body ? (JSON.parse(body) as StackSettings) : null
      if (parsed) f.stackSettingsById[id] = parsed
      return json({ ok: true })
    }
    if (method === 'GET' && urlPath.startsWith('/api/stacks/')) {
      const id = decodeURIComponent(urlPath.split('/').slice(3).join('/'))
      const dbg = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
      dbg.stackDetailCalls += 1
      dbg.stackDetailCallsById[id] = (dbg.stackDetailCallsById[id] ?? 0) + 1
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

        persistState()
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
          persistState()
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
        persistState()
      }, 1_200)

      return json({ jobId })
    }

    const serviceStateResponse = await handleServiceStateRoutes(routeCtx)
    if (serviceStateResponse) return serviceStateResponse

    return json({ error: `unhandled mock route: ${method} ${urlString}` }, { status: 501 })
    } finally {
      persistState()
    }
  }
}
