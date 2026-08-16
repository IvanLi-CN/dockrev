export * from './api/types'

import type {
  StackListItem,
  StackSettings,
  ServiceSettings,
  ServiceBackupRecordsResponse,
  ServiceBackupTargetsResponse,
  PutServiceBackupTargetsRequest,
  PutServiceBackupTargetsResponse,
  ServiceDigestTagsSnapshotResult,
  StackDetail,
  ServiceRepoLinkInferenceResponse,
  ServiceTagSuggestionsResponse,
  ServiceLogSnapshotResponse,
  PutServiceComposeTagResponse,
  DiscoveredProject,
  TriggerDiscoveryScanJobResponse,
  TriggerVersionInferenceRefreshResponse,
  NewVersionDiscoveryTimelineResponse,
  ServiceGitHubReleasesResponse,
  ServiceReleaseNotesDirection,
  ServiceReleaseNotesResponse,
  VersionInferenceOverviewResponse,
  GetVersionInferenceOverviewInput,
  JobListItem,
  CompactJobListItem,
  ListJobsInput,
  ListJobsResponse,
  ListCompactJobsResponse,
  JobDetail,
  IgnoreRule,
  SettingsResponse,
  PutSettingsInput,
  ServiceResourceUsageWindow,
  ServiceResourceHistoryResponse,
  HomepageNavResponse,
  ServiceResourceOverviewResponse,
  DeployCheckReportEnvelope,
  DeployWelcomeResponse,
  CleanupScanRequest,
  CleanupApplyRequest,
  CleanupApplyResponse,
  CleanupScanResponse,
  CleanupScanRunStartResponse,
  NotificationConfig,
  NotificationTestChannel,
  TestNotificationsResponse,
  GitHubPackagesSettingsResponse,
  PutGitHubPackagesSettingsRequest,
  ResolveGitHubPackagesTargetResponse,
  SyncGitHubPackagesWebhooksRequest,
  SyncGitHubPackagesWebhooksResponse,
  TriggerGitHubPackagesWebhookSyncResponse,
  ListGitHubPackagesReposResponse,
  SetGitHubPackagesRepoSelectedRequest,
  SetGitHubPackagesRepoSelectedResponse,
  BulkSetGitHubPackagesReposSelectedRequest,
  BulkSetGitHubPackagesReposSelectedResponse,
  DeleteGitHubPackagesRepoRequest,
  DeleteGitHubPackagesRepoResponse,
  GitHubPackagesWebhookOverviewResponse,
  ListGitHubPackagesWebhookDeliveriesResponse,
  AddGitHubPackagesTargetRequest,
  AddGitHubPackagesTargetResponse,
  RemoveGitHubPackagesTargetRequest,
  RemoveGitHubPackagesTargetResponse,
  ServiceLifecycleState
} from './api/types'

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
export type AuthRequiredDetails = {
  reason?: string
  message?: string
  forwardHeaderName?: string
  groupHeaderName?: string
  authorizationMode?: string
  allowedUserMasked?: string | null
  allowedGroupMasked?: string | null
  currentUser?: string | null
  currentGroups?: string[]
  avatarUrl?: string | null
}

export const AUTH_REQUIRED_EVENT = 'dockrev:auth-required'
export const AUTH_RECOVERED_EVENT = 'dockrev:auth-recovered'

export function isAnonymousPublicApiRequest(path: string, method = 'GET'): boolean {
  const normalizedPath = path.split('?')[0] ?? path
  const upperMethod = method.toUpperCase()
  if (upperMethod === 'GET' && (normalizedPath === '/api/health' || normalizedPath === '/api/version')) {
    return true
  }
  return normalizedPath.startsWith('/api/webhooks/')
}

