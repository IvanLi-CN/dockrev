import type {
  DeployCheckReportEnvelope,
  DeployCheckReportResponse,
  DeployWelcomeResponse,
  DiscoveredProject,
  GitHubPackagesRepo,
  GitHubPackagesSettingsResponse,
  IgnoreRule,
  JobDetail,
  JobListItem,
  NewVersionDiscoveryTimelineResponse,
  NotificationConfig,
  NotificationTestChannel,
  ServiceGitHubReleaseItem,
  ServiceGitHubReleasesStatus,
  ServiceGitHubRepoRef,
  ServiceReleaseNotesAnchor,
  ServiceReleaseNotesStale,
  ServiceBackupTargetsResponse,
  ServiceBackupRecordsResponse,
  ServiceRepoLinkInferenceResponse,
  ServiceResourcePeak,
  ServiceResourceSample,
  ServiceLogEventEnvelope,
  ServiceLogSnapshotResponse,
  ServiceRollbackTargetResponse,
  ServiceSettings,
  ServiceTagSuggestionItem,
  SettingsResponse,
  StackSettings,
  StackDetail,
  StackListItem,
  GitHubReleaseAuthMode,
} from '../../../api'
import type { CleanupMockScenario } from '../cleanupMockData'

export type DockrevApiScenario =
  | 'cleanup-console'
  | 'cleanup-console-storage-normal'
  | 'cleanup-console-empty'
  | 'cleanup-console-aggressive-unowned'
  | 'cleanup-console-stale'
  | 'cleanup-console-scan-pending'
  | 'cleanup-console-scan-slow'
  | 'cleanup-console-apply-slow'
  | 'cleanup-console-unknown-volume-only'
  | 'default'
  | 'dashboard-demo'
  | 'dashboard-demo-slow-update'
  | 'dashboard-demo-hydrated-update'
  | 'service-action-progress'
  | 'service-detail-rollback-available'
  | 'service-detail-rollback-unavailable'
  | 'service-detail-rollback-active'
  | 'service-detail-rollback-confirm-open'
  | 'service-detail-history-rollback-action'
  | 'service-detail-rollback-stale-after-update'
  | 'service-detail-lifecycle-running'
  | 'service-detail-lifecycle-stopped'
  | 'service-detail-lifecycle-partial'
  | 'service-detail-lifecycle-unknown'
  | 'service-detail-lifecycle-active'
  | 'stack-detail-lifecycle-running'
  | 'stack-detail-lifecycle-stopped'
  | 'stack-detail-lifecycle-partial'
  | 'stack-detail-lifecycle-unknown'
  | 'stack-detail-lifecycle-active'
  | 'link-icon-catalog'
  | 'digest-pinned-image-display'
  | 'services-inference-pending-candidate-loading'
  | 'service-detail-compose-fallbacks'
  | 'service-detail-version-anomaly'
  | 'service-detail-resource-monitor-disabled'
  | 'service-detail-resource-monitor-empty'
  | 'service-detail-resource-monitor-stream-error'
  | 'service-detail-logs'
  | 'repo-link-editing'
  | 'guide-line-long-names'
  | 'resolved-tag-demo'
  | 'version-inference-overview'
  | 'version-inference-resync-required'
  | 'version-inference-idle'
  | 'version-inference-running'
  | 'version-inference-queue-backlog'
  | 'version-inference-stale-all-failed'
  | 'version-tags-popover-demo'
  | 'version-tags-popover-same-digest'
  | 'version-tags-popover-snapshot-pending'
  | 'version-tags-popover-snapshot-missing'
  | 'archived-stack-detail-navigation'
  | 'multi-stack-mixed'
  | 'overview-discovery-readable'
  | 'overview-discovery-stale-temp-reconcile'
  | 'overview-resource-monitor-error'
  | 'overview-homepage-slow-refresh'
  | 'overview-resource-monitor-stale'
  | 'overview-resource-monitor-zero-rates'
  | 'aggregate-dockrev-guard'
  | 'aggregate-dockrev-only'
  | 'overview-jobs-card-heavy-inflight'
  | 'overview-jobs-card-running-progress-modes'
  | 'overview-jobs-card-terminal-only'
  | 'overview-jobs-card-global-labels'
  | 'overview-jobs-card-exact-five-non-terminal'
  | 'queue-mixed'
  | 'queue-progress-smoothing'
  | 'queue-health-rollback'
  | 'queue-legacy-progress'
  | 'queue-update-layer-progress'
  | 'queue-update-cancelled'
  | 'queue-update-indeterminate'
  | 'queue-update-download-determinate'
  | 'queue-long-logs'
  | 'queue-backup-progress'
  | 'settings-configured'
  | 'settings-configured-load-slow'
  | 'settings-configured-resolve-slow'
  | 'settings-notification-channel-errors'
  | 'no-candidates'
  | 'empty'
  | 'error'

