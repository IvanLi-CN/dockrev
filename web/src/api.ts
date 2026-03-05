export type StackStatus = 'healthy' | 'degraded' | 'unknown'

export type StackListItem = {
  id: string
  name: string
  status: StackStatus
  services: number
  updates: number
  lastCheckAt: string
  archived?: boolean
  archivedServices?: number
}

export type ComposeConfig = {
  type: string
  composeFiles: string[]
  envFile?: string | null
}

export type ArchMatch = 'match' | 'mismatch' | 'unknown'

export type TernaryChoice = 'inherit' | 'skip' | 'force'

export type BackupTargetOverrides = {
  bindPaths: Record<string, TernaryChoice>
  volumeNames: Record<string, TernaryChoice>
}

export type ServiceSettings = {
  autoRollback: boolean
  backupTargets: BackupTargetOverrides
}

export type ServiceImage = {
  ref: string
  tag: string
  digest?: string | null
  resolvedTag?: string | null
  resolvedTags?: string[] | null
}

export type VersionInferenceState = {
  status: 'ready' | 'pending' | string
  reason?: string | null
  checkedAt?: string | null
}

export type Service = {
  id: string
  name: string
  image: ServiceImage
  candidate?: {
    tag: string
    resolvedTag?: string | null
    digest: string
    archMatch: ArchMatch
    arch: string[]
  } | null
  ignore?: {
    matched: boolean
    ruleId: string
    reason: string
  } | null
  versionInference?: VersionInferenceState | null
  settings: ServiceSettings
  archived?: boolean
}

export type ServiceDigestTagsScanSummary = {
  repoTagsTotal: number
  repoTagsConsidered: number
  manifestsOk: number
  manifestsTimeout: number
  manifestsError: number
}

export type ServiceDigestTagsResponse = {
  digest: string
  tags: string[]
  repoTags?: string[]
  scan: ServiceDigestTagsScanSummary
}

export type ServiceDigestTagsSnapshotResponse = {
  digest: string
  tags: string[]
  checkedAt: string
  scan: ServiceDigestTagsScanSummary
}

export type ServiceDigestTagsSnapshotPendingResponse = {
  status: 'pending'
  digest: string
  retryAfterMs: number
}

export type ServiceDigestTagsSnapshotResult =
  | ServiceDigestTagsSnapshotResponse
  | ServiceDigestTagsSnapshotPendingResponse

export function isServiceDigestTagsSnapshotPending(
  data: ServiceDigestTagsSnapshotResult,
): data is ServiceDigestTagsSnapshotPendingResponse {
  return (data as ServiceDigestTagsSnapshotPendingResponse).status === 'pending'
}

export type StackDetail = {
  id: string
  name: string
  compose: ComposeConfig
  services: Service[]
  archived?: boolean
}

export type DiscoveredProjectStatus = 'active' | 'missing' | 'invalid'

export type DiscoveredProject = {
  project: string
  status: DiscoveredProjectStatus
  stackId?: string | null
  configFiles?: string[] | null
  lastSeenAt?: string | null
  lastScanAt?: string | null
  lastError?: string | null
  archived: boolean
}

export type DiscoveryScanResponse = {
  startedAt: string
  durationMs: number
  summary: {
    projectsSeen: number
    stacksCreated: number
    stacksUpdated: number
    stacksSkipped: number
    stacksFailed: number
    stacksMarkedMissing: number
  }
  actions: Array<{
    project: string
    action: 'created' | 'updated' | 'skipped' | 'failed' | 'marked_missing'
    stackId?: string | null
    reason?: string | null
    details?: unknown
  }>
}

export type TriggerDiscoveryScanJobResponse = {
  jobId: string
}

export type TriggerVersionInferenceRefreshResponse = {
  status: 'pending' | string
  serviceId: string
  imageRepo: string
  reason: string
}

export type VersionInferenceOverviewStatus =
  | 'queued'
  | 'running'
  | 'ready'
  | 'stale'
  | 'all_failed'
  | string