export function asAuthRequiredDetails(details: unknown): AuthRequiredDetails | null {
  if (!isRecord(details)) return null
  const currentGroups = Array.isArray(details.currentGroups)
    ? details.currentGroups.filter((value): value is string => typeof value === 'string')
    : undefined
  return {
    reason: typeof details.reason === 'string' ? details.reason : undefined,
    message: typeof details.message === 'string' ? details.message : undefined,
    forwardHeaderName: typeof details.forwardHeaderName === 'string' ? details.forwardHeaderName : undefined,
    groupHeaderName: typeof details.groupHeaderName === 'string' ? details.groupHeaderName : undefined,
    authorizationMode: typeof details.authorizationMode === 'string' ? details.authorizationMode : undefined,
    allowedUserMasked:
      typeof details.allowedUserMasked === 'string' || details.allowedUserMasked === null
        ? (details.allowedUserMasked as string | null)
        : undefined,
    allowedGroupMasked:
      typeof details.allowedGroupMasked === 'string' || details.allowedGroupMasked === null
        ? (details.allowedGroupMasked as string | null)
        : undefined,
    currentUser:
      typeof details.currentUser === 'string' || details.currentUser === null
        ? (details.currentUser as string | null)
        : undefined,
    currentGroups,
    avatarUrl:
      typeof details.avatarUrl === 'string' || details.avatarUrl === null ? (details.avatarUrl as string | null) : undefined,
  }
}

function dispatchAuthRequired(error: ApiError) {
  if (typeof window === 'undefined') return
  if (error.status !== 401 || error.code !== 'auth_required') return
  window.dispatchEvent(
    new CustomEvent(AUTH_REQUIRED_EVENT, {
      detail: {
        status: error.status,
        code: error.code,
        message: error.message,
        details: asAuthRequiredDetails(error.details),
      },
    }),
  )
}