export type DockrevMockApiOptions = {
  dockrevApiBehaviorByRoute?: Record<string, DockrevApiRouteBehavior>
  cleanupScenario?: CleanupMockScenario
  initialFixture?: Fixture
  onStateChange?: (fixture: Fixture) => void
  jobsOverride?: JobListItem[]
  jobsEventsPayload?: string
  discoveryTimelineByServiceId?: Record<string, NewVersionDiscoveryTimelineResponse>
  discoveryTimelineErrorServiceIds?: string[]
  githubReleasesByServiceId?: Record<string, DockrevMockGitHubReleasesDataset>
  serviceOverridesById?: Record<string, Partial<StackDetail['services'][number]>>
  serviceBackupRecordsById?: Record<string, ServiceBackupRecordsResponse>
  serviceLogsByServiceId?: Record<string, ServiceLogsMockDataset>
  serviceTagSuggestionsById?: Record<string, ServiceTagSuggestionItem[]>
  deployCheckReportOverride?: Partial<DeployCheckReportEnvelope>
  deployCheckReportSequence?: DeployCheckReportEnvelope[]
  deployWelcomeOverride?: Partial<DeployWelcomeResponse>
  supervisorSelfUpgradeResponse?: {
    status: number
    body: unknown
  }
}

export type DockrevApiRouteBehavior = {
  delayMs?: number
  failTimes?: number
  failureStatus?: number
  failureBody?: unknown
}

export type ServiceLogsMockDataset = {
  snapshot: ServiceLogSnapshotResponse
  eventsGate?: string
  eventsPayload?: string
}

export type MockServiceLogEventGateState = {
  released: Set<string>
  waiting: Set<string>
  abortController: AbortController
}

export type DockrevMockGitHubReleasesDataset = {
  authMode?: GitHubReleaseAuthMode
  repo?: ServiceGitHubRepoRef | null
  listStatus?: ServiceGitHubReleasesStatus
  listMessage?: string | null
  stale?: ServiceReleaseNotesStale | null
  items?: ServiceGitHubReleaseItem[]
  locateByVersion?: Record<string, Partial<ServiceReleaseNotesAnchor>>
}

export const realFetch = globalThis.fetch.bind(globalThis)

type ParsedSseEvent = {
  id: string
  event: string
  data: string
}

export function parseSsePayload(payload: string): ParsedSseEvent[] {
  const chunks = payload.split(/\r?\n\r?\n/)
  const out: ParsedSseEvent[] = []
  for (const chunk of chunks) {
    const lines = chunk.split(/\r?\n/)
    let id = ''
    let event = ''
    const dataLines: string[] = []
    for (const line of lines) {
      if (!line || line.startsWith(':')) continue
      if (line.startsWith('id:')) {
        id = line.slice(3).trim()
        continue
      }
      if (line.startsWith('event:')) {
        event = line.slice(6).trim()
        continue
      }
      if (line.startsWith('data:')) {
        dataLines.push(line.slice(5).trimStart())
      }
    }
    if (!id && !event && dataLines.length === 0) continue
    out.push({ id, event, data: dataLines.join('\n') })
  }
  return out
}

export class MockEventSource extends EventTarget {
  static readonly CONNECTING = 0
  static readonly OPEN = 1
  static readonly CLOSED = 2
  static pollIntervalMs = 4_000

  readonly CONNECTING = MockEventSource.CONNECTING
  readonly OPEN = MockEventSource.OPEN
  readonly CLOSED = MockEventSource.CLOSED

  readonly url: string
  readonly withCredentials: boolean

  readyState = MockEventSource.CONNECTING
  onopen: ((this: EventSource, ev: Event) => unknown) | null = null
  onmessage: ((this: EventSource, ev: MessageEvent) => unknown) | null = null
  onerror: ((this: EventSource, ev: Event) => unknown) | null = null