export type VersionInferenceTaskProgress = {
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

export type VersionInferenceTask = {
  key: string
  imageRepo: string
  hostPlatform: string
  status: 'queued' | 'running' | string
  reason: string
  enqueuedAt: string
  startedAt?: string | null
  updatedAt: string
  progress?: VersionInferenceTaskProgress | null
}

export type VersionInferenceCacheRow = {
  key: string
  imageRepo: string
  hostPlatform: string
  status: VersionInferenceOverviewStatus
  serviceCount: number
  reason?: string | null
  checkedAt?: string | null
  updatedAt?: string | null
  progress?: VersionInferenceTaskProgress | null
}

export type VersionInferenceOverviewSummary = {
  snapshotsTotal: number
  queued: number
  running: number
  ready: number
  stale: number
  allFailed: number
}

export type VersionInferenceWorkerState = {
  maxConcurrency: number
  queued: number
  running: number
  inFlight: number
}

export type VersionInferenceGcState = {
  retentionDays: number
  intervalSeconds: number
  lastRunAt?: string | null
  lastDeleted?: number | null
  lastDurationMs?: number | null
  lastError?: string | null
}

export type VersionInferenceOverviewResponse = {
  worker: VersionInferenceWorkerState
  gc: VersionInferenceGcState
  summary: VersionInferenceOverviewSummary
  tasks: VersionInferenceTask[]
  rows: VersionInferenceCacheRow[]
  page: number
  perPage: number
  total: number
}

// Backward-compatible aliases for existing imports.
export type VersionInferenceTaskState = VersionInferenceTask
export type VersionInferenceOverviewRow = VersionInferenceCacheRow

export type GetVersionInferenceOverviewInput = {
  q?: string | null
  status?: string | null
  page?: number
  perPage?: number
}

export type JobListItem = {
  id: string
  type: string
  scope: string
  stackId?: string | null
  serviceId?: string | null
  status: string
  createdBy: string
  reason: string
  createdAt: string
  startedAt?: string | null
  finishedAt?: string | null
  allowArchMismatch: boolean
  backupMode: string
  summary: unknown
  progress?: JobProgress | null
}

export type JobLogLine = {
  ts: string
  level: string
  msg: string
}

export type JobProgress = {
  phase: string
  message: string
  current: number
  total: number
  percent: number
  plannedCurrent?: number | null
  plannedTotal?: number | null
  plannedPercent?: number | null
  currentTarget?: string | null
  updatedAt: string
}

export type JobDetail = JobListItem & { logs: JobLogLine[]; logsLastId: number; progress?: JobProgress | null }

export type IgnoreRule = {
  id: string
  enabled: boolean
  scope: { type: string; serviceId: string }
  match: { kind: string; value: string }
  note?: string | null
}

export type SettingsResponse = {
  backup: {
    enabled: boolean
    requireSuccess: boolean
    baseDir: string
    skipTargetsOverBytes: number
  }
  resourceMonitor: {
    enabled: boolean
    sampleIntervalSeconds: 10 | 30 | 60 | 300
    retentionDays: number
  }
  schedules: {
    updateCheck: { enabled: boolean; cron: string }
    ghcrWebhookAudit: { enabled: boolean; cron: string }
  }
  auth: {
    forwardHeaderName: string
    allowAnonymousInDev: boolean
  }
  instance: {
    // Optional for backward compatibility with older servers.
    publicBaseUrl?: string | null
  }
}

export type PutSettingsInput = {
  backup: SettingsResponse['backup']
  resourceMonitor?: {
    enabled: boolean
    sampleIntervalSeconds: 10 | 30 | 60 | 300
  }
  schedules?: {
    updateCheck?: { enabled: boolean; cron: string }
    ghcrWebhookAudit?: { enabled: boolean; cron: string }
  }
  instance?: {
    // When present, updates the stored public base URL. `null` (or empty string) clears it.
    publicBaseUrl?: string | null
  }
}

export type ServiceResourceUsageWindow = '15m' | '1h' | '6h'

export type ServiceResourceSample = {
  sampledAt: string
  cpuPercent: number
  memUsedBytes?: number
  memLimitBytes?: number
  netRxBytes?: number
  netTxBytes?: number
  blockReadBytes?: number
  blockWriteBytes?: number
  pids?: number
  containerCount: number
}

export type ServiceResourceHistoryResponse = {
  serviceId: string
  window: ServiceResourceUsageWindow | string
  samples: ServiceResourceSample[]
}

export type DeployCheckStatus = 'pass' | 'fail' | 'na'
export type DeployCheckNaReason = 'disabled_by_switch' | 'missing_prerequisite' | 'not_applicable'

export type DeployCheckGroup = 'core' | 'feature' | string

export type DeployCheckItem = {
  id: string
  title: string
  group: DeployCheckGroup
  required: boolean
  status: DeployCheckStatus
  naReason?: DeployCheckNaReason
  summary: string
  impact: string
  evidence: string
  recommendation: string
}

export type DeployCheckReportResponse = {
  overall: {
    result: 'pass' | 'fail'
    blockingCheckIds: string[]
    summary: string
  }
  generatedAt: string
  checks: DeployCheckItem[]
}

export type DeployWelcomeResponse = {
  neverAutoOpen: boolean
  updatedAt?: string | null
}

export type NotificationConfig = {
  email: { enabled: boolean; smtpUrl?: string | null }
  webhook: { enabled: boolean; url?: string | null }
  telegram: { enabled: boolean; botToken?: string | null; botTokenConfigured?: boolean; chatId?: string | null }
  webPush: {
    enabled: boolean
    vapidPublicKey?: string | null
    vapidPrivateKey?: string | null
    vapidSubject?: string | null
  }
}

export type NotificationTestChannel = 'email' | 'webhook' | 'telegram' | 'webPush'

export type NotificationChannelTestResult = {
  ok: boolean
  error?: string
}

export type TestNotificationsResponse = {
  ok: boolean
  results: Partial<Record<NotificationTestChannel, NotificationChannelTestResult>>
}

export type GitHubPackagesTarget = {
  input: string
  kind: 'repo' | 'owner' | string
  owner: string
  warnings: string[]
}

export type GitHubPackagesRepo = {
  fullName: string
  selected: boolean
  webhookState?: 'unknown' | 'queued' | 'running' | 'ok' | 'missing' | 'error' | 'conflict' | string | null
  webhookJobId?: string | null
  hookId?: number | null
  lastSyncAt?: string | null
  lastAuditAt?: string | null
  lastOp?: 'register' | 'unregister' | 'audit' | 'audit_all' | 'sync_all' | 'sync_repo' | string | null
  lastError?: string | null
}

export type GitHubPackagesSettingsResponse = {
  enabled: boolean
  callbackUrl: string
  targets: GitHubPackagesTarget[]
  reposTotal: number
  reposSelectedTotal: number
  patMasked?: string | null
  secretMasked?: string | null
}

export type PutGitHubPackagesSettingsRequest = {
  enabled: boolean
  callbackUrl: string
  targets?: Array<{ input: string }> | null
  repos?: Array<{ fullName: string; selected: boolean }> | null
  pat?: string | null
}

export type ResolveGitHubPackagesTargetResponse = {
  kind: 'repo' | 'owner' | string
  owner: string
  repos: Array<{
    fullName: string
    selected: boolean
    visibility?: 'public' | 'private' | 'unknown' | string
    lastActivityAt?: string | null
  }>
  warnings: string[]
}

export type SyncGitHubPackagesWebhookResult = {
  repo: string
  action: 'noop' | 'created' | 'updated' | 'conflict' | 'error' | string
  hookId?: number | null
  conflictHooks?: Array<{ id: number; url: string; events: string[]; active: boolean }> | null
  message?: string | null
}

export type SyncGitHubPackagesWebhooksRequest = {
  dryRun?: boolean
  resolveConflicts?: Array<{ repo: string; keepHookId: number; deleteHookIds: number[] }>
  repos?: string[] | null
}

export type SyncGitHubPackagesWebhooksResponse = {
  ok: boolean
  results: SyncGitHubPackagesWebhookResult[]
}

export type TriggerGitHubPackagesWebhookSyncResponse = {
  ok: boolean
  jobId: string
  status: 'queued' | 'running' | string
  reused: boolean
}

export type ListGitHubPackagesReposResponse = {
  page: number
  perPage: number
  total: number
  filteredTotal: number
  selectedTotal: number
  repos: GitHubPackagesRepo[]
}

export type SetGitHubPackagesRepoSelectedRequest = {
  fullName: string
  selected: boolean
}

export type SetGitHubPackagesRepoSelectedResponse = {
  ok: boolean
  jobId?: string | null
}

export type BulkSetGitHubPackagesReposSelectedRequest = {
  q?: string | null
  selectedFilter?: 'all' | 'selected' | 'unselected' | string | null
  selected: boolean
}

export type BulkSetGitHubPackagesReposSelectedResponse = {
  ok: boolean
  affected: number
}

export type DeleteGitHubPackagesRepoRequest = {
  fullName: string
}

export type DeleteGitHubPackagesRepoResponse = {
  ok: boolean
  jobId: string
}

export type GitHubPackagesWebhookOverviewResponse = {
  summary: {
    tracked: number
    ok: number
    missing: number
    error: number
    conflict: number
    queued: number
    running: number
    unknown: number
  }
  jobsQueued: number
  jobsRunning: number
  runningJobId?: string | null
  lastAuditAt?: string | null
}

export type GitHubPackagesWebhookDelivery = {
  deliveryId: string
  receivedAt: string
  firstReceivedAt: string
  owner?: string | null
  repo?: string | null
  fullName?: string | null
  event?: string | null
  action?: string | null
  decision: 'processed' | 'ignored' | 'rejected' | string
  reason?: string | null
  responseStatus?: number | null
  jobId?: string | null
  attemptCount: number
}

export type ListGitHubPackagesWebhookDeliveriesResponse = {
  page: number
  perPage: number
  total: number
  filteredTotal: number
  summary: {
    processed: number
    ignored: number
    rejected: number
  }
  deliveries: GitHubPackagesWebhookDelivery[]
}

export type AddGitHubPackagesTargetRequest = {
  input: string
}

export type AddGitHubPackagesTargetResponse = {
  ok: boolean
  kind: 'repo' | 'owner' | string
  owner: string
  reposAdded: number
}

export type RemoveGitHubPackagesTargetRequest = {
  input: string
}

export type RemoveGitHubPackagesTargetResponse = {
  ok: boolean
}

const API_BASE = import.meta.env.VITE_API_BASE_URL ?? ''

export function apiBaseUrl(): string {
  return API_BASE
}

export class ApiError extends Error {
  readonly status: number
  readonly code?: string
  readonly details?: unknown
  readonly bodyText?: string

  constructor(input: { status: number; message: string; code?: string; details?: unknown; bodyText?: string }) {
    super(input.message)
    this.name = 'ApiError'
    this.status = input.status
    this.code = input.code
    this.details = input.details
    this.bodyText = input.bodyText
  }
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null
}

async function apiFetch(path: string, init?: RequestInit) {
  const resp = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers || {}),
    },
  })

  if (!resp.ok) {
    const contentType = resp.headers.get('content-type') ?? ''
    const text = await resp.text().catch(() => '')

    if (contentType.includes('application/json')) {
      try {
        const parsed = (text ? JSON.parse(text) : null) as unknown
        const err = isRecord(parsed) && isRecord(parsed.error) ? parsed.error : null
        const code = err && typeof err.code === 'string' ? err.code : undefined
        const message =
          err && typeof err.message === 'string'
            ? err.message
            : text || resp.statusText || `HTTP ${resp.status}`
        const details = err ? (err.details as unknown) : undefined
        throw new ApiError({ status: resp.status, code, message, details, bodyText: text || undefined })
      } catch (e) {
        if (e instanceof ApiError) throw e
        // fall through to plain text error for invalid/unexpected JSON
      }
    }

    throw new ApiError({
      status: resp.status,
      message: text || resp.statusText || `HTTP ${resp.status}`,
      bodyText: text || undefined,
    })
  }
  return resp
}

