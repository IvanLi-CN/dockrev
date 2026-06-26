import type {
  CleanupScanResponse,
  DeployCheckReportEnvelope,
  DeployCheckReportResponse,
  DeployWelcomeResponse,
  DiscoveredProject,
  GitHubPackagesSettingsResponse,
  GitHubPackagesWebhookOverviewResponse,
  IgnoreRule,
  JobDetail,
  JobListItem,
  NewVersionDiscoveryTimelineResponse,
  NotificationConfig,
  Service,
  ServiceDigestTagsSnapshotResult,
  ServiceGitHubReleasesResponse,
  ServiceResourceHistoryResponse,
  ServiceResourceOverviewResponse,
  SettingsResponse,
  StackDetail,
  StackListItem,
  StackSettings,
  VersionInferenceOverviewResponse,
} from '../api'

type DemoInstallResult = {
  enabled: boolean
  mode: 'app'
}

const originalFetch = globalThis.fetch.bind(globalThis)
let installed = false
let checkSeq = 0
const jobsById: Record<string, JobDetail> = {}

function nowIso(offsetMs = 0): string {
  return new Date(Date.now() + offsetMs).toISOString()
}

function digest(fill: string, suffix: string): string {
  return `sha256:${fill.repeat(62)}${suffix}`
}

const defaultSettings = {
  autoRollback: true,
  backupTargets: { bindPaths: {}, volumeNames: {} },
  repoUrl: null,
}