  private closed = false
  private closeReported = false
  private polling = false
  private lastEventId = ''
  private pollTimer: number | null = null

  constructor(url: string | URL, eventSourceInitDict?: EventSourceInit) {
    super()
    this.url = String(url)
    this.withCredentials = eventSourceInitDict?.withCredentials === true
    if (this.url.includes('/resource-usage/events')) {
      const debug = globalThis.__DOCKREV_MOCK_DEBUG__
      if (debug) debug.resourceUsageEventSourceCalls += 1
    }
    this.connect()
    this.pollTimer = window.setInterval(() => {
      this.connect()
    }, MockEventSource.pollIntervalMs)
  }

  close() {
    if (this.closed) return
    this.closed = true
    this.readyState = MockEventSource.CLOSED
    if (!this.closeReported && this.url.includes('/resource-usage/events')) {
      const debug = globalThis.__DOCKREV_MOCK_DEBUG__
      if (debug) debug.resourceUsageEventSourceCloseCalls += 1
      this.closeReported = true
    }
    if (this.pollTimer != null) {
      window.clearInterval(this.pollTimer)
      this.pollTimer = null
    }
  }

  private emitOpen() {
    if (this.closed || this.readyState === MockEventSource.OPEN) return
    this.readyState = MockEventSource.OPEN
    const evt = new Event('open')
    this.onopen?.call(this as unknown as EventSource, evt)
    this.dispatchEvent(evt)
  }

  private emitError() {
    if (this.closed) return
    this.readyState = MockEventSource.CONNECTING
    const evt = new Event('error')
    this.onerror?.call(this as unknown as EventSource, evt)
    this.dispatchEvent(evt)
  }

  private emitMessage(name: string, data: string, id: string) {
    if (this.closed) return
    const evt = new MessageEvent(name || 'message', {
      data,
      lastEventId: id,
      origin: typeof window !== 'undefined' ? window.location.origin : '',
    })
    if (!name || name === 'message') this.onmessage?.call(this as unknown as EventSource, evt)
    this.dispatchEvent(evt)
  }

  private async connect() {
    if (this.closed || this.polling) return
    this.polling = true
    try {
      const u = new URL(this.url, typeof window !== 'undefined' ? window.location.href : 'http://localhost')
      if (this.lastEventId) u.searchParams.set('afterId', this.lastEventId)
      const resp = await globalThis.fetch(u.toString(), {
        method: 'GET',
        credentials: this.withCredentials ? 'include' : 'same-origin',
        headers: { Accept: 'text/event-stream', ...(this.lastEventId ? { 'Last-Event-ID': this.lastEventId } : {}) },
      })
      if (!resp.ok) throw new Error(`SSE request failed: ${resp.status}`)
      const payload = await resp.text()
      const managementCursor = u.pathname === '/api/events' ? resp.headers.get('x-dockrev-management-cursor') : null
      this.emitOpen()
      for (const evt of parseSsePayload(payload)) {
        this.emitMessage(evt.event || 'message', evt.data, evt.id)
        if (evt.id) this.lastEventId = evt.id
      }
      if (managementCursor && !this.lastEventId) this.lastEventId = managementCursor
    } catch {
      this.emitError()
    } finally {
      this.polling = false
    }
  }
}

export type MockDebug = {
  jobsEventsCalls: number
  jobsListCalls: number
  jobsListUrls: string[]
  ghcrReposUrls: string[]
  lastUpdateRequest: unknown | null
  lastUpdateUrl: string | null
  lastUpdateMethod: string | null
  lastLifecycleRequest: { kind: 'stack' | 'service'; id: string; action: string } | null
  stackDetailCalls: number
  stackDetailCallsById: Record<string, number>
  digestTagsSnapshotCalls: number
  digestTagsCalls: number
  lastDigestTagsSnapshotUrl: string | null
  lastDigestTagsUrl: string | null
  versionInferenceRefreshCalls: number
  lastVersionInferenceRefreshUrl: string | null
  lastVersionInferenceRefreshDigest: string | null
  serviceTagSuggestionCalls: number
  lastServiceTagSuggestionUrl: string | null
  lastComposeTagRequest: unknown | null
  resourceUsageEventSourceCalls: number
  resourceUsageEventSourceCloseCalls: number
  resourceUsageLastSnapshot: ServiceResourceSample | null
  resourceUsageLastTick: ServiceResourceSample | null
}