export async function getDockrevVersion(): Promise<string> {
  const resp = await apiFetch('/api/version')
  const data = (await resp.json()) as unknown
  if (!isRecord(data) || typeof data.version !== 'string' || !data.version.trim()) {
    throw new Error('invalid /api/version response')
  }
  return data.version
}

export async function listStacks(): Promise<StackListItem[]> {
  const resp = await apiFetch('/api/stacks')
  const data = await resp.json()
  return data.stacks as StackListItem[]
}

export async function listStacksArchived(filter: 'exclude' | 'include' | 'only'): Promise<StackListItem[]> {
  const resp = await apiFetch(`/api/stacks?archived=${encodeURIComponent(filter)}`)
  const data = await resp.json()
  return data.stacks as StackListItem[]
}

export async function getStack(stackId: string): Promise<StackDetail> {
  const resp = await apiFetch(`/api/stacks/${encodeURIComponent(stackId)}`)
  const data = await resp.json()
  return data.stack as StackDetail
}

export async function triggerDiscoveryScan(): Promise<TriggerDiscoveryScanJobResponse> {
  const resp = await apiFetch('/api/discovery/scan', { method: 'POST', body: '{}' })
  return (await resp.json()) as TriggerDiscoveryScanJobResponse
}

export async function listDiscoveryProjects(filter: 'exclude' | 'include' | 'only' = 'exclude'): Promise<DiscoveredProject[]> {
  const resp = await apiFetch(`/api/discovery/projects?archived=${encodeURIComponent(filter)}`)
  const data = await resp.json()
  return data.projects as DiscoveredProject[]
}