const services = {
  api: {
    id: 'svc-prod-api',
    name: 'api',
    image: { ref: 'ghcr.io/acme/api:5.2.1', tag: '5.2.1', digest: digest('a', 'b1') },
    candidate: { tag: '5.2.3', digest: digest('b', '9f'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    homepage: {
      group: 'Brain',
      name: 'Acme API',
      icon: 'si-github',
      href: 'https://api.example.com',
      description: 'API gateway & auth',
    },
    settings: { ...defaultSettings, repoUrl: 'https://github.com/acme/api' },
  },
  web: {
    id: 'svc-prod-web',
    name: 'web',
    image: { ref: 'harbor.local/ops/web:5.2', tag: '5.2', digest: digest('c', 'c2') },
    candidate: { tag: '5.2.7', digest: digest('d', '7a'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    homepage: {
      group: 'Brain',
      name: 'Web Console',
      icon: 'mdi-monitor-dashboard',
      href: 'https://web.example.com',
      description: 'Primary admin console',
    },
    settings: { ...defaultSettings, repoUrl: 'https://github.com/acme/web' },
  },
  worker: {
    id: 'svc-prod-worker',
    name: 'worker',
    image: { ref: 'ghcr.io/acme/worker:5.2.0', tag: '5.2.0', digest: digest('e', 'aa') },
    candidate: null,
    ignore: null,
    homepage: {
      group: 'Tools',
      name: 'Background Jobs',
      icon: 'mdi-cog-refresh-outline',
      href: null,
      description: 'Queue workers & cron',
    },
    settings: { ...defaultSettings, repoUrl: 'https://github.com/acme/worker' },
  },
  loki: {
    id: 'svc-infra-loki',
    name: 'loki',
    image: {
      ref: 'ghcr.io/grafana/loki:2.9.0',
      tag: '2.9.0',
      digest: 'sha256:1111111111111111111111111111111111111111111111111111111111111111',
    },
    candidate: null,
    ignore: null,
    homepage: {
      group: 'Media',
      name: 'Loki',
      icon: 'mdi-file-document-multiple-outline',
      href: 'https://logs.example.com',
      description: 'Log aggregation',
    },
    settings: { ...defaultSettings, repoUrl: 'https://github.com/grafana/loki' },
  },
  prometheus: {
    id: 'svc-infra-prom',
    name: 'prometheus',
    image: {
      ref: 'quay.io/prometheus/prometheus:2.49.0',
      tag: '2.49.0',
      digest: 'sha256:3333333333333333333333333333333333333333333333333333333333333333',
    },
    candidate: null,
    ignore: null,
    homepage: {
      group: 'Tools',
      name: 'Prometheus',
      icon: 'prometheus.svg',
      href: 'https://metrics.example.com',
      description: 'Metrics & alerts',
    },
    settings: defaultSettings,
  },
  postgres: {
    id: 'svc-infra-postgres',
    name: 'postgres',
    image: { ref: 'docker.io/library/postgres:16', tag: '16', digest: digest('p', '16') },
    candidate: { tag: '18.1', digest: digest('p', '18'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    homepage: {
      group: 'Infra',
      name: 'Postgres',
      icon: 'https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/postgres.svg',
      href: 'https://db.example.com',
      description: 'Transactional database',
    },
    settings: defaultSettings,
  },
} satisfies Record<string, Service>

const stackDetails: Record<string, StackDetail> = {
  'stack-prod': {
    id: 'stack-prod',
    name: 'prod',
    compose: { type: 'path', composeFiles: ['/srv/app/compose.yml'], envFile: '/srv/app/.env' },
    services: [services.api, services.web, services.worker],
  },
  'stack-infra': {
    id: 'stack-infra',
    name: 'infra',
    compose: { type: 'path', composeFiles: ['/srv/infra/compose.yml'], envFile: '/srv/infra/.env' },
    services: [services.loki, services.prometheus, services.postgres],
  },
}

const stackSettingsById: Record<string, StackSettings> = {
  'stack-prod': {
    autoUpdatePolicy: {
      mode: 'override',
      enabled: true,
      rules: [
        {
          id: 'demo-prod-stable',
          name: 'Stable releases',
          enabled: true,
          matcher: { type: 'semver', pattern: '>=5.0.0, <6.0.0' },
          action: 'delayed',
          delay: { minAgeSeconds: 86400, minVersionLag: 2 },
        },
      ],
    },
  },
  'stack-infra': {
    autoUpdatePolicy: {
      mode: 'override',
      enabled: false,
      rules: [],
    },
  },
}

const stackList = [
  {
    id: 'stack-prod',
    name: 'prod',
    status: 'healthy',
    services: 3,
    updates: 2,
    lastCheckAt: '2026-01-18T06:10:00.000Z',
  },
  {
    id: 'stack-infra',
    name: 'infra',
    status: 'healthy',
    services: 3,
    updates: 1,
    lastCheckAt: '2026-01-18T06:10:00.000Z',
  },
] satisfies StackListItem[]

const demoSettings = {
  backup: {
    enabled: true,
    requireSuccess: true,
    baseDir: '/srv/dockrev/backups',
    skipTargetsOverBytes: 0,
  },
  resourceMonitor: {
    enabled: true,
    sampleIntervalSeconds: 10,
    retentionDays: 7,
  },
  schedules: {
    updateCheck: { enabled: true, cron: '17 */6 * * *' },
    ghcrWebhookAudit: { enabled: true, cron: '42 */12 * * *' },
  },
  auth: {
    forwardHeaderName: 'X-Forwarded-User',
    groupHeaderName: 'X-Forwarded-Groups',
    allowAnonymousInDev: true,
    authorizationMode: 'disabled',
    allowedUserMasked: null,
    allowedGroupMasked: null,
    currentUser: 'Forward Auth',
    currentGroups: ['ops', 'demo'],
    avatarUrl: null,
    matchedBy: null,
  },
  instance: {
    publicBaseUrl: 'http://127.0.0.1:50884',
  },
} satisfies SettingsResponse

const metrics = {
  'svc-prod-api': { cpuPercent: 13, memUsedBytes: 270 * 1024 * 1024, netRxRateBps: 7.58 * 1024, netTxRateBps: 20.9 * 1024 },
  'svc-prod-web': { cpuPercent: 19, memUsedBytes: 554 * 1024 * 1024, netRxRateBps: 7.58 * 1024, netTxRateBps: 4.85 * 1024 },
  'svc-prod-worker': { cpuPercent: 8, memUsedBytes: 186 * 1024 * 1024, netRxRateBps: 12.6 * 1024, netTxRateBps: 14.1 * 1024 },
  'svc-infra-loki': { cpuPercent: 21, memUsedBytes: 840 * 1024 * 1024, netRxRateBps: 9.1 * 1024, netTxRateBps: 18.2 * 1024 },
  'svc-infra-prom': { cpuPercent: 26, memUsedBytes: 440 * 1024 * 1024, netRxRateBps: 14.5 * 1024, netTxRateBps: 11.7 * 1024 },
  'svc-infra-postgres': { cpuPercent: 19, memUsedBytes: 310 * 1024 * 1024, netRxRateBps: 0, netTxRateBps: 7.55 * 1024 },
}

function json(data: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(data), {
    status: init?.status ?? 200,
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers ?? {}),
    },
  })
}

function readUrl(input: RequestInfo | URL): URL | null {
  const urlString = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url
  try {
    return new URL(urlString, typeof window !== 'undefined' ? window.location.href : 'http://localhost')
  } catch {
    return null
  }
}

function buildResourceOverview(windowValue: string): ServiceResourceOverviewResponse {
  const sampledAt = nowIso(-12_000)
  return {
    enabled: true,
    window: windowValue || '1h',
    generatedAt: nowIso(),
    staleAfterSeconds: 60,
    services: Object.entries(metrics).map(([serviceId, metric]) => ({
      serviceId,
      sampledAt,
      cpuPercent: metric.cpuPercent,
      memUsedBytes: metric.memUsedBytes,
      memLimitBytes: 2 * 1024 * 1024 * 1024,
      netRxRateBps: metric.netRxRateBps,
      netTxRateBps: metric.netTxRateBps,
      stale: false,
      sampleCount: 120,
    })),
  }
}

function buildResourceHistory(serviceId: string, windowValue: string): ServiceResourceHistoryResponse {
  const metric = metrics[serviceId as keyof typeof metrics]
  return {
    serviceId,
    window: windowValue,
    samples: Array.from({ length: 18 }, (_, idx) => ({
      sampledAt: nowIso(-((17 - idx) * 30_000)),
      cpuPercent: metric ? Math.max(0, metric.cpuPercent + Math.sin(idx / 2) * 3) : 0,
      memUsedBytes: metric?.memUsedBytes ?? 0,
      memLimitBytes: 2 * 1024 * 1024 * 1024,
      netRxBytes: Math.round((metric?.netRxRateBps ?? 0) * idx * 30),
      netTxBytes: Math.round((metric?.netTxRateBps ?? 0) * idx * 30),
      containerCount: 1,
    })),
  }
}

function listJobItems(): JobListItem[] {
  return Object.values(jobsById).map((job) => ({
    id: job.id,
    type: job.type,
    scope: job.scope,
    stackId: job.stackId,
    serviceId: job.serviceId,
    status: job.status,
    createdBy: job.createdBy,
    reason: job.reason,
    createdAt: job.createdAt,
    startedAt: job.startedAt,
    finishedAt: job.finishedAt,
    allowArchMismatch: job.allowArchMismatch,
    backupMode: job.backupMode,
    summary: job.summary,
    progress: job.progress,
  }))
}

function createJob(type: string, summary: unknown = {}): JobDetail {
  checkSeq += 1
  const id = `demo-${type}-${checkSeq}`
  const createdAt = nowIso(-500)
  const job: JobDetail = {
    id,
    type,
    scope: 'all',
    stackId: null,
    serviceId: null,
    status: 'success',
    createdBy: 'demo',
    reason: 'ui',
    createdAt,
    startedAt: createdAt,
    finishedAt: nowIso(-100),
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary,
    logs: [{ ts: createdAt, level: 'info', msg: `demo ${type} completed` }],
    logsLastId: 1,
    progress: null,
  }
  jobsById[id] = job
  return job
}

function createCheckJob(): { checkId: string } {
  return { checkId: createJob('check', { checkedServices: Object.keys(metrics).length }).id }
}

function createRuntimeScanJob(): { jobId: string } {
  return { jobId: createJob('runtime_scan', { scannedServices: Object.keys(metrics).length }).id }
}

function createUpdateJob(): { jobId: string } {
  return { jobId: createJob('update', { updatedServices: 2 }).id }
}

function createDiscoveryJob(): { jobId: string } {
  return { jobId: createJob('discovery_scan', { projectsSeen: stackList.length }).id }
}

const discoveredProjects = [
  {
    project: 'prod',
    status: 'active',
    stackId: 'stack-prod',
    configFiles: ['/srv/app/compose.yml'],
    lastSeenAt: nowIso(-180_000),
    lastScanAt: nowIso(-180_000),
    lastError: null,
    archived: false,
  },
  {
    project: 'infra',
    status: 'active',
    stackId: 'stack-infra',
    configFiles: ['/srv/infra/compose.yml'],
    lastSeenAt: nowIso(-180_000),
    lastScanAt: nowIso(-180_000),
    lastError: null,
    archived: false,
  },
] satisfies DiscoveredProject[]

const ignores: IgnoreRule[] = []

const notifications = {
  email: { enabled: false, smtpUrl: null },
  webhook: { enabled: false, url: null },
  telegram: { enabled: false, botTokenConfigured: false, botToken: null, chatId: null },
  webPush: { enabled: false, vapidPublicKey: null, vapidPrivateKey: null, vapidSubject: null },
  events: { update: true, newVersion: true, ghcrWebhookAnomaly: true },
} satisfies NotificationConfig

const githubPackagesSettings = {
  enabled: true,
  callbackUrl: 'http://127.0.0.1:50884/api/webhooks/github-packages',
  targets: [{ input: 'acme', kind: 'owner', owner: 'acme', warnings: [] }],
  reposTotal: 3,
  reposSelectedTotal: 2,
  patMasked: 'ghp_********',
  secretMasked: '********',
} satisfies GitHubPackagesSettingsResponse

const githubWebhookOverview = {
  summary: { tracked: 2, ok: 2, missing: 0, error: 0, conflict: 0, queued: 0, running: 0, unknown: 0 },
  jobsQueued: 0,
  jobsRunning: 0,
  runningJobId: null,
  lastAuditAt: nowIso(-900_000),
} satisfies GitHubPackagesWebhookOverviewResponse

const versionInferenceOverview = {
  worker: { maxConcurrency: 2, queued: 0, running: 0, inFlight: 0 },
  gc: { retentionDays: 7, intervalSeconds: 3600, lastRunAt: nowIso(-3_600_000), lastDeleted: 0, lastDurationMs: 18, lastError: null },
  summary: { snapshotsTotal: 6, queued: 0, running: 0, ready: 6, stale: 0, allFailed: 0 },
  tasks: [],
  rows: Object.values(services).map((svc) => ({
    key: `${svc.image.ref}|linux/amd64`,
    imageRepo: svc.image.ref.split(':')[0] ?? svc.image.ref,
    hostPlatform: 'linux/amd64',
    status: 'ready',
    serviceCount: 1,
    reason: null,
    checkedAt: nowIso(-600_000),
    updatedAt: nowIso(-600_000),
    progress: null,
  })),
  page: 1,
  perPage: 25,
  total: Object.keys(services).length,
} satisfies VersionInferenceOverviewResponse

const deployCheckReport = {
  overall: { result: 'pass', blockingCheckIds: [], summary: 'Demo deployment checks are configured.' },
  generatedAt: nowIso(),
  checks: [
    {
      id: 'public-base-url',
      title: 'Public base URL',
      group: 'core',
      required: true,
      status: 'pass',
      summary: 'Configured for demo.',
      impact: 'Links resolve inside the demo preview.',
      evidence: demoSettings.instance.publicBaseUrl,
      recommendation: 'Keep this aligned with the deployed origin.',
    },
  ],
} satisfies DeployCheckReportResponse

const deployCheckEnvelope = {
  status: 'ready',
  refreshing: false,
  retryAfterMs: null,
  report: deployCheckReport,
} satisfies DeployCheckReportEnvelope

function buildCleanupScan(): CleanupScanResponse {
  return {
    status: 'ready',
    reason: 'page',
    preset: 'balanced',
    scope: 'all',
    scannedAt: nowIso(),
    refreshing: false,
    retryAfterMs: null,
    estimatedReclaimableBytes: 734 * 1024 * 1024,
    stackGroups: [
      {
        stackId: 'stack-prod',
        stackName: 'prod',
        estimatedReclaimableBytes: 420 * 1024 * 1024,
        stackOrphans: [],
        services: [
          {
            serviceId: services.web.id,
            serviceName: services.web.name,
            estimatedReclaimableBytes: 420 * 1024 * 1024,
            resources: [
              {
                resourceId: 'image:harbor.local/ops/web:5.1',
                kind: 'image',
                label: 'web:5.1',
                reason: 'older image kept after update',
                minPreset: 'balanced',
                estimatedReclaimableBytes: 420 * 1024 * 1024,
              },
            ],
          },
        ],
      },
    ],
    unownedGroup: null,
    confirmationFingerprint: 'demo-cleanup-fingerprint',
  }
}

function digestSnapshot(digestValue: string): ServiceDigestTagsSnapshotResult {
  return {
    digest: digestValue,
    tags: ['5.2.1', 'stable'],
    checkedAt: nowIso(-300_000),
    scan: { repoTagsTotal: 3, repoTagsConsidered: 3, manifestsOk: 3, manifestsTimeout: 0, manifestsError: 0 },
  }
}

function githubReleases(): ServiceGitHubReleasesResponse {
  return {
    status: 'ready',
    authMode: 'anonymous',
    repo: { fullName: 'acme/api', htmlUrl: 'https://github.com/acme/api' },
    page: 1,
    perPage: 20,
    hasMore: false,
    items: [
      {
        id: 1001,
        tagName: 'v5.2.3',
        name: 'v5.2.3',
        body: 'Demo release notes.',
        htmlUrl: 'https://github.com/acme/api/releases/tag/v5.2.3',
        draft: false,
        prerelease: false,
        publishedAt: nowIso(-86_400_000),
        createdAt: nowIso(-86_400_000),
      },
    ],
  }
}

function isDemoRequested(): boolean {
  const flag = import.meta.env.VITE_DOCKREV_DEMO
  const normalizedFlag = (flag ?? '').trim().toLowerCase()
  return normalizedFlag === 'app' || normalizedFlag === 'true' || normalizedFlag === '1'
}

function installDemoEventSource() {
  if (typeof EventSource === 'undefined') return
  class DemoEventSource extends EventTarget {
    static readonly CONNECTING = 0
    static readonly OPEN = 1
    static readonly CLOSED = 2

    readonly url: string
    readonly withCredentials: boolean
    readyState = DemoEventSource.OPEN
    onopen: ((event: Event) => void) | null = null
    onmessage: ((event: MessageEvent) => void) | null = null
    onerror: ((event: Event) => void) | null = null

    constructor(url: string | URL, init?: EventSourceInit) {
      super()
      this.url = String(url)
      this.withCredentials = Boolean(init?.withCredentials)
      window.setTimeout(() => {
        const event = new Event('open')
        this.dispatchEvent(event)
        this.onopen?.(event)
      }, 0)
    }

    close() {
      this.readyState = DemoEventSource.CLOSED
    }
  }
  globalThis.EventSource = DemoEventSource as unknown as typeof EventSource
}

export function installAppDemoApi(): DemoInstallResult | null {
  if (!isDemoRequested()) return null
  if (installed) return { enabled: true, mode: 'app' }
  installed = true
  installDemoEventSource()

  globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
    const method = (init?.method ?? (input instanceof Request ? input.method : 'GET')).toUpperCase()
    const url = readUrl(input)
    if (!url) return originalFetch(input, init)

    const path = url.pathname
    if (path === '/api/version' && method === 'GET') return json({ version: '0.6.0-demo' })
    if (path === '/api/settings' && method === 'GET') return json(demoSettings)
    if (path === '/api/settings' && method === 'PUT') return json({ ok: true })
    if (path === '/api/notifications' && method === 'GET') return json(notifications)
    if (path === '/api/notifications' && method === 'PUT') return json({ ok: true })
    if (path === '/api/notifications/test' && method === 'POST') {
      return json({ ok: true, results: { email: { ok: true }, webhook: { ok: true }, telegram: { ok: true }, webPush: { ok: true } } })
    }
    if (path === '/api/deploy-check/report' && method === 'GET') return json(deployCheckEnvelope)
    if (path === '/api/deploy-check/report/refresh' && method === 'POST') return json(deployCheckEnvelope, { status: 202 })
    if (path === '/api/deploy-welcome' && method === 'GET') {
      return json({ neverAutoOpen: true, updatedAt: nowIso(-86_400_000) } satisfies DeployWelcomeResponse)
    }
    if (path === '/api/deploy-welcome' && method === 'PUT') {
      return json({ ok: true, neverAutoOpen: true, updatedAt: nowIso() } satisfies DeployWelcomeResponse & { ok: boolean })
    }
    if (path === '/api/stacks' && method === 'GET') return json({ stacks: stackList })
    if (path.startsWith('/api/stacks/') && path.endsWith('/settings') && method === 'GET') {
      const id = decodeURIComponent(path.split('/').slice(3, -1).join('/'))
      return json(stackSettingsById[id] ?? { autoUpdatePolicy: { mode: 'override', enabled: false, rules: [] } })
    }
    if (path.startsWith('/api/stacks/') && path.endsWith('/settings') && method === 'PUT') {
      const id = decodeURIComponent(path.split('/').slice(3, -1).join('/'))
      const body = typeof init?.body === 'string' ? init.body : ''
      if (body) stackSettingsById[id] = JSON.parse(body) as StackSettings
      return json({ ok: true })
    }
    if (path.startsWith('/api/stacks/') && method === 'GET') {
      const id = decodeURIComponent(path.split('/').slice(3).join('/'))
      const stack = stackDetails[id]
      return stack ? json({ stack }) : json({ error: 'not found' }, { status: 404 })
    }
    if (path.startsWith('/api/stacks/') && method === 'POST') return json({ ok: true })
    if (path.startsWith('/api/services/') && (path.endsWith('/archive') || path.endsWith('/restore')) && method === 'POST') {
      return json({ ok: true })
    }
    if (path === '/api/discovery/projects' && method === 'GET') return json({ projects: discoveredProjects })
    if (path.startsWith('/api/discovery/projects/') && method === 'POST') return json({ ok: true })
    if (path === '/api/discovery/scan' && method === 'POST') return json(createDiscoveryJob())
    if (path === '/api/runtime-scans' && method === 'POST') return json(createRuntimeScanJob())
    if (path === '/api/services/resource-usage/overview' && method === 'GET') {
      return json(buildResourceOverview(url.searchParams.get('window') ?? '1h'))
    }
    if (path.startsWith('/api/services/') && path.endsWith('/resource-usage/history') && method === 'GET') {
      const serviceId = decodeURIComponent(path.split('/')[3] ?? '')
      return json(buildResourceHistory(serviceId, url.searchParams.get('window') ?? '1h'))
    }
    if (path.startsWith('/api/services/') && path.endsWith('/settings') && method === 'GET') {
      const serviceId = decodeURIComponent(path.split('/')[3] ?? '')
      const service = Object.values(services).find((item) => item.id === serviceId)
      return service ? json(service.settings) : json(defaultSettings)
    }
    if (path.startsWith('/api/services/') && path.endsWith('/settings') && method === 'PUT') return json({ ok: true })
    if (path.startsWith('/api/services/') && path.endsWith('/rollback-target') && method === 'GET') {
      return json({ available: false, currentDigest: '', unavailableReason: 'no_matching_update_history' })
    }
    if (path.startsWith('/api/services/') && path.endsWith('/rollback') && method === 'POST') return json(createUpdateJob())
    if (path.startsWith('/api/services/') && path.endsWith('/digest-tags') && method === 'GET') {
      return json(digestSnapshot(url.searchParams.get('digest') ?? 'sha256:demo'))
    }
    if (path.startsWith('/api/services/') && path.endsWith('/digest-tags-snapshot') && method === 'GET') {
      return json(digestSnapshot(url.searchParams.get('digest') ?? 'sha256:demo'))
    }
    if (path.startsWith('/api/services/') && path.endsWith('/version-inference/refresh') && method === 'POST') {
      const serviceId = decodeURIComponent(path.split('/')[3] ?? '')
      return json({ status: 'pending', serviceId, imageRepo: 'ghcr.io/acme/demo', digest: 'sha256:demo', reason: 'demo' })
    }
    if (path.startsWith('/api/services/') && path.endsWith('/new-version-discovery-timeline') && method === 'GET') {
      return json({ items: [{ kind: 'currentCandidate', version: '5.2.3', occurredAt: nowIso(-86_400_000) }] } satisfies NewVersionDiscoveryTimelineResponse)
    }
    if (path.startsWith('/api/services/') && path.endsWith('/github-releases') && method === 'GET') return json(githubReleases())
    if (path.startsWith('/api/services/') && path.endsWith('/repo-link/infer') && method === 'POST') {
      return json({ repoUrl: 'https://github.com/acme/demo', strategy: 'oci_source', reason: null })
    }
    if (path === '/api/checks' && method === 'POST') return json(createCheckJob())
    if (path === '/api/updates' && method === 'POST') return json(createUpdateJob())
    if (path === '/api/cleanups/scan' && method === 'POST') return json(buildCleanupScan())
    if (path === '/api/cleanups/apply' && method === 'POST') return json({ jobId: createJob('cleanup_apply', { reclaimedBytes: 734 * 1024 * 1024 }).id })
    if (path === '/api/jobs' && method === 'GET') return json({ jobs: listJobItems() })
    if (path.startsWith('/api/jobs/') && method === 'GET') {
      const id = decodeURIComponent(path.split('/').slice(3).join('/'))
      const job = jobsById[id]
      return job ? json({ job }) : json({ error: 'not found' }, { status: 404 })
    }
    if (path === '/api/ignores' && method === 'GET') return json({ rules: ignores })
    if (path === '/api/ignores' && method === 'POST') return json({ ruleId: 'demo-ignore-rule' })
    if (path === '/api/ignores' && method === 'DELETE') return json({ deleted: true })
    if (path === '/api/version-inference/overview' && method === 'GET') return json(versionInferenceOverview)
    if (path === '/api/github-packages/settings' && method === 'GET') return json(githubPackagesSettings)
    if (path === '/api/github-packages/settings' && method === 'PUT') return json({ ok: true })
    if (path === '/api/github-packages/webhook/overview' && method === 'GET') return json(githubWebhookOverview)
    if (path === '/api/github-packages/webhook/deliveries' && method === 'GET') {
      return json({ page: 1, perPage: 25, total: 0, filteredTotal: 0, summary: { processed: 0, ignored: 0, rejected: 0 }, deliveries: [] })
    }
    if (path === '/api/github-packages/repos' && method === 'GET') {
      const repos = [
        { fullName: 'acme/api', selected: true, webhookState: 'ok', hookId: 1001, lastSyncAt: nowIso(-900_000) },
        { fullName: 'acme/web', selected: true, webhookState: 'ok', hookId: 1002, lastSyncAt: nowIso(-900_000) },
        { fullName: 'acme/worker', selected: false, webhookState: 'unknown', hookId: null, lastSyncAt: null },
      ]
      return json({ page: 1, perPage: 25, total: repos.length, filteredTotal: repos.length, selectedTotal: 2, repos })
    }
    if (path === '/api/github-packages/resolve' && method === 'POST') {
      return json({ kind: 'owner', owner: 'acme', warnings: [], repos: [{ fullName: 'acme/demo', selected: false, visibility: 'public', lastActivityAt: nowIso(-86_400_000), ghcrLinked: true, deployed: true }] })
    }
    if (path.startsWith('/api/github-packages/') && method === 'POST') {
      return json({ ok: true, jobId: createJob('github_packages', {}).id, status: 'success', reused: false, affected: 1, results: [] })
    }
    if (path === '/api/web-push/subscriptions' && (method === 'POST' || method === 'DELETE')) return json({ ok: true })
    if (path.startsWith('/api/')) return json({ error: `unhandled demo route: ${method} ${path}` }, { status: 501 })
    return originalFetch(input, init)
  }

  ;(globalThis as { __DOCKREV_APP_DEMO__?: DemoInstallResult }).__DOCKREV_APP_DEMO__ = {
    enabled: true,
    mode: 'app',
  }
  return { enabled: true, mode: 'app' }
}