export type VersionInferenceTaskProgressMock = {
  phase: string
  message: string
  current: number
  total: number
  percent: number
  assignedCurrent?: number
  assignedTotal?: number
  assignedPercent?: number
  resultCurrent?: number
  resultTotal?: number
  resultPercent?: number
  updatedAt: string
}

export type VersionInferenceTaskMock = {
  key: string
  imageRepo: string
  hostPlatform: string
  status: string
  reason: string
  enqueuedAt: string
  startedAt?: string | null
  updatedAt: string
  progress?: VersionInferenceTaskProgressMock | null
}

export type VersionInferenceOverviewRowMock = {
  key: string
  imageRepo: string
  hostPlatform: string
  status: string
  serviceCount: number
  reason?: string | null
  checkedAt?: string | null
  updatedAt?: string | null
  progress?: VersionInferenceTaskProgressMock | null
}

export type VersionInferenceOverviewMock = {
  worker: {
    maxConcurrency: number
    queued: number
    running: number
    inFlight: number
  }
  gc: {
    retentionDays: number
    intervalSeconds: number
    lastRunAt?: string | null
    lastDeleted?: number | null
    lastDurationMs?: number | null
    lastError?: string | null
  }
  summary: {
    snapshotsTotal: number
    queued: number
    running: number
    ready: number
    stale: number
    allFailed: number
  }
  tasks: VersionInferenceTaskMock[]
  rows: VersionInferenceOverviewRowMock[]
  page: number
  perPage: number
  total: number
}

export type VersionInferenceEventMock = {
  id: number
  data: Record<string, unknown>
}

declare global {
  var __DOCKREV_MOCK_DEBUG__: MockDebug | undefined
  var __DOCKREV_MOCK_EVENT_GATES__: MockServiceLogEventGateState | undefined
}

export type Fixture = {
  stacks: StackListItem[]
  stackById: Record<string, StackDetail>
  jobs: JobListItem[]
  jobById: Record<string, JobDetail>
  ignores: IgnoreRule[]
  discoveredProjects: DiscoveredProject[]
  settings: SettingsResponse
  notifications: NotificationConfig
  githubPackagesSettings: GitHubPackagesSettingsResponse
  githubPackagesRepos: GitHubPackagesRepo[]
  serviceSettingsById: Record<string, ServiceSettings>
  serviceBackupTargetsById: Record<string, ServiceBackupTargetsResponse>
  serviceBackupRecordsById: Record<string, ServiceBackupRecordsResponse>
  stackSettingsById: Record<string, StackSettings>
  rollbackTargetByServiceId: Record<string, ServiceRollbackTargetResponse>
  repoLinkInferenceByServiceId: Record<string, ServiceRepoLinkInferenceResponse>
  serviceTagSuggestionsById: Record<string, ServiceTagSuggestionItem[]>
  serviceLogsByServiceId: Record<string, ServiceLogsMockDataset>
  deployCheckReport: DeployCheckReportEnvelope
  deployWelcome: DeployWelcomeResponse
  versionInferenceOverview: VersionInferenceOverviewMock
  versionInferenceEvents: VersionInferenceEventMock[]
}

export function buildServiceLogsSsePayload(events: ServiceLogEventEnvelope[]): string {
  if (events.length === 0) return ': keep-alive\n\n'
  return events
    .map((event) => {
      const eventName = event.type === 'reset' ? 'service_log_reset' : 'service_log_line'
      return `id: ${event.id}\nevent: ${eventName}\ndata: ${JSON.stringify(event)}\n\n`
    })
    .join('')
}

export function json(data: unknown, init?: ResponseInit) {
  return new Response(JSON.stringify(data), {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers ?? {}),
    },
  })
}

export function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null
}

export function parseJsonBody(body: unknown): unknown {
  if (typeof body !== 'string' || !body) return null
  try {
    return JSON.parse(body) as unknown
  } catch {
    return null
  }
}

export function getString(v: unknown): string | null {
  return typeof v === 'string' ? v : null
}

export function getBoolean(v: unknown): boolean | null {
  return typeof v === 'boolean' ? v : null
}

export function toNotificationsResponse(input: NotificationConfig): NotificationConfig {
  const botTokenRaw = input.telegram.botToken ?? ''
  const botTokenConfigured = input.telegram.botTokenConfigured ?? botTokenRaw.trim().length > 0
  return {
    ...input,
    telegram: {
      ...input.telegram,
      botToken: null,
      botTokenConfigured,
    },
  }
}

