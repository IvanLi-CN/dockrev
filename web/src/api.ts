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

export type Service = {
  id: string
  name: string
  image: {
    ref: string
    tag: string
    digest?: string | null
  }
  candidate?: {
    tag: string
    digest: string
    archMatch: ArchMatch
    arch: string[]
  } | null
  ignore?: {
    matched: boolean
    ruleId: string
    reason: string
  } | null
  settings: ServiceSettings
  archived?: boolean
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
}

export type JobLogLine = {
  ts: string
  level: string
  msg: string
}

export type JobDetail = JobListItem & { logs: JobLogLine[] }

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
  auth: {
    forwardHeaderName: string
    allowAnonymousInDev: boolean
  }
}

export type NotificationConfig = {
  email: { enabled: boolean; smtpUrl?: string | null }
  webhook: { enabled: boolean; url?: string | null }
  telegram: { enabled: boolean; botToken?: string | null; chatId?: string | null }
  webPush: {
    enabled: boolean
    vapidPublicKey?: string | null
    vapidPrivateKey?: string | null
    vapidSubject?: string | null
  }
}

const API_BASE = import.meta.env.VITE_API_BASE_URL ?? ''

async function apiFetch(path: string, init?: RequestInit) {
  const resp = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers || {}),
    },
  })

  if (!resp.ok) {
    const text = await resp.text().catch(() => '')
    throw new Error(`HTTP ${resp.status}: ${text || resp.statusText}`)
  }
  return resp
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

export async function triggerDiscoveryScan(): Promise<DiscoveryScanResponse> {
  const resp = await apiFetch('/api/discovery/scan', { method: 'POST', body: '{}' })
  return (await resp.json()) as DiscoveryScanResponse
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

export async function triggerCheck(scope: string, stackId?: string, serviceId?: string) {
  const resp = await apiFetch('/api/checks', {
    method: 'POST',
    body: JSON.stringify({ scope, stackId, serviceId, reason: 'ui' }),
  })
  return (await resp.json()) as { checkId: string }
}

export async function triggerUpdate(input: {
  scope: string
  stackId?: string
  serviceId?: string
  mode: 'apply' | 'dry-run'
  allowArchMismatch: boolean
  backupMode: 'inherit' | 'skip' | 'force'
}) {
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

export async function putSettings(input: SettingsResponse['backup']) {
  const resp = await apiFetch('/api/settings', {
    method: 'PUT',
    body: JSON.stringify({ backup: input }),
  })
  return (await resp.json()) as { ok: boolean }
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

export async function testNotifications(message?: string) {
  const resp = await apiFetch('/api/notifications/test', {
    method: 'POST',
    body: JSON.stringify({ message: message || null }),
  })
  return (await resp.json()) as { ok: boolean; results: unknown }
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