export async function archiveDiscoveredProject(project: string) {
  await apiFetch(`/api/discovery/projects/${encodeURIComponent(project)}/archive`, { method: 'POST', body: '{}' })
}

export async function restoreDiscoveredProject(project: string) {
  await apiFetch(`/api/discovery/projects/${encodeURIComponent(project)}/restore`, { method: 'POST', body: '{}' })
}

export async function archiveStack(stackId: string) {
  await apiFetch(`/api/stacks/${encodeURIComponent(stackId)}/archive`, { method: 'POST', body: '{}' })
}

export async function restoreStack(stackId: string) {
  await apiFetch(`/api/stacks/${encodeURIComponent(stackId)}/restore`, { method: 'POST', body: '{}' })
}

export async function archiveService(serviceId: string) {
  await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/archive`, { method: 'POST', body: '{}' })
}

export async function restoreService(serviceId: string) {
  await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/restore`, { method: 'POST', body: '{}' })
}

export async function listServiceDigestTags(serviceId: string, digest: string): Promise<ServiceDigestTagsResponse> {
  const resp = await apiFetch(
    `/api/services/${encodeURIComponent(serviceId)}/digest-tags?digest=${encodeURIComponent(digest)}`,
  )
  return (await resp.json()) as ServiceDigestTagsResponse
}