export function isMaskLiteral(value: string): boolean {
  return value === '******' || value === '••••••••••••••••'
}

export function isVariableMaskLiteral(value: string): boolean {
  const trimmed = value.trim()
  return trimmed.length > 0 && /^[*•]+$/.test(trimmed)
}

export const TELEGRAM_BOT_TOKEN_PATTERN = /^\d{5,}:[A-Za-z0-9_-]{8,}$/

export function isValidTelegramBotToken(value: string): boolean {
  return TELEGRAM_BOT_TOKEN_PATTERN.test(value) && !/\s/.test(value)
}

export function isNotificationTestChannel(v: string): v is NotificationTestChannel {
  return v === 'email' || v === 'webhook' || v === 'telegram' || v === 'webPush'
}

export function mockNotificationChannelResult(
  notifications: NotificationConfig,
  channel: NotificationTestChannel,
): { ok: boolean; error?: string } {
  if (channel === 'email') {
    const smtpUrl = (notifications.email.smtpUrl ?? '').trim()
    if (!smtpUrl) return { ok: false, error: 'email.smtpUrl missing' }
    return { ok: true }
  }

  if (channel === 'webhook') {
    const url = (notifications.webhook.url ?? '').trim()
    if (!url) return { ok: false, error: 'webhook.url missing' }
    return { ok: true }
  }

  if (channel === 'telegram') {
    const botToken = (notifications.telegram.botToken ?? '').trim()
    if (!botToken) return { ok: false, error: 'telegram.botToken missing' }
    const chatId = (notifications.telegram.chatId ?? '').trim()
    if (!chatId) return { ok: false, error: 'telegram.chatId missing' }
    return { ok: true }
  }

  const privateKey = (notifications.webPush.vapidPrivateKey ?? '').trim()
  if (!privateKey) return { ok: false, error: 'webPush.vapidPrivateKey missing' }
  return { ok: true }
}

export function hashString(input: string): number {
  let h = 0
  for (let i = 0; i < input.length; i += 1) {
    h = Math.imul(31, h) + input.charCodeAt(i)
    h |= 0
  }
  return Math.abs(h)
}

export function parseResourceWindow(windowRaw: string | null): { window: '3m' | '1h' | '24h' | '7d' | '30d'; seconds: number } {
  if (windowRaw === '3m') return { window: '3m', seconds: 3 * 60 }
  if (windowRaw === '24h') return { window: '24h', seconds: 24 * 60 * 60 }
  if (windowRaw === '7d') return { window: '7d', seconds: 7 * 24 * 60 * 60 }
  if (windowRaw === '30d') return { window: '30d', seconds: 30 * 24 * 60 * 60 }
  return { window: '1h', seconds: 60 * 60 }
}

export function parseMockVersion(input: string | null | undefined): [number, number, number] | null {
  const trimmed = (input ?? '').trim()
  const match = /^v?(\d+)(?:\.(\d+))?(?:\.(\d+))?$/.exec(trimmed)
  if (!match) return null
  return [
    Number.parseInt(match[1] ?? '0', 10),
    Number.parseInt(match[2] ?? '0', 10),
    Number.parseInt(match[3] ?? '0', 10),
  ]
}

export function offsetMockVersion(input: string | null | undefined, delta: number, fallback: string): string {
  const parsed = parseMockVersion(input)
  if (!parsed) return fallback
  const [major, minor, patch] = parsed
  return `${major}.${minor}.${Math.max(0, patch + delta)}`
}