function dispatchAuthRecovered() {
  if (typeof window === 'undefined') return
  window.dispatchEvent(new CustomEvent(AUTH_RECOVERED_EVENT))
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
        const apiError = new ApiError({ status: resp.status, code, message, details, bodyText: text || undefined })
        dispatchAuthRequired(apiError)
        throw apiError
      } catch (e) {
        if (e instanceof ApiError) throw e
        // fall through to plain text error for invalid/unexpected JSON
      }
    }

    const apiError = new ApiError({
      status: resp.status,
      message: text || resp.statusText || `HTTP ${resp.status}`,
      bodyText: text || undefined,
    })
    dispatchAuthRequired(apiError)
    throw apiError
  }
  if (!isAnonymousPublicApiRequest(path, init?.method ?? 'GET')) {
    dispatchAuthRecovered()
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

export async function getStackSettings(stackId: string): Promise<StackSettings> {
  const resp = await apiFetch(`/api/stacks/${encodeURIComponent(stackId)}/settings`)
  return (await resp.json()) as StackSettings
}

export async function putStackSettings(stackId: string, settings: StackSettings) {
  const resp = await apiFetch(`/api/stacks/${encodeURIComponent(stackId)}/settings`, {
    method: 'PUT',
    body: JSON.stringify(settings),
  })
  return (await resp.json()) as { ok: boolean }
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

export async function listServiceDigestTags(serviceId: string, digest: string): Promise<ServiceDigestTagsSnapshotResult> {
  const resp = await apiFetch(
    `/api/services/${encodeURIComponent(serviceId)}/digest-tags?digest=${encodeURIComponent(digest)}`,
  )
  return (await resp.json()) as ServiceDigestTagsSnapshotResult
}

export async function getServiceDigestTagsSnapshot(serviceId: string, digest: string): Promise<ServiceDigestTagsSnapshotResult> {
  const resp = await apiFetch(
    `/api/services/${encodeURIComponent(serviceId)}/digest-tags-snapshot?digest=${encodeURIComponent(digest)}`,
  )
  return (await resp.json()) as ServiceDigestTagsSnapshotResult
}

export async function forceRefreshServiceVersionInference(
  serviceId: string,
  digest: string,
): Promise<TriggerVersionInferenceRefreshResponse> {
  const resp = await apiFetch(
    `/api/services/${encodeURIComponent(serviceId)}/version-inference/refresh`,
    { method: 'POST', body: JSON.stringify({ digest }) },
  )
  return (await resp.json()) as TriggerVersionInferenceRefreshResponse
}

export async function getServiceNewVersionDiscoveryTimeline(
  serviceId: string,
): Promise<NewVersionDiscoveryTimelineResponse> {
  const resp = await apiFetch(
    `/api/services/${encodeURIComponent(serviceId)}/new-version-discovery-timeline`,
  )
  return (await resp.json()) as NewVersionDiscoveryTimelineResponse
}

export async function getServiceGitHubReleases(
  serviceId: string,
  input: { page?: number; perPage?: number } = {},
): Promise<ServiceGitHubReleasesResponse> {
  const sp = new URLSearchParams()
  if (typeof input.page === 'number' && Number.isFinite(input.page)) {
    sp.set('page', String(Math.max(1, Math.round(input.page))))
  }
  if (typeof input.perPage === 'number' && Number.isFinite(input.perPage)) {
    sp.set('perPage', String(Math.max(1, Math.round(input.perPage))))
  }
  const query = sp.toString()
  const resp = await apiFetch(
    `/api/services/${encodeURIComponent(serviceId)}/github-releases${query ? `?${query}` : ''}`,
  )
  return (await resp.json()) as ServiceGitHubReleasesResponse
}

export async function getServiceReleaseNotes(
  serviceId: string,
  input: { cursor?: string | null; direction?: ServiceReleaseNotesDirection; limit?: number } = {},
): Promise<ServiceReleaseNotesResponse> {
  const sp = new URLSearchParams()
  const cursor = input.cursor?.trim()
  if (cursor) sp.set('cursor', cursor)
  if (input.direction) sp.set('direction', input.direction)
  if (typeof input.limit === 'number' && Number.isFinite(input.limit)) {
    sp.set('limit', String(Math.max(1, Math.round(input.limit))))
  }
  const query = sp.toString()
  const resp = await apiFetch(
    `/api/services/${encodeURIComponent(serviceId)}/release-notes${query ? `?${query}` : ''}`,
  )
  return (await resp.json()) as ServiceReleaseNotesResponse
}

export async function locateServiceReleaseNotes(
  serviceId: string,
  input: { version: string; limit?: number },
): Promise<ServiceReleaseNotesResponse> {
  const sp = new URLSearchParams()
  const version = input.version.trim()
  if (version) sp.set('version', version)
  if (typeof input.limit === 'number' && Number.isFinite(input.limit)) {
    sp.set('limit', String(Math.max(1, Math.round(input.limit))))
  }
  const query = sp.toString()
  const resp = await apiFetch(
    `/api/services/${encodeURIComponent(serviceId)}/release-notes/locate${query ? `?${query}` : ''}`,
  )
  return (await resp.json()) as ServiceReleaseNotesResponse
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

export async function scanCleanups(input: CleanupScanRequest): Promise<CleanupScanResponse> {
  const resp = await apiFetch('/api/cleanups/scan', {
    method: 'POST',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as CleanupScanResponse
}

export async function startCleanupScanRun(input: CleanupScanRequest): Promise<CleanupScanRunStartResponse> {
  const resp = await apiFetch('/api/cleanups/scan-runs', {
    method: 'POST',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as CleanupScanRunStartResponse
}

export function cleanupScanRunEventsUrl(scanId: string, opts?: { afterId?: number }): string {
  const base = apiBaseUrl().replace(/\/$/, '')
  const params = new URLSearchParams()
  if (opts?.afterId != null && opts.afterId > 0) params.set('afterId', String(opts.afterId))
  const suffix = params.toString()
  return `${base}/api/cleanups/scan-runs/${encodeURIComponent(scanId)}/events${suffix ? `?${suffix}` : ''}`
}

export async function applyCleanups(input: CleanupApplyRequest): Promise<CleanupApplyResponse> {
  const resp = await apiFetch('/api/cleanups/apply', {
    method: 'POST',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as CleanupApplyResponse
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

export function githubPackagesWebhookDeliveriesEventsUrl(opts?: { afterId?: number }): string {
  const base = apiBaseUrl().replace(/\/$/, '')
  let url = `${base}/api/github-packages/webhook/deliveries/events`
  if (opts && typeof opts.afterId === 'number' && Number.isFinite(opts.afterId)) {
    url += `?afterId=${encodeURIComponent(String(opts.afterId))}`
  }
  return url
}

export function newGitHubPackagesWebhookDeliveriesEventsSource(opts?: { afterId?: number }): EventSource {
  return new EventSource(githubPackagesWebhookDeliveriesEventsUrl(opts), { withCredentials: true })
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

export async function getServiceLogs(serviceId: string, tail = 500): Promise<ServiceLogSnapshotResponse> {
  const query = new URLSearchParams({ tail: String(tail) })
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/logs?${query.toString()}`)
  return (await resp.json()) as ServiceLogSnapshotResponse
}

export function serviceLogsEventsUrl(serviceId: string, opts?: { afterId?: number }): string {
  const base = apiBaseUrl().replace(/\/$/, '')
  let url = `${base}/api/services/${encodeURIComponent(serviceId)}/logs/events`
  if (opts && typeof opts.afterId === 'number' && Number.isFinite(opts.afterId)) {
    url += `?afterId=${encodeURIComponent(String(opts.afterId))}`
  }
  return url
}

export function newServiceLogsEventsSource(serviceId: string, opts?: { afterId?: number }): EventSource {
  return new EventSource(serviceLogsEventsUrl(serviceId, opts), { withCredentials: true })
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

export async function getServiceResourceUsageOverview(
  window: ServiceResourceUsageWindow = '1h',
): Promise<ServiceResourceOverviewResponse> {
  const query = new URLSearchParams({ window })
  const resp = await apiFetch(`/api/services/resource-usage/overview?${query.toString()}`)
  return (await resp.json()) as ServiceResourceOverviewResponse
}

export async function getHomepageNav(): Promise<HomepageNavResponse> {
  const resp = await apiFetch('/api/homepage/nav')
  return (await resp.json()) as HomepageNavResponse
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

export type UpdateServiceTargetInput = {
  serviceId: string
  targetTag: string
  targetDigest: string
  pullTags: string[]
}

export type TriggerUpdateInput =
  | (TriggerUpdateCommonInput & {
      scope: 'service'
      serviceId: string
      stackId?: string
      targetTag: string
      targetDigest: string
      pullTags: string[]
      targets?: never
    })
  | (TriggerUpdateCommonInput & {
      scope: 'stack'
      stackId: string
      serviceId?: never
      targetTag?: never
      targetDigest?: never
      pullTags?: never
      targets: UpdateServiceTargetInput[]
    })
  | (TriggerUpdateCommonInput & {
      scope: 'all'
      stackId?: never
      serviceId?: never
      targetTag?: never
      targetDigest?: never
      pullTags?: never
      targets: UpdateServiceTargetInput[]
    })

export async function triggerUpdate(input: TriggerUpdateInput) {
  const resp = await apiFetch('/api/updates', {
    method: 'POST',
    body: JSON.stringify({ ...input, reason: 'ui' }),
  })
  return (await resp.json()) as { jobId: string }
}

export type ServiceRollbackTargetResponse = {
  available: boolean
  currentDigest: string
  currentDisplayTag?: string | null
  targetDigest?: string | null
  targetDisplayTag?: string | null
  sourceUpdateJobId?: string | null
  sourceFinishedAt?: string | null
  unavailableReason?: string | null
  activeJobId?: string | null
  activeJobStatus?: string | null
}

export async function getServiceRollbackTarget(serviceId: string): Promise<ServiceRollbackTargetResponse> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/rollback-target`)
  return (await resp.json()) as ServiceRollbackTargetResponse
}

export async function triggerServiceRollback(serviceId: string): Promise<{ jobId: string }> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/rollback`, {
    method: 'POST',
  })
  return (await resp.json()) as { jobId: string }
}

export type ServiceLifecycleAction = 'start' | 'stop' | 'restart'
export type ServiceLifecycleStatusResponse = {
  state: ServiceLifecycleState
  activeJob?: {
    id: string
    type: string
    status: string
    action?: ServiceLifecycleAction | null
  } | null
  unavailableReason?: string | null
}

export async function getServiceLifecycleStatus(serviceId: string): Promise<ServiceLifecycleStatusResponse> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/lifecycle-status`)
  return (await resp.json()) as ServiceLifecycleStatusResponse
}

export async function triggerServiceLifecycle(
  serviceId: string,
  action: ServiceLifecycleAction,
): Promise<{ jobId: string }> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/lifecycle`, {
    method: 'POST',
    body: JSON.stringify({ action }),
  })
  return (await resp.json()) as { jobId: string }
}

export async function getStackLifecycleStatus(stackId: string): Promise<ServiceLifecycleStatusResponse> {
  const resp = await apiFetch(`/api/stacks/${encodeURIComponent(stackId)}/lifecycle-status`)
  return (await resp.json()) as ServiceLifecycleStatusResponse
}

export async function triggerStackLifecycle(
  stackId: string,
  action: ServiceLifecycleAction,
): Promise<{ jobId: string }> {
  const resp = await apiFetch(`/api/stacks/${encodeURIComponent(stackId)}/lifecycle`, {
    method: 'POST',
    body: JSON.stringify({ action }),
  })
  return (await resp.json()) as { jobId: string }
}

export async function listJobsPage(input: ListJobsInput = {}): Promise<ListJobsResponse> {
  const params = new URLSearchParams()
  if (input.cursor) params.set('cursor', input.cursor)
  if (input.limit != null) params.set('limit', String(input.limit))
  const type = Array.isArray(input.type) ? input.type.join(',') : input.type
  if (type) params.set('type', type)
  if (input.status) params.set('status', input.status)
  if (input.stackId) params.set('stackId', input.stackId)
  if (input.serviceId) params.set('serviceId', input.serviceId)
  const suffix = params.size > 0 ? `?${params.toString()}` : ''
  const resp = await apiFetch(`/api/jobs${suffix}`)
  return (await resp.json()) as ListJobsResponse
}

export async function listJobs(input: ListJobsInput = {}): Promise<JobListItem[]> {
  const jobs: JobListItem[] = []
  let cursor = input.cursor ?? null

  // Keep legacy callers bounded at the former API ceiling while specialized
  // surfaces use listJobsPage for explicit cursor navigation.
  while (jobs.length < 2000) {
    const page = await listJobsPage({ ...input, cursor, limit: Math.min(input.limit ?? 200, 200) })
    jobs.push(...page.jobs)
    if (!page.nextCursor) break
    cursor = page.nextCursor
  }

  return jobs.slice(0, 2000)
}

export async function listCompactJobsPage(input: ListJobsInput = {}): Promise<ListCompactJobsResponse> {
  const params = new URLSearchParams()
  params.set('view', 'compact')
  if (input.cursor) params.set('cursor', input.cursor)
  if (input.limit != null) params.set('limit', String(input.limit))
  const type = Array.isArray(input.type) ? input.type.join(',') : input.type
  if (type) params.set('type', type)
  if (input.status) params.set('status', input.status)
  if (input.stackId) params.set('stackId', input.stackId)
  if (input.serviceId) params.set('serviceId', input.serviceId)
  const resp = await apiFetch(`/api/jobs?${params.toString()}`)
  return (await resp.json()) as ListCompactJobsResponse
}

// Hot surfaces intentionally request one explicit page. Detail pages keep listJobs for
// backwards-compatible access to raw summaries.
export async function listCompactJobs(input: ListJobsInput = {}): Promise<CompactJobListItem[]> {
  const page = await listCompactJobsPage({ ...input, limit: Math.min(input.limit ?? 200, 200) })
  return page.jobs
}

export async function getJob(jobId: string): Promise<JobDetail> {
  const resp = await apiFetch(`/api/jobs/${encodeURIComponent(jobId)}`)
  const data = await resp.json()
  return data.job as JobDetail
}

export async function stopJob(jobId: string): Promise<{ jobId: string; state: 'requested' }> {
  const resp = await apiFetch(`/api/jobs/${encodeURIComponent(jobId)}/stop`, { method: 'POST' })
  return (await resp.json()) as { jobId: string; state: 'requested' }
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
  const data = (await resp.json()) as SettingsResponse & {
    backup: SettingsResponse['backup'] & { storage?: SettingsResponse['backup']['storage'] }
  }
  return {
    ...data,
    backup: {
      ...data.backup,
      storage: data.backup.storage ?? {
        mode: 'legacy',
        logicalPath: data.backup.baseDir,
        resolvedLocation: data.backup.baseDir,
        writable: true,
        diagnostic: '旧版 API 未提供部署存储解析状态',
      },
    },
    releaseNotes: data.releaseNotes ?? {
      provider: 'gitHub',
      octoRill: {
        enabled: false,
        apiBaseUrl: null,
        apiKeyMasked: null,
        defaultView: 'smart',
      },
    },
    auth: {
      ...data.auth,
      currentGroups: Array.isArray(data.auth?.currentGroups)
        ? data.auth.currentGroups.filter((value): value is string => typeof value === 'string')
        : [],
    },
  }
}

export async function putSettings(input: PutSettingsInput, legacyBackupBaseDir?: string) {
  const request = (body: unknown) => apiFetch('/api/settings', {
    method: 'PUT',
    body: JSON.stringify(body),
  })
  try {
    const resp = await request(input)
    return (await resp.json()) as { ok: boolean }
  } catch (error) {
    const missingLegacyBaseDir = error instanceof ApiError
      && error.status === 400
      && /base.?dir|missing field/i.test(`${error.message} ${error.bodyText ?? ''}`)
    if (!missingLegacyBaseDir || !legacyBackupBaseDir) throw error
    const resp = await request({
      ...input,
      backup: { ...input.backup, baseDir: legacyBackupBaseDir },
    })
    return (await resp.json()) as { ok: boolean }
  }
}

export async function getDeployCheckReport(): Promise<DeployCheckReportEnvelope> {
  const resp = await apiFetch('/api/deploy-check/report')
  return (await resp.json()) as DeployCheckReportEnvelope
}

export async function refreshDeployCheckReport(): Promise<DeployCheckReportEnvelope> {
  const resp = await apiFetch('/api/deploy-check/report/refresh', {
    method: 'POST',
    body: '{}',
  })
  return (await resp.json()) as DeployCheckReportEnvelope
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
  webhookState?: 'all' | 'ok' | 'missing' | 'error' | 'conflict' | 'queued' | 'running' | 'unknown' | string | null
}): Promise<ListGitHubPackagesReposResponse> {
  const sp = new URLSearchParams()
  sp.set('page', String(input.page))
  sp.set('perPage', String(input.perPage))
  if (input.q) sp.set('q', input.q)
  if (input.selectedFilter && input.selectedFilter !== 'all') sp.set('selectedFilter', input.selectedFilter)
  if (input.webhookState && input.webhookState !== 'all') sp.set('webhookState', input.webhookState)
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

export async function getServiceBackupTargets(serviceId: string): Promise<ServiceBackupTargetsResponse> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/backup-targets`)
  return (await resp.json()) as ServiceBackupTargetsResponse
}

export async function putServiceBackupTargets(
  serviceId: string,
  input: PutServiceBackupTargetsRequest,
): Promise<PutServiceBackupTargetsResponse> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/backup-targets`, {
    method: 'PUT',
    body: JSON.stringify(input),
  })
  return (await resp.json()) as PutServiceBackupTargetsResponse
}

export async function getServiceBackupRecords(serviceId: string): Promise<ServiceBackupRecordsResponse> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/backup-records`)
  const data = (await resp.json()) as ServiceBackupRecordsResponse
  return {
    ...data,
    records: Array.isArray(data.records)
      ? data.records.map((record) => ({
          ...record,
          assets: Array.isArray(record.assets) ? record.assets : [],
        }))
      : [],
  }
}

export async function inferServiceRepoLink(serviceId: string): Promise<ServiceRepoLinkInferenceResponse> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/repo-link/infer`, {
    method: 'POST',
  })
  return (await resp.json()) as ServiceRepoLinkInferenceResponse
}

export async function listServiceTagSuggestions(serviceId: string): Promise<ServiceTagSuggestionsResponse> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/tag-suggestions`)
  return (await resp.json()) as ServiceTagSuggestionsResponse
}

export async function putServiceComposeTag(
  serviceId: string,
  tag: string,
): Promise<PutServiceComposeTagResponse> {
  const resp = await apiFetch(`/api/services/${encodeURIComponent(serviceId)}/compose-tag`, {
    method: 'PUT',
    body: JSON.stringify({ tag }),
  })
  return (await resp.json()) as PutServiceComposeTagResponse
}