export async function getServiceDigestTagsSnapshot(serviceId: string, digest: string): Promise<ServiceDigestTagsSnapshotResult> {
  const resp = await apiFetch(
    `/api/services/${encodeURIComponent(serviceId)}/digest-tags-snapshot?digest=${encodeURIComponent(digest)}`,
  )
  return (await resp.json()) as ServiceDigestTagsSnapshotResult
}

export async function forceRefreshServiceVersionInference(
  serviceId: string,
): Promise<TriggerVersionInferenceRefreshResponse> {
  const resp = await apiFetch(
    `/api/services/${encodeURIComponent(serviceId)}/version-inference/refresh`,
    { method: 'POST', body: '{}' },
  )
  return (await resp.json()) as TriggerVersionInferenceRefreshResponse
}

export async function getVersionInferenceOverview(
  input: GetVersionInferenceOverviewInput = {},
): Promise<VersionInferenceOverviewResponse> {
  const sp = new URLSearchParams()
  const q = (input.q ?? '').trim()
  const status = (input.status ?? '').trim()

  if (q) sp.set('q', q)
  if (status) sp.set('status', status)
  if (typeof input.page === 'number' && Number.isFinite(input.page)) {
    sp.set('page', String(Math.max(1, Math.round(input.page))))
  }
  if (typeof input.perPage === 'number' && Number.isFinite(input.perPage)) {
    sp.set('perPage', String(Math.max(1, Math.round(input.perPage))))
  }

  const query = sp.toString()
  const resp = await apiFetch(`/api/version-inference/overview${query ? `?${query}` : ''}`)
  return (await resp.json()) as VersionInferenceOverviewResponse
}

export async function triggerCheck(scope: string, stackId?: string, serviceId?: string) {
  const resp = await apiFetch('/api/checks', {
    method: 'POST',
    body: JSON.stringify({ scope, stackId, serviceId, reason: 'ui' }),
  })
  return (await resp.json()) as { checkId: string }
}