export function buildResourceHistorySamples(serviceId: string, seconds: number, window?: string): ServiceResourceSample[] {
  const stepSeconds = window === '7d' ? 60 : window === '30d' ? 300 : 30
  const points = Math.max(8, Math.floor(seconds / stepSeconds))
  const seed = hashString(serviceId)
  const baseCpu = 8 + (seed % 28)
  const baseMem = (220 + (seed % 420)) * 1024 * 1024
  const memWave = (24 + (seed % 96)) * 1024 * 1024
  const basePids = 8 + (seed % 26)
  const containerCount = 1 + (seed % 2)
  let netRx = (18 + (seed % 40)) * 1024 * 1024
  let netTx = (12 + (seed % 36)) * 1024 * 1024
  let blockRead = (40 + (seed % 50)) * 1024 * 1024
  let blockWrite = (28 + (seed % 44)) * 1024 * 1024
  const now = Date.now()
  const start = now - points * stepSeconds * 1000
  const out: ServiceResourceSample[] = []

  for (let i = 0; i <= points; i += 1) {
    const t = start + i * stepSeconds * 1000
    const rad = i / 5 + (seed % 7)
    const cpu = Math.max(0, Math.min(100, baseCpu + Math.sin(rad) * 9 + Math.cos(rad / 2) * 5))
    const memUsed = Math.max(64 * 1024 * 1024, Math.round(baseMem + Math.sin(rad / 2) * memWave))
    const memLimit = 1024 * 1024 * 1024
    const pids = Math.max(1, Math.round(basePids + Math.sin(rad / 1.4) * 4))

    netRx += Math.round((0.35 + Math.max(0, Math.sin(rad))) * 650 * 1024)
    netTx += Math.round((0.28 + Math.max(0, Math.cos(rad / 1.3))) * 520 * 1024)
    blockRead += Math.round((0.2 + Math.max(0, Math.sin(rad / 1.1))) * 380 * 1024)
    blockWrite += Math.round((0.16 + Math.max(0, Math.cos(rad / 1.6))) * 260 * 1024)

    out.push({
      sampledAt: new Date(t).toISOString(),
      cpuPercent: Number(cpu.toFixed(2)),
      memUsedBytes: memUsed,
      memLimitBytes: memLimit,
      netRxBytes: netRx,
      netTxBytes: netTx,
      blockReadBytes: blockRead,
      blockWriteBytes: blockWrite,
      pids,
      containerCount,
    })
  }

  return out
}

export function buildResourceHistoryPeaks(samples: ServiceResourceSample[]): ServiceResourcePeak[] {
  return samples.map((sample, index) => {
    const previous = samples[index - 1]
    const elapsedSeconds = previous
      ? Math.max(1, (Date.parse(sample.sampledAt) - Date.parse(previous.sampledAt)) / 1000)
      : null
    const rate = (current?: number, prior?: number) =>
      elapsedSeconds && current !== undefined && prior !== undefined
        ? Math.max(0, (current - prior) / elapsedSeconds) * 1.15
        : undefined
    return {
      sampledAt: sample.sampledAt,
      cpuPercent: sample.cpuPercent + 4,
      memUsedBytes: sample.memUsedBytes,
      memLimitBytes: sample.memLimitBytes,
      pids: sample.pids,
      containerCount: sample.containerCount,
      netRxRateBps: rate(sample.netRxBytes, previous?.netRxBytes),
      netTxRateBps: rate(sample.netTxBytes, previous?.netTxBytes),
      blockReadRateBps: rate(sample.blockReadBytes, previous?.blockReadBytes),
      blockWriteRateBps: rate(sample.blockWriteBytes, previous?.blockWriteBytes),
    }
  })
}

export function buildCompactMockJob(job: JobListItem, fixture: Fixture) {
  const summary = isRecord(job.summary) ? job.summary : {}
  const targetVersion = ['targetDisplayTag', 'targetTag', 'to']
    .map((key) => summary[key])
    .find((value): value is string => typeof value === 'string' && value.trim().length > 0)
  const serviceName = job.serviceId
    ? Object.values(fixture.stackById)
        .flatMap((stack) => stack.services)
        .find((service) => service.id === job.serviceId)?.name
    : undefined
  const stackName = job.stackId ? fixture.stackById[job.stackId]?.name : undefined
  const lifecycleAction =
    (job.type === 'service_lifecycle' || job.type === 'stack_lifecycle') && typeof summary.action === 'string'
      ? summary.action
      : ''
  const lifecycleLabel =
    lifecycleAction === 'start' ? '启动任务'
      : lifecycleAction === 'stop' ? '停止任务'
        : lifecycleAction === 'restart' ? '重启任务'
          : undefined
  return {
    id: job.id, type: job.type, scope: job.scope, stackId: job.stackId, serviceId: job.serviceId,
    status: job.status, createdBy: job.createdBy, reason: job.reason, createdAt: job.createdAt,
    startedAt: job.startedAt, finishedAt: job.finishedAt,
    ...(isRecord(summary.progress) ? { progress: summary.progress } : {}),
    ...(job.resultReason ? { resultReason: job.resultReason } : {}),
    displayLabel: lifecycleLabel ?? serviceName ?? stackName ?? job.type,
    ...(targetVersion ? { targetVersion } : {}),
  }
}

