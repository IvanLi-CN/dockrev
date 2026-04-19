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
  repoUrl?: string | null
}

export type ServiceHomepage = {
  group?: string | null
  name?: string | null
  icon?: string | null
  href?: string | null
  description?: string | null
}

export type ServiceUpdateGuard = {
  blocked: true
  code: 'traefik_online_service_requires_manual_zero_downtime' | string
  reason: string
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
  homepage?: ServiceHomepage | null
  updateGuard?: ServiceUpdateGuard | null
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
  newVersionDiscoveryCount?: number | null
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

export type ServiceRepoLinkInferenceResponse = {
  repoUrl: string | null
  strategy: 'oci_source' | 'ghcr_exact' | 'none'
  reason?: string | null
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
  digest: string
  reason: string
}

export type NewVersionDiscoveryTimelineItemKind =
  | 'currentCandidate'
  | 'historicalCandidate'
  | 'currentRunning'

export type NewVersionDiscoveryTimelineItem = {
  kind: NewVersionDiscoveryTimelineItemKind
  version: string
  occurredAt?: string | null
}

export type NewVersionDiscoveryTimelineResponse = {
  items: NewVersionDiscoveryTimelineItem[]
}

export type GitHubReleaseAuthMode = 'pat' | 'anonymous'

export type ServiceGitHubReleasesStatus =
  | 'ready'
  | 'unsupportedRepo'
  | 'permissionDenied'
  | 'rateLimited'
  | 'upstreamError'

export type ServiceGitHubRepoRef = {
  fullName: string
  htmlUrl: string
}

export type ServiceGitHubReleaseItem = {
  id: number
  tagName: string
  name?: string | null
  body?: string | null
  htmlUrl: string
  draft: boolean
  prerelease: boolean
  publishedAt?: string | null
  createdAt?: string | null
}

export type ServiceGitHubReleasesResponse = {
  status: ServiceGitHubReleasesStatus
  authMode: GitHubReleaseAuthMode
  repo?: ServiceGitHubRepoRef | null
  page: number
  perPage: number
  hasMore: boolean
  items: ServiceGitHubReleaseItem[]
  message?: string | null
}

export type ServiceGitHubReleaseLocateStatus =
  | 'found'
  | 'outsideWindow'
  | 'notFound'
  | 'unsupportedRepo'
  | 'permissionDenied'
  | 'rateLimited'
  | 'upstreamError'

export type ServiceGitHubReleaseLocateResponse = {
  status: ServiceGitHubReleaseLocateStatus
  authMode: GitHubReleaseAuthMode
  repo?: ServiceGitHubRepoRef | null
  version: string
  searchedCount: number
  matchedTag?: string | null
  page?: number | null
  indexWithinPage?: number | null
  absoluteIndex?: number | null
  message?: string | null
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
    groupHeaderName: string
    allowAnonymousInDev: boolean
    authorizationMode: string
    allowedUserMasked?: string | null
    allowedGroupMasked?: string | null
    currentUser?: string | null
    currentGroups: string[]
    matchedBy?: string | null
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

export type CleanupPreset = 'conservative' | 'balanced' | 'project_deep_clean' | 'aggressive'

export type CleanupScope = 'all' | 'stack' | 'service'

export type CleanupResourceKind = 'image' | 'container' | 'network' | 'volume' | 'builder_cache'

export type CleanupScanReason = 'page' | 'confirm'

export type CleanupApplyReason = 'ui'

export type CleanupScanRequest = {
  reason: CleanupScanReason
  preset: CleanupPreset
  scope: CleanupScope
  stackId?: string
  serviceId?: string
}

export type CleanupApplyRequest = {
  reason: CleanupApplyReason
  preset: CleanupPreset
  scope: CleanupScope
  stackId?: string
  serviceId?: string
  confirmationFingerprint: string
}

export type CleanupApplyResponse = {
  jobId: string
}

export type CleanupResourceItem = {
  resourceId: string
  kind: CleanupResourceKind
  label: string
  reason: string
  minPreset: CleanupPreset
  estimatedReclaimableBytes?: number | null
  estimateUnknown?: boolean
}

export type CleanupServiceGroup = {
  serviceId: string
  serviceName: string
  estimatedReclaimableBytes: number
  hasUnknownSize?: boolean
  resources: CleanupResourceItem[]
}

export type CleanupStackGroup = {
  stackId: string
  stackName: string
  estimatedReclaimableBytes: number
  hasUnknownSize?: boolean
  stackOrphans: CleanupResourceItem[]
  services: CleanupServiceGroup[]
}

export type CleanupUnownedGroup = {
  title: string
  estimatedReclaimableBytes: number
  hasUnknownSize?: boolean
  resources: CleanupResourceItem[]
}

export type CleanupScanResponse = {
  reason: CleanupScanReason
  preset: CleanupPreset
  scope: CleanupScope
  scannedAt: string
  estimatedReclaimableBytes: number
  hasUnknownSize?: boolean
  stackGroups: CleanupStackGroup[]
  unownedGroup?: CleanupUnownedGroup | null
  confirmationFingerprint?: string | null
}

export type CleanupFingerprintMismatchError = {
  latest: CleanupScanResponse
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
  events?: {
    update: boolean
    newVersion: boolean
    ghcrWebhookAnomaly: boolean
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
    ghcrLinked?: boolean | null
    deployed: boolean
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
  jobIds?: string[]
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

export type GitHubPackagesWebhookDeliveryEventPayload = {
  type: 'github_packages_delivery_event' | string
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
  jobIds?: string[]
  attemptCount: number
}

export type GitHubPackagesWebhookDeliveryEventsErrorPayload = {
  type: 'github_packages_delivery_events_error' | string
  error: string
  afterId: number
  oldestId?: number | null
  latestId?: number | null
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