export async function triggerRuntimeScan(scope: string, stackId?: string, serviceId?: string) {
  const resp = await apiFetch('/api/runtime-scans', {
    method: 'POST',
    body: JSON.stringify({ scope, stackId, serviceId, reason: 'ui' }),
  })
  return (await resp.json()) as { jobId: string }
}

export function jobEventsUrl(jobId: string, opts?: { afterId?: number }): string {
  const base = apiBaseUrl().replace(/\/$/, '')
  let url = `${base}/api/jobs/${encodeURIComponent(jobId)}/events`
  if (opts && typeof opts.afterId === 'number' && Number.isFinite(opts.afterId)) {
    url += `?afterId=${encodeURIComponent(String(opts.afterId))}`
  }
  return url
}

export function newJobEventsSource(jobId: string, opts?: { afterId?: number }): EventSource {
  return new EventSource(jobEventsUrl(jobId, opts), { withCredentials: true })
}

export function jobsEventsUrl(opts?: { afterId?: number }): string {
  const base = apiBaseUrl().replace(/\/$/, '')
  let url = `${base}/api/jobs/events`
  if (opts && typeof opts.afterId === 'number' && Number.isFinite(opts.afterId)) {
    url += `?afterId=${encodeURIComponent(String(opts.afterId))}`
  }
  return url
}

export function newJobsEventsSource(opts?: { afterId?: number }): EventSource {
  return new EventSource(jobsEventsUrl(opts), { withCredentials: true })
}

export function versionInferenceEventsUrl(opts?: { afterId?: number }): string {
  const base = apiBaseUrl().replace(/\/$/, '')
  let url = `${base}/api/version-inference/events`
  if (opts && typeof opts.afterId === 'number' && Number.isFinite(opts.afterId)) {
    url += `?afterId=${encodeURIComponent(String(opts.afterId))}`
  }
  return url
}

export function newVersionInferenceEventsSource(opts?: { afterId?: number }): EventSource {
  return new EventSource(versionInferenceEventsUrl(opts), { withCredentials: true })
}

export async function getServiceResourceUsageHistory(
  serviceId: string,
  window: ServiceResourceUsageWindow,
): Promise<ServiceResourceHistoryResponse> {
  const query = new URLSearchParams({ window })
  const resp = await apiFetch(
    `/api/services/${encodeURIComponent(serviceId)}/resource-usage/history?${query.toString()}`,
  )
  return (await resp.json()) as ServiceResourceHistoryResponse
}

export function serviceResourceUsageEventsUrl(serviceId: string): string {
  const base = apiBaseUrl().replace(/\/$/, '')
  return `${base}/api/services/${encodeURIComponent(serviceId)}/resource-usage/events`
}

export function newServiceResourceUsageEventsSource(serviceId: string): EventSource {
  return new EventSource(serviceResourceUsageEventsUrl(serviceId), { withCredentials: true })
}

type TriggerUpdateCommonInput = {
  mode: 'apply' | 'dry-run'
  allowArchMismatch: boolean
  backupMode: 'inherit' | 'skip' | 'force'
}

export type TriggerUpdateInput =
  | (TriggerUpdateCommonInput & {
      scope: 'service'
      serviceId: string
      stackId?: string
      targetTag: string
      targetDigest: string
    })
  | (TriggerUpdateCommonInput & {
      scope: 'stack'
      stackId: string
      serviceId?: never
      targetTag?: never
      targetDigest?: never
    })
  | (TriggerUpdateCommonInput & {
      scope: 'all'
      stackId?: never
      serviceId?: never
      targetTag?: never
      targetDigest?: never
    })

export async function triggerUpdate(input: TriggerUpdateInput) {
  const resp = await apiFetch('/api/updates', {
    method: 'POST',
    body: JSON.stringify({ ...input, reason: 'ui' }),
  })
  return (await resp.json()) as { jobId: string }
}

export async function listJobs(): Promise<JobListItem[]> {
  const resp = await apiFetch('/api/jobs')
  const data = await resp.json()
  return data.jobs as JobListItem[]
}