export function buildResourceSsePayload(
  serviceId: string,
  samples: ServiceResourceSample[],
  scenario: DockrevApiScenario,
): string {
  const snapshot = samples[samples.length - 1] ?? null
  if (!snapshot) return ': keep-alive\n\n'

  const tick: ServiceResourceSample = {
    ...snapshot,
    sampledAt: new Date().toISOString(),
    cpuPercent: Number((snapshot.cpuPercent + 1.2).toFixed(2)),
    netRxBytes: (snapshot.netRxBytes ?? 0) + 300_000,
    netTxBytes: (snapshot.netTxBytes ?? 0) + 250_000,
    blockReadBytes: (snapshot.blockReadBytes ?? 0) + 160_000,
    blockWriteBytes: (snapshot.blockWriteBytes ?? 0) + 120_000,
    pids: (snapshot.pids ?? 0) + 1,
  }

  const debug = globalThis.__DOCKREV_MOCK_DEBUG__
  if (debug) {
    debug.resourceUsageLastSnapshot = snapshot
    debug.resourceUsageLastTick = tick
  }

  const events: string[] = []
  events.push(`id: 1\nevent: resource_usage_snapshot\ndata: ${JSON.stringify({ serviceId, sample: snapshot })}\n\n`)

  if (scenario === 'service-detail-resource-monitor-stream-error') {
    events.push(
      `id: 2\nevent: resource_usage_error\ndata: ${JSON.stringify({ serviceId, error: 'runtime_stats_unavailable' })}\n\n`,
    )
    return events.join('')
  }

  events.push(`id: 2\nevent: resource_usage_tick\ndata: ${JSON.stringify({ serviceId, sample: tick })}\n\n`)
  return events.join('')
}

export function nowIso(offsetMs = 0) {
  return new Date(Date.now() + offsetMs).toISOString()
}

export function makeMockDebug(): MockDebug {
  return {
    jobsEventsCalls: 0,
    jobsListCalls: 0,
    jobsListUrls: [],
    ghcrReposUrls: [],
    lastUpdateRequest: null,
    lastUpdateUrl: null,
    lastUpdateMethod: null,
    lastLifecycleRequest: null,
    stackDetailCalls: 0,
    stackDetailCallsById: {},
    digestTagsSnapshotCalls: 0,
    digestTagsCalls: 0,
    lastDigestTagsSnapshotUrl: null,
    lastDigestTagsUrl: null,
    versionInferenceRefreshCalls: 0,
    lastVersionInferenceRefreshUrl: null,
    lastVersionInferenceRefreshDigest: null,
    serviceTagSuggestionCalls: 0,
    lastServiceTagSuggestionUrl: null,
    lastComposeTagRequest: null,
    resourceUsageEventSourceCalls: 0,
    resourceUsageEventSourceCloseCalls: 0,
    resourceUsageLastSnapshot: null,
    resourceUsageLastTick: null,
  }
}

export function makeDefaultSettings(): SettingsResponse {
  return {
    backup: {
      enabled: true,
      requireSuccess: true,
      baseDir: '/var/lib/dockrev/backups',
      skipTargetsOverBytes: 104857600,
      storage: {
        mode: 'docker_bind',
        logicalPath: '/var/lib/dockrev/backups',
        resolvedLocation: '/srv/dockrev/data/backups',
        writable: true,
        diagnostic: null,
      },
    },
    resourceMonitor: { enabled: true, sampleIntervalSeconds: 5, retentionDays: 1 },
    schedules: {
      updateCheck: { enabled: false, cron: '*/30 * * * *' },
      ghcrWebhookAudit: { enabled: true, cron: '0 3 * * *' },
    },
    releaseNotes: {
      provider: 'octoRill',
      octoRill: {
        enabled: true,
        apiBaseUrl: 'https://octo.example.com',
        apiKeyMasked: '••••••••••••••••••••',
        apiKey: '••••••••••••••••••••',
        defaultView: 'smart',
      },
    },
    auth: {
      forwardHeaderName: 'X-Forwarded-User',
      groupHeaderName: 'Remote-Groups',
      allowAnonymousInDev: true,
      authorizationMode: 'user_or_group',
      allowedUserMasked: 'al***ce',
      allowedGroupMasked: 'o**s',
      currentUser: 'alice',
      currentGroups: ['o**s'],
      matchedBy: 'user',
    },
    instance: { publicBaseUrl: null },
  }
}

export function makeDefaultNotifications(): NotificationConfig {
  return {
    email: { enabled: false, smtpUrl: null },
    webhook: { enabled: false, url: null },
    telegram: { enabled: false, botToken: null, botTokenConfigured: false, chatId: null },
    webPush: { enabled: false, vapidPublicKey: null, vapidPrivateKey: null, vapidSubject: null },
  }
}

export function makeDefaultGitHubPackagesSettings(): GitHubPackagesSettingsResponse {
  return {
    enabled: false,
    callbackUrl: '',
    targets: [],
    reposTotal: 0,
    reposSelectedTotal: 0,
    patMasked: null,
    secretMasked: null,
  }
}

export function makeDefaultDeployCheckReport(): DeployCheckReportResponse {
  return {
    overall: {
      result: 'pass',
      blockingCheckIds: [],
      summary: 'All required capabilities are available',
    },
    generatedAt: nowIso(),
    checks: [
      {
        id: 'core.docker_engine',
        title: 'Docker 引擎可用',
        group: 'core',
        required: true,
        status: 'pass',
        summary: 'docker daemon reachable',
        impact: '不可用时无法执行更新',
        evidence: 'docker info ok',
        recommendation: '',
      },
      {
        id: 'core.compose_access',
        title: 'Compose 配置可访问',
        group: 'core',
        required: true,
        status: 'pass',
        summary: 'compose paths are readable',
        impact: '服务解析不完整，更新目标不可信',
        evidence: '/srv/app/docker-compose.yml',
        recommendation: '',
      },
      {
        id: 'feature.notifications.webhook',
        title: '通知能力：Webhook',
        group: 'feature',
        required: false,
        status: 'na',
        summary: 'webhook notification is disabled',
        impact: '该功能未启用；不纳入阻塞判定',
        evidence: 'enabled=false',
        recommendation: '',
      },
    ],
  }
}

export function makeDefaultDeployCheckEnvelope(): DeployCheckReportEnvelope {
  return {
    status: 'ready',
    refreshing: false,
    retryAfterMs: null,
    report: makeDefaultDeployCheckReport(),
  }
}

export function makeDefaultDeployWelcome(): DeployWelcomeResponse {
  return {
    // Keep Storybook on existing pages by default.
    neverAutoOpen: true,
    updatedAt: null,
  }
}

export function summarizeVersionInferenceRows(rows: VersionInferenceOverviewRowMock[]) {
  const summary = {
    snapshotsTotal: 0,
    queued: 0,
    running: 0,
    ready: 0,
    stale: 0,
    allFailed: 0,
  }
  for (const row of rows) {
    if (row.checkedAt) summary.snapshotsTotal += 1
    switch (row.status) {
      case 'queued':
        summary.queued += 1
        break
      case 'running':
        summary.running += 1
        break
      case 'ready':
        summary.ready += 1
        break
      case 'stale':
        summary.stale += 1
        break
      case 'all_failed':
        summary.allFailed += 1
        break
      default:
        break
    }
  }
  return summary
}

export function makeVersionInferenceOverview(input?: {
  rows?: VersionInferenceOverviewRowMock[]
  tasks?: VersionInferenceTaskMock[]
  worker?: Partial<VersionInferenceOverviewMock['worker']>
  gc?: Partial<VersionInferenceOverviewMock['gc']>
}): VersionInferenceOverviewMock {
  const rows = input?.rows ?? []
  const tasks = input?.tasks ?? []
  const summary = summarizeVersionInferenceRows(rows)
  const workerBase = {
    maxConcurrency: 4,
    queued: tasks.filter((x) => x.status === 'queued').length,
    running: tasks.filter((x) => x.status === 'running').length,
    inFlight: tasks.filter((x) => x.status === 'queued' || x.status === 'running').length,
  }
  const worker = {
    ...workerBase,
    ...(input?.worker ?? {}),
  }
  return {
    worker,
    gc: {
      retentionDays: 30,
      intervalSeconds: 24 * 60 * 60,
      lastRunAt: nowIso(-5 * 60 * 1000),
      lastDeleted: 3,
      lastDurationMs: 42,
      lastError: null,
      ...(input?.gc ?? {}),
    },
    summary,
    tasks,
    rows,
    page: 1,
    perPage: 50,
    total: rows.length,
  }
}