export async function getJob(jobId: string): Promise<JobDetail> {
  const resp = await apiFetch(`/api/jobs/${encodeURIComponent(jobId)}`)
  const data = await resp.json()
  return data.job as JobDetail
}

export async function listIgnores(): Promise<IgnoreRule[]> {
  const resp = await apiFetch('/api/ignores')
  const data = await resp.json()
  return data.rules as IgnoreRule[]
}

export async function createIgnore(input: {
  enabled: boolean
  serviceId: string
  kind: string
  value: string
  note?: string
}) {
  const resp = await apiFetch('/api/ignores', {
    method: 'POST',
    body: JSON.stringify({
      enabled: input.enabled,
      scope: { type: 'service', serviceId: input.serviceId },
      match: { kind: input.kind, value: input.value },
      note: input.note || null,
    }),
  })
  return (await resp.json()) as { ruleId: string }
}

export async function deleteIgnore(ruleId: string) {
  const resp = await apiFetch('/api/ignores', {
    method: 'DELETE',
    body: JSON.stringify({ ruleId }),
  })
  return (await resp.json()) as { deleted: boolean }
}

export async function getSettings(): Promise<SettingsResponse> {
  const resp = await apiFetch('/api/settings')
  return (await resp.json()) as SettingsResponse
}

export async function putSettings(input: PutSettingsInput) {
  const resp = await apiFetch('/api/settings', {
    method: 'PUT',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as { ok: boolean }
}

export async function getDeployCheckReport(): Promise<DeployCheckReportResponse> {
  const resp = await apiFetch('/api/deploy-check/report')
  return (await resp.json()) as DeployCheckReportResponse
}

export async function getDeployWelcome(): Promise<DeployWelcomeResponse> {
  const resp = await apiFetch('/api/deploy-welcome')
  return (await resp.json()) as DeployWelcomeResponse
}

export async function putDeployWelcome(input: { neverAutoOpen: boolean }): Promise<DeployWelcomeResponse & { ok: boolean }> {
  const resp = await apiFetch('/api/deploy-welcome', {
    method: 'PUT',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as DeployWelcomeResponse & { ok: boolean }
}

export async function getNotifications(): Promise<NotificationConfig> {
  const resp = await apiFetch('/api/notifications')
  return (await resp.json()) as NotificationConfig
}

export async function putNotifications(input: NotificationConfig) {
  const resp = await apiFetch('/api/notifications', {
    method: 'PUT',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as { ok: boolean }
}

export async function getGitHubPackagesSettings(): Promise<GitHubPackagesSettingsResponse> {
  const resp = await apiFetch('/api/github-packages/settings')
  return (await resp.json()) as GitHubPackagesSettingsResponse
}

export async function putGitHubPackagesSettings(input: PutGitHubPackagesSettingsRequest) {
  const resp = await apiFetch('/api/github-packages/settings', {
    method: 'PUT',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as { ok: boolean }
}

export async function resolveGitHubPackagesTarget(input: string): Promise<ResolveGitHubPackagesTargetResponse> {
  const resp = await apiFetch('/api/github-packages/resolve', {
    method: 'POST',
    body: JSON.stringify({ input }),
  })
  return (await resp.json()) as ResolveGitHubPackagesTargetResponse
}

export async function syncGitHubPackagesWebhooks(
  input: SyncGitHubPackagesWebhooksRequest,
): Promise<SyncGitHubPackagesWebhooksResponse> {
  const resp = await apiFetch('/api/github-packages/sync', {
    method: 'POST',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as SyncGitHubPackagesWebhooksResponse
}

export async function triggerGitHubPackagesWebhookSyncAll(): Promise<TriggerGitHubPackagesWebhookSyncResponse> {
  const resp = await apiFetch('/api/github-packages/webhook/sync-all', {
    method: 'POST',
  })
  return (await resp.json()) as TriggerGitHubPackagesWebhookSyncResponse
}

export async function triggerGitHubPackagesWebhookSyncRepo(input: {
  fullName: string
}): Promise<TriggerGitHubPackagesWebhookSyncResponse> {
  const resp = await apiFetch('/api/github-packages/webhook/sync-repo', {
    method: 'POST',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as TriggerGitHubPackagesWebhookSyncResponse
}

export async function listGitHubPackagesRepos(input: {
  page: number
  perPage: number
  q?: string | null
  selectedFilter?: 'all' | 'selected' | 'unselected' | string | null
}): Promise<ListGitHubPackagesReposResponse> {
  const sp = new URLSearchParams()
  sp.set('page', String(input.page))
  sp.set('perPage', String(input.perPage))
  if (input.q) sp.set('q', input.q)
  if (input.selectedFilter && input.selectedFilter !== 'all') sp.set('selectedFilter', input.selectedFilter)
  const resp = await apiFetch(`/api/github-packages/repos?${sp.toString()}`)
  return (await resp.json()) as ListGitHubPackagesReposResponse
}

export async function getGitHubPackagesWebhookOverview(): Promise<GitHubPackagesWebhookOverviewResponse> {
  const resp = await apiFetch('/api/github-packages/webhook/overview')
  return (await resp.json()) as GitHubPackagesWebhookOverviewResponse
}

export async function listGitHubPackagesWebhookDeliveries(input: {
  page: number
  perPage: number
  decision?: 'all' | 'processed' | 'ignored' | 'rejected' | string | null
  q?: string | null
}): Promise<ListGitHubPackagesWebhookDeliveriesResponse> {
  const sp = new URLSearchParams()
  sp.set('page', String(input.page))
  sp.set('perPage', String(input.perPage))
  if (input.decision && input.decision !== 'all') sp.set('decision', input.decision)
  if (input.q && input.q.trim()) sp.set('q', input.q.trim())
  const resp = await apiFetch(`/api/github-packages/webhook/deliveries?${sp.toString()}`)
  return (await resp.json()) as ListGitHubPackagesWebhookDeliveriesResponse
}

export async function setGitHubPackagesRepoSelected(input: SetGitHubPackagesRepoSelectedRequest) {
  const resp = await apiFetch('/api/github-packages/repos/selected', {
    method: 'POST',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as SetGitHubPackagesRepoSelectedResponse
}

export async function deleteGitHubPackagesRepo(input: DeleteGitHubPackagesRepoRequest) {
  const resp = await apiFetch('/api/github-packages/repos/delete', {
    method: 'POST',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as DeleteGitHubPackagesRepoResponse
}

export async function bulkSetGitHubPackagesReposSelected(input: BulkSetGitHubPackagesReposSelectedRequest) {
  const resp = await apiFetch('/api/github-packages/repos/bulk-selected', {
    method: 'POST',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as BulkSetGitHubPackagesReposSelectedResponse
}

export async function addGitHubPackagesTarget(input: AddGitHubPackagesTargetRequest) {
  const resp = await apiFetch('/api/github-packages/targets/add', {
    method: 'POST',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as AddGitHubPackagesTargetResponse
}

export async function removeGitHubPackagesTarget(input: RemoveGitHubPackagesTargetRequest) {
  const resp = await apiFetch('/api/github-packages/targets/remove', {
    method: 'POST',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as RemoveGitHubPackagesTargetResponse
}

export async function testNotifications(input?: {
  message?: string | null
  channel?: NotificationTestChannel
}): Promise<TestNotificationsResponse> {
  const resp = await apiFetch('/api/notifications/test', {
    method: 'POST',
    body: JSON.stringify({
      message: input?.message ?? null,
      channel: input?.channel ?? null,
    }),
  })
  return (await resp.json()) as TestNotificationsResponse
}

export async function createWebPushSubscription(input: { endpoint: string; keys: { p256dh: string; auth: string } }) {
  const resp = await apiFetch('/api/web-push/subscriptions', {
    method: 'POST',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as { ok: boolean }
}

export async function deleteWebPushSubscription(endpoint: string) {
  const resp = await apiFetch('/api/web-push/subscriptions', {
    method: 'DELETE',
    body: JSON.stringify({ endpoint }),
  })
  return (await resp.json()) as { ok: boolean }
}

export async function getServiceSettings(serviceId: string): Promise<ServiceSettings> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/settings`)
  return (await resp.json()) as ServiceSettings
}

export async function putServiceSettings(serviceId: string, settings: ServiceSettings) {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/settings`, {
    method: 'PUT',
    body: JSON.stringify(settings),
  })
  return (await resp.json()) as { ok: boolean }
}
