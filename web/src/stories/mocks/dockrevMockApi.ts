import type {
  DeployCheckReportResponse,
  DeployWelcomeResponse,
  DiscoveredProject,
  GitHubPackagesRepo,
  GitHubPackagesSettingsResponse,
  IgnoreRule,
  JobDetail,
  JobListItem,
  ListGitHubPackagesReposResponse,
  NotificationConfig,
  BulkSetGitHubPackagesReposSelectedRequest,
  PutGitHubPackagesSettingsRequest,
  SetGitHubPackagesRepoSelectedRequest,
  AddGitHubPackagesTargetRequest,
  RemoveGitHubPackagesTargetRequest,
  ResolveGitHubPackagesTargetResponse,
  ServiceSettings,
  SettingsResponse,
  StackDetail,
  StackListItem,
  SyncGitHubPackagesWebhooksResponse,
} from '../../api'

export type DockrevApiScenario =
  | 'default'
  | 'dashboard-demo'
  | 'services-inference-pending-candidate-loading'
  | 'service-detail-compose-fallbacks'
  | 'service-detail-version-anomaly'
  | 'guide-line-long-names'
  | 'resolved-tag-demo'
  | 'version-inference-overview'
  | 'version-inference-resync-required'
  | 'version-inference-idle'
  | 'version-inference-running'
  | 'version-inference-queue-backlog'
  | 'version-inference-stale-all-failed'
  | 'version-tags-popover-demo'
  | 'version-tags-popover-snapshot-pending'
  | 'version-tags-popover-snapshot-missing'
  | 'multi-stack-mixed'
  | 'queue-mixed'
  | 'queue-legacy-progress'
  | 'queue-long-logs'
  | 'settings-configured'
  | 'settings-configured-resolve-slow'
  | 'no-candidates'
  | 'empty'
  | 'error'

const realFetch = globalThis.fetch.bind(globalThis)

type ParsedSseEvent = {
  id: string
  event: string
  data: string
}

function parseSsePayload(payload: string): ParsedSseEvent[] {
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

class MockEventSource extends EventTarget {
  static readonly CONNECTING = 0
  static readonly OPEN = 1
  static readonly CLOSED = 2

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
  private polling = false
  private lastEventId = 0
  private pollTimer: number | null = null

  constructor(url: string | URL, eventSourceInitDict?: EventSourceInit) {
    super()
    this.url = String(url)
    this.withCredentials = eventSourceInitDict?.withCredentials === true
    this.connect()
    this.pollTimer = window.setInterval(() => {
      this.connect()
    }, 4_000)
  }

  close() {
    this.closed = true
    this.readyState = MockEventSource.CLOSED
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
      if (this.lastEventId > 0) u.searchParams.set('afterId', String(this.lastEventId))
      const resp = await globalThis.fetch(u.toString(), {
        method: 'GET',
        credentials: this.withCredentials ? 'include' : 'same-origin',
        headers: { Accept: 'text/event-stream' },
      })
      if (!resp.ok) throw new Error(`SSE request failed: ${resp.status}`)
      const payload = await resp.text()
      this.emitOpen()
      for (const evt of parseSsePayload(payload)) {
        this.emitMessage(evt.event || 'message', evt.data, evt.id)
        const parsedId = Number.parseInt(evt.id, 10)
        if (Number.isFinite(parsedId) && parsedId > this.lastEventId) this.lastEventId = parsedId
      }
    } catch {
      this.emitError()
    } finally {
      this.polling = false
    }
  }
}

type MockDebug = {
  lastUpdateRequest: unknown | null
  lastUpdateUrl: string | null
  lastUpdateMethod: string | null
  digestTagsSnapshotCalls: number
  digestTagsCalls: number
  lastDigestTagsSnapshotUrl: string | null
  lastDigestTagsUrl: string | null
}

type VersionInferenceTaskProgressMock = {
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

type VersionInferenceTaskMock = {
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

type VersionInferenceOverviewRowMock = {
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

type VersionInferenceOverviewMock = {
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

type VersionInferenceEventMock = {
  id: number
  data: Record<string, unknown>
}

declare global {
  var __DOCKREV_MOCK_DEBUG__: MockDebug | undefined
}

type Fixture = {
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
  deployCheckReport: DeployCheckReportResponse
  deployWelcome: DeployWelcomeResponse
  versionInferenceOverview: VersionInferenceOverviewMock
  versionInferenceEvents: VersionInferenceEventMock[]
}

function json(data: unknown, init?: ResponseInit) {
  return new Response(JSON.stringify(data), {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers ?? {}),
    },
  })
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null
}

function parseJsonBody(body: unknown): unknown {
  if (typeof body !== 'string' || !body) return null
  try {
    return JSON.parse(body) as unknown
  } catch {
    return null
  }
}

function getString(v: unknown): string | null {
  return typeof v === 'string' ? v : null
}

function getBoolean(v: unknown): boolean | null {
  return typeof v === 'boolean' ? v : null
}

function nowIso(offsetMs = 0) {
  return new Date(Date.now() + offsetMs).toISOString()
}

function makeMockDebug(): MockDebug {
  return {
    lastUpdateRequest: null,
    lastUpdateUrl: null,
    lastUpdateMethod: null,
    digestTagsSnapshotCalls: 0,
    digestTagsCalls: 0,
    lastDigestTagsSnapshotUrl: null,
    lastDigestTagsUrl: null,
  }
}

function makeDefaultSettings(): SettingsResponse {
  return {
    backup: { enabled: true, requireSuccess: true, baseDir: '/var/lib/dockrev/backup', skipTargetsOverBytes: 104857600 },
    auth: { forwardHeaderName: 'X-Forwarded-User', allowAnonymousInDev: true },
  }
}

function makeDefaultNotifications(): NotificationConfig {
  return {
    email: { enabled: false, smtpUrl: null },
    webhook: { enabled: false, url: null },
    telegram: { enabled: false, botToken: null, chatId: null },
    webPush: { enabled: false, vapidPublicKey: null, vapidPrivateKey: null, vapidSubject: null },
  }
}

function makeDefaultGitHubPackagesSettings(): GitHubPackagesSettingsResponse {
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

function makeDefaultDeployCheckReport(): DeployCheckReportResponse {
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

function makeDefaultDeployWelcome(): DeployWelcomeResponse {
  return {
    // Keep Storybook on existing pages by default.
    neverAutoOpen: true,
    updatedAt: null,
  }
}

function summarizeVersionInferenceRows(rows: VersionInferenceOverviewRowMock[]) {
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

function makeVersionInferenceOverview(input?: {
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

function baseEmpty(): Fixture {
  return {
    stacks: [],
    stackById: {},
    jobs: [],
    jobById: {},
    ignores: [],
    discoveredProjects: [],
    settings: makeDefaultSettings(),
    notifications: makeDefaultNotifications(),
    githubPackagesSettings: makeDefaultGitHubPackagesSettings(),
    githubPackagesRepos: [],
    serviceSettingsById: {},
    deployCheckReport: makeDefaultDeployCheckReport(),
    deployWelcome: makeDefaultDeployWelcome(),
    versionInferenceOverview: makeVersionInferenceOverview(),
    versionInferenceEvents: [],
  }
}

function buildDashboardDemo(): Fixture {
  const f = baseEmpty()
  const lastCheckAt = '2026-01-18T06:10:00.000Z'

  const prodStackId = 'stack-prod'
  const infraStackId = 'stack-infra'

  const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

  const serviceProdApi = {
    id: 'svc-prod-api',
    name: 'api',
    image: { ref: 'ghcr.io/acme/api', tag: '5.2.1', digest: d('a', 'b1') },
    candidate: { tag: '5.2.3', digest: d('b', '9f'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    settings: { autoRollback: true, backupTargets: { bindPaths: { '/var/lib/api/data': 'inherit' }, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const serviceProdWeb = {
    id: 'svc-prod-web',
    name: 'web',
    image: { ref: 'harbor.local/ops/web', tag: '5.2', digest: d('c', 'c2') },
    candidate: { tag: '5.2.7', digest: d('d', '7a'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    settings: { autoRollback: true, backupTargets: { bindPaths: { '/var/lib/web/uploads': 'force' }, volumeNames: { 'vol:web-data': 'inherit' } } },
  } satisfies StackDetail['services'][number]

  const serviceProdWorker = {
    id: 'svc-prod-worker',
    name: 'worker',
    image: { ref: 'ghcr.io/acme/worker', tag: '5.2.0', digest: d('e', 'aa') },
    candidate: { tag: '5.2.2', digest: d('f', '0d'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: { matched: true, ruleId: 'ignore-prod-worker', reason: '备份失败（fail-closed）' },
    settings: { autoRollback: false, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const prodDetail = {
    id: prodStackId,
    name: 'prod',
    compose: { type: 'path', composeFiles: ['/srv/app/compose.yml'], envFile: '/srv/app/.env' },
    services: [serviceProdApi, serviceProdWeb, serviceProdWorker],
  } satisfies StackDetail

  const infraSvcA = {
    id: 'svc-infra-loki',
    name: 'loki',
    image: { ref: 'ghcr.io/grafana/loki', tag: '2.9.0', digest: 'sha256:1111111111111111111111111111111111111111111111111111111111111111' },
    candidate: { tag: '2.9.1', digest: 'sha256:2222222222222222222222222222222222222222222222222222222222222222', archMatch: 'unknown', arch: ['linux/amd64', 'linux/arm64'] },
    ignore: null,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const infraSvcB = {
    id: 'svc-infra-prom',
    name: 'prometheus',
    image: { ref: 'quay.io/prometheus/prometheus', tag: '2.49.0', digest: 'sha256:3333333333333333333333333333333333333333333333333333333333333333' },
    candidate: { tag: '2.50.0', digest: 'sha256:4444444444444444444444444444444444444444444444444444444444444444', archMatch: 'mismatch', arch: ['linux/arm64'] },
    ignore: null,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const infraSvcC = {
    id: 'svc-infra-postgres',
    name: 'postgres',
    image: { ref: 'docker.io/library/postgres', tag: '16', digest: d('p', '16') },
    candidate: { tag: '18.1', digest: d('p', '18'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const infraDetail = {
    id: infraStackId,
    name: 'infra',
    compose: { type: 'path', composeFiles: ['/srv/app/compose.yml'], envFile: '/srv/app/.env' },
    services: [infraSvcA, infraSvcB, infraSvcC],
  } satisfies StackDetail

  const prodListItem = {
    id: prodStackId,
    name: 'prod',
    status: 'healthy',
    services: prodDetail.services.length,
    updates: 2,
    lastCheckAt,
  } satisfies StackListItem

  const infraListItem = {
    id: infraStackId,
    name: 'infra',
    status: 'healthy',
    services: infraDetail.services.length,
    updates: 0,
    lastCheckAt,
  } satisfies StackListItem

  const ignoreRule = {
    id: 'ignore-prod-worker',
    enabled: true,
    scope: { type: 'service', serviceId: serviceProdWorker.id },
    match: { kind: 'regex', value: '.*' },
    note: 'blocked via mock',
  } satisfies IgnoreRule

  f.stacks = [prodListItem, infraListItem]
  f.stackById = { [prodStackId]: prodDetail, [infraStackId]: infraDetail }
  f.ignores = [ignoreRule]
  f.serviceSettingsById = {
    [serviceProdApi.id]: serviceProdApi.settings,
    [serviceProdWeb.id]: serviceProdWeb.settings,
    [serviceProdWorker.id]: serviceProdWorker.settings,
    [infraSvcA.id]: infraSvcA.settings,
    [infraSvcB.id]: infraSvcB.settings,
    [infraSvcC.id]: infraSvcC.settings,
  }

  f.discoveredProjects = [
    {
      project: 'missing-compose',
      status: 'missing',
      stackId: null,
      configFiles: ['/srv/missing/docker-compose.yml'],
      lastSeenAt: nowIso(-600_000),
      lastScanAt: nowIso(-300_000),
      lastError: 'compose file not found',
      archived: false,
    },
    {
      project: 'invalid-compose',
      status: 'invalid',
      stackId: null,
      configFiles: ['/srv/invalid/docker-compose.yml'],
      lastSeenAt: nowIso(-520_000),
      lastScanAt: nowIso(-290_000),
      lastError: 'yaml parse error: unexpected indent',
      archived: false,
    },
  ]

  const job1 = {
    id: 'job-1',
    type: 'update',
    scope: 'service',
    stackId: prodStackId,
    serviceId: serviceProdApi.id,
    status: 'running',
    createdBy: 'ivan',
    reason: 'ui',
    createdAt: nowIso(-60_000),
    startedAt: nowIso(-30_000),
    finishedAt: null,
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary: {},
  } satisfies JobListItem

  f.jobs = [job1]
  f.jobById = {
    [job1.id]: {
      ...job1,
      logs: [
        { ts: nowIso(-28_000), level: 'info', msg: 'Pulling images...' },
        { ts: nowIso(-12_000), level: 'info', msg: 'Waiting for healthcheck...' },
      ],
      logsLastId: 2,
    } satisfies JobDetail,
  }

  const now = nowIso()
  f.versionInferenceOverview = makeVersionInferenceOverview({
    rows: [
      {
        key: 'ghcr.io/acme/api@linux/amd64',
        imageRepo: 'ghcr.io/acme/api',
        hostPlatform: 'linux/amd64',
        status: 'ready',
        serviceCount: 1,
        checkedAt: nowIso(-4 * 60 * 1000),
      },
      {
        key: 'harbor.local/ops/web@linux/amd64',
        imageRepo: 'harbor.local/ops/web',
        hostPlatform: 'linux/amd64',
        status: 'queued',
        serviceCount: 1,
        reason: 'new_version',
        updatedAt: nowIso(-20 * 1000),
      },
      {
        key: 'ghcr.io/acme/worker@linux/amd64',
        imageRepo: 'ghcr.io/acme/worker',
        hostPlatform: 'linux/amd64',
        status: 'stale',
        serviceCount: 1,
        checkedAt: nowIso(-8 * 24 * 60 * 60 * 1000),
        reason: 'cache_stale',
      },
      {
        key: 'quay.io/prometheus/prometheus@linux/amd64',
        imageRepo: 'quay.io/prometheus/prometheus',
        hostPlatform: 'linux/amd64',
        status: 'stale',
        serviceCount: 1,
        checkedAt: nowIso(-9 * 24 * 60 * 60 * 1000),
        reason: 'cache_stale',
      },
    ],
    tasks: [
      {
        key: 'harbor.local/ops/web@linux/amd64',
        imageRepo: 'harbor.local/ops/web',
        hostPlatform: 'linux/amd64',
        status: 'queued',
        reason: 'new_version',
        enqueuedAt: nowIso(-30 * 1000),
        updatedAt: nowIso(-20 * 1000),
      },
    ],
    gc: {
      lastRunAt: nowIso(-5 * 60 * 1000),
      lastDeleted: 2,
      lastDurationMs: 31,
      lastError: null,
    },
  })
  f.versionInferenceEvents = [
    {
      id: 41,
      data: {
        type: 'task_enqueued',
        ts: nowIso(-30 * 1000),
        key: 'harbor.local/ops/web@linux/amd64',
        imageRepo: 'harbor.local/ops/web',
        hostPlatform: 'linux/amd64',
        reason: 'new_version',
      },
    },
    {
      id: 42,
      data: {
        type: 'gc_ran',
        ts: nowIso(-5 * 60 * 1000),
        cutoff: nowIso(-30 * 24 * 60 * 60 * 1000),
        deleted: 2,
        durationMs: 31,
        ok: true,
      },
    },
    {
      id: 43,
      data: {
        type: 'task_progress',
        ts: now,
        key: 'harbor.local/ops/web@linux/amd64',
        imageRepo: 'harbor.local/ops/web',
        hostPlatform: 'linux/amd64',
        reason: 'new_version',
        phase: 'scan_tags',
        message: 'scanning tags',
        current: 4,
        total: 10,
        percent: 40,
        updatedAt: now,
      },
    },
  ]

  return f
}

function buildGuideLineLongNames(): Fixture {
  const f = buildDashboardDemo()

  const prod = f.stackById['stack-prod']
  if (prod) {
    prod.services = prod.services.map((svc) =>
      svc.id === 'svc-prod-api'
        ? {
            ...svc,
            name: 'api-gateway-edge-proxy-with-a-very-very-long-service-name-that-should-wrap-to-two-lines',
          }
        : svc
    )
  }

  const infra = f.stackById['stack-infra']
  if (infra) {
    infra.services = infra.services.map((svc) =>
      svc.id === 'svc-infra-prom'
        ? {
            ...svc,
            name: 'prometheus-metrics-exporter-for-kubernetes-cluster-with-a-super-long-service-name',
          }
        : svc
    )
  }

  return f
}

function buildServiceDetailComposeFallbacks(): Fixture {
  const f = buildDashboardDemo()
  const stack = f.stackById['stack-prod']
  if (stack) {
    stack.compose = {
      ...stack.compose,
      composeFiles: [],
      envFile: null,
    }
  }
  return f
}

function buildServiceDetailVersionAnomaly(): Fixture {
  const f = buildDashboardDemo()
  const stack = f.stackById['stack-prod']
  if (!stack) return f

  stack.services = stack.services.map((svc) =>
    svc.id === 'svc-prod-api'
      ? {
          ...svc,
          image: {
            ...svc.image,
            tag: 'latest',
            resolvedTag: 'v0.3.1',
          },
          candidate: svc.candidate
            ? {
                ...svc.candidate,
                tag: 'latest',
                resolvedTag: 'v0.2.53',
              }
            : svc.candidate,
        }
      : svc
  )
  return f
}

function buildNoCandidates(): Fixture {
  const f = baseEmpty()
  const stackId = 'stack-1'
  const lastCheckAt = nowIso(-3_600_000)

  const serviceA = {
    id: 'svc-a',
    name: 'api',
    image: { ref: 'ghcr.io/acme/api', tag: 'v1.2.3', digest: 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' },
    candidate: null,
    ignore: null,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const serviceB = {
    id: 'svc-b',
    name: 'worker',
    image: { ref: 'ghcr.io/acme/worker', tag: 'v2.0.0', digest: 'sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' },
    candidate: null,
    ignore: null,
    settings: { autoRollback: false, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const serviceC = {
    id: 'svc-c',
    name: 'ui',
    image: { ref: 'ghcr.io/acme/ui', tag: 'v0.9.0', digest: null },
    candidate: null,
    ignore: null,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const stackDetail = {
    id: stackId,
    name: 'prod',
    compose: { type: 'path', composeFiles: ['/srv/prod/compose.yml'], envFile: '/srv/prod/.env' },
    services: [serviceA, serviceB, serviceC],
  } satisfies StackDetail

  f.stacks = [
    {
      id: stackId,
      name: 'prod',
      status: 'healthy',
      services: stackDetail.services.length,
      updates: 0,
      lastCheckAt,
    } satisfies StackListItem,
  ]
  f.stackById = { [stackId]: stackDetail }
  f.serviceSettingsById = {
    [serviceA.id]: serviceA.settings,
    [serviceB.id]: serviceB.settings,
    [serviceC.id]: serviceC.settings,
  }
  return f
}

function buildResolvedTagDemo(): Fixture {
  const f = baseEmpty()
  const lastCheckAt = nowIso(-60_000)

  const stackId = 'stack-resolved'
  const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

  const service = {
    id: 'svc-resolved-web',
    name: 'web',
    image: {
      ref: 'ghcr.io/acme/web',
      tag: '5.2',
      digest: d('a', 'b1'),
      resolvedTag: 'v5.2.1',
      resolvedTags: ['v5.2.1', '5.2.1'],
    },
    candidate: { tag: 'v5.2.3', digest: d('b', '9f'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const detail = {
    id: stackId,
    name: 'prod',
    compose: { type: 'path', composeFiles: ['/srv/prod/compose.yml'], envFile: '/srv/prod/.env' },
    services: [service],
  } satisfies StackDetail

  f.stacks = [
    {
      id: stackId,
      name: 'prod',
      status: 'healthy',
      services: detail.services.length,
      updates: 1,
      lastCheckAt,
    } satisfies StackListItem,
  ]
  f.stackById = { [stackId]: detail }
  f.serviceSettingsById = { [service.id]: service.settings }

  return f
}

function buildServicesInferencePendingCandidateLoading(): Fixture {
  const f = baseEmpty()
  const lastCheckAt = nowIso(-60_000)

  const stackId = 'stack-inference-pending'
  const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

  const service = {
    id: 'svc-inference-pending',
    name: 'ai-codex-vibe-monitor',
    image: {
      ref: 'ghcr.io/ivanli-cn/codex-vibe-monitor',
      tag: 'latest',
      digest: d('a', 'b1'),
    },
    candidate: {
      tag: 'latest',
      digest: d('b', '9f'),
      archMatch: 'match',
      arch: ['linux/amd64'],
    },
    versionInference: {
      status: 'pending',
      reason: 'mock',
      checkedAt: nowIso(-20_000),
    },
    ignore: null,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const detail = {
    id: stackId,
    name: 'ai',
    compose: { type: 'path', composeFiles: ['/srv/ai/compose.yml'], envFile: '/srv/ai/.env' },
    services: [service],
  } satisfies StackDetail

  f.stacks = [
    {
      id: stackId,
      name: 'ai',
      status: 'healthy',
      services: 1,
      updates: 1,
      lastCheckAt,
    } satisfies StackListItem,
  ]
  f.stackById = { [stackId]: detail }
  f.serviceSettingsById = { [service.id]: service.settings }

  return f
}

function buildVersionTagsPopoverDemo(): Fixture {
  const f = baseEmpty()
  const lastCheckAt = nowIso(-60_000)

  const stackId = 'stack-version-tags'
  const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

  const service = {
    id: 'svc-version-tags',
    name: 'axonhub',
    image: {
      ref: 'docker.io/looplj/axonhub',
      tag: '0.8',
      digest: d('a', 'b1'),
    },
    candidate: { tag: 'v0.8.8-arm64', digest: d('b', '9f'), archMatch: 'match', arch: ['linux/arm64'] },
    ignore: null,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const detail = {
    id: stackId,
    name: 'ai',
    compose: { type: 'path', composeFiles: ['/srv/ai/compose.yml'], envFile: '/srv/ai/.env' },
    services: [service],
  } satisfies StackDetail

  f.stacks = [
    {
      id: stackId,
      name: 'ai',
      status: 'healthy',
      services: detail.services.length,
      updates: 1,
      lastCheckAt,
    } satisfies StackListItem,
  ]
  f.stackById = { [stackId]: detail }
  f.serviceSettingsById = { [service.id]: service.settings }

  return f
}

function buildQueueMixed(): Fixture {
  const f = buildDashboardDemo()

  const makeJob = (input: Partial<JobListItem> & Pick<JobListItem, 'id' | 'status'>): JobListItem => {
    const base: JobListItem = {
      id: input.id,
      type: input.type ?? 'update',
      scope: input.scope ?? 'service',
      stackId: input.stackId !== undefined ? input.stackId : 'stack-prod',
      serviceId: input.serviceId !== undefined ? input.serviceId : 'svc-prod-api',
      status: input.status,
      createdBy: input.createdBy ?? 'ivan',
      reason: input.reason ?? 'ui',
      createdAt: input.createdAt ?? nowIso(-120_000),
      startedAt: input.startedAt ?? nowIso(-110_000),
      finishedAt: input.finishedAt ?? nowIso(-10_000),
      allowArchMismatch: input.allowArchMismatch ?? false,
      backupMode: input.backupMode ?? 'inherit',
      summary: input.summary ?? {},
      progress: input.progress ?? null,
    }
    return base
  }

  const jobs: JobListItem[] = [
    makeJob({
      id: 'job-running',
      status: 'running',
      finishedAt: null,
      startedAt: nowIso(-20_000),
      createdAt: nowIso(-40_000),
      progress: {
        phase: 'pulling',
        message: 'updating images',
        current: 2,
        total: 5,
        percent: 40,
        plannedCurrent: 4,
        plannedTotal: 5,
        plannedPercent: 80,
        currentTarget: 'worker',
        updatedAt: nowIso(-2_000),
      },
    }),
    makeJob({
      id: 'job-discovery',
      type: 'discovery',
      scope: 'all',
      stackId: null,
      serviceId: null,
      status: 'success',
      createdAt: nowIso(-90_000),
      startedAt: nowIso(-89_000),
      finishedAt: nowIso(-88_000),
      summary: { scan: { startedAt: nowIso(-89_000), durationMs: 12, summary: {}, actions: [] } },
    }),
    makeJob({ id: 'job-success', status: 'success' }),
    makeJob({ id: 'job-failed', status: 'failed' }),
    makeJob({ id: 'job-rolled', status: 'rolled_back' }),
  ]

  f.jobs = jobs
  f.jobById = Object.fromEntries(
    jobs.map((j) => {
      const logs =
        j.status === 'failed'
          ? [
              { ts: nowIso(-20_000), level: 'info', msg: 'Pulling images...' },
              { ts: nowIso(-10_000), level: 'error', msg: 'Backup failed (fail-closed).' },
            ]
          : [{ ts: nowIso(-12_000), level: 'info', msg: 'Done.' }]
      return [
        j.id,
        {
          ...j,
          logs,
          logsLastId: logs.length,
        } satisfies JobDetail,
      ]
    }),
  )

  const now = nowIso()
  f.versionInferenceOverview = makeVersionInferenceOverview({
    rows: [
      {
        key: 'ghcr.io/acme/api@linux/amd64',
        imageRepo: 'ghcr.io/acme/api',
        hostPlatform: 'linux/amd64',
        status: 'running',
        serviceCount: 1,
        reason: 'new_version',
        updatedAt: now,
        progress: {
          phase: 'scan_tags',
          message: 'checking registry tags',
          current: 2,
          total: 5,
          percent: 40,
          updatedAt: now,
        },
      },
      {
        key: 'harbor.local/ops/web@linux/amd64',
        imageRepo: 'harbor.local/ops/web',
        hostPlatform: 'linux/amd64',
        status: 'queued',
        serviceCount: 1,
        reason: 'force',
        updatedAt: nowIso(-20 * 1000),
      },
      {
        key: 'ghcr.io/acme/worker@linux/amd64',
        imageRepo: 'ghcr.io/acme/worker',
        hostPlatform: 'linux/amd64',
        status: 'all_failed',
        serviceCount: 1,
        checkedAt: nowIso(-2 * 60 * 1000),
        reason: 'all_failed',
      },
      {
        key: 'quay.io/prometheus/prometheus@linux/amd64',
        imageRepo: 'quay.io/prometheus/prometheus',
        hostPlatform: 'linux/amd64',
        status: 'stale',
        serviceCount: 1,
        checkedAt: nowIso(-9 * 24 * 60 * 60 * 1000),
        reason: 'cache_stale',
      },
    ],
    tasks: [
      {
        key: 'ghcr.io/acme/api@linux/amd64',
        imageRepo: 'ghcr.io/acme/api',
        hostPlatform: 'linux/amd64',
        status: 'running',
        reason: 'new_version',
        enqueuedAt: nowIso(-40 * 1000),
        startedAt: nowIso(-30 * 1000),
        updatedAt: now,
        progress: {
          phase: 'scan_tags',
          message: 'checking registry tags',
          current: 2,
          total: 5,
          percent: 40,
          updatedAt: now,
        },
      },
      {
        key: 'harbor.local/ops/web@linux/amd64',
        imageRepo: 'harbor.local/ops/web',
        hostPlatform: 'linux/amd64',
        status: 'queued',
        reason: 'force',
        enqueuedAt: nowIso(-20 * 1000),
        updatedAt: nowIso(-20 * 1000),
      },
    ],
    gc: {
      lastRunAt: nowIso(-6 * 60 * 1000),
      lastDeleted: 5,
      lastDurationMs: 55,
      lastError: null,
    },
  })
  f.versionInferenceEvents = [
    {
      id: 100,
      data: {
        type: 'task_started',
        ts: nowIso(-30 * 1000),
        key: 'ghcr.io/acme/api@linux/amd64',
        imageRepo: 'ghcr.io/acme/api',
        hostPlatform: 'linux/amd64',
        reason: 'new_version',
      },
    },
    {
      id: 101,
      data: {
        type: 'task_progress',
        ts: now,
        key: 'ghcr.io/acme/api@linux/amd64',
        imageRepo: 'ghcr.io/acme/api',
        hostPlatform: 'linux/amd64',
        reason: 'new_version',
        phase: 'scan_tags',
        message: 'checking registry tags',
        current: 2,
        total: 5,
        percent: 40,
        updatedAt: now,
      },
    },
  ]

  return f
}

function buildQueueLongLogs(): Fixture {
  const f = buildDashboardDemo()

  const makeJob = (input: Partial<JobListItem> & Pick<JobListItem, 'id' | 'status'>): JobListItem => {
    const base: JobListItem = {
      id: input.id,
      type: input.type ?? 'check',
      scope: input.scope ?? 'all',
      stackId: input.stackId !== undefined ? input.stackId : null,
      serviceId: input.serviceId !== undefined ? input.serviceId : null,
      status: input.status,
      createdBy: input.createdBy ?? 'ivan',
      reason: input.reason ?? 'ui',
      createdAt: input.createdAt ?? nowIso(-120_000),
      startedAt: input.startedAt ?? nowIso(-110_000),
      finishedAt: input.finishedAt ?? nowIso(-10_000),
      allowArchMismatch: input.allowArchMismatch ?? false,
      backupMode: input.backupMode ?? 'inherit',
      summary: input.summary ?? {},
      progress: input.progress ?? null,
    }
    return base
  }

  const jobShort = makeJob({
    id: 'job-short',
    status: 'running',
    finishedAt: null,
    createdAt: nowIso(-40_000),
    startedAt: nowIso(-20_000),
    progress: {
      phase: 'checking',
      message: 'scanning tags',
      current: 7,
      total: 10,
      percent: 70,
      plannedCurrent: 9,
      plannedTotal: 10,
      plannedPercent: 90,
      currentTarget: 'api',
      updatedAt: nowIso(-1_500),
    },
  })

  const jobLong = makeJob({
    id: 'job-long',
    status: 'success',
    createdAt: nowIso(-90_000),
    startedAt: nowIso(-89_000),
    finishedAt: nowIso(-88_000),
  })

  const digest = `sha256:${'9'.repeat(64)}`
  const longToken = `tok_${'a'.repeat(220)}`
  const longImageRef = `ghcr.io/ivanli-cn/example/super/long/repo/name/that/should/wrap@${digest}`
  const longUrl =
    'https://registry.example.com/v2/ivanli-cn/example/manifests/sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef?ns=docker.io&service=registry&scope=repository%3Aivanli-cn%2Fexample%3Apull&offline_token=true&client_id=dockrev-ui&foo=bar&bar=baz&bar2=quux'

  f.jobs = [jobShort, jobLong]
  f.jobById = {
    [jobShort.id]: {
      ...jobShort,
      logs: [{ ts: nowIso(-12_000), level: 'info', msg: 'check started' }],
      logsLastId: 1,
    } satisfies JobDetail,
    [jobLong.id]: {
      ...jobLong,
      logs: [
        { ts: nowIso(-12_000), level: 'info', msg: 'check started' },
        {
          ts: nowIso(-11_500),
          level: 'warn',
          msg: `list tags failed for library/postgres: error sending request for url (${longUrl})`,
        },
        // Keep a long digest line near the top so automated storybook tests can assert it is visible without scrolling.
        { ts: nowIso(-11_000), level: 'warn', msg: digest },
        {
          ts: nowIso(-10_500),
          level: 'warn',
          msg: 'list tags failed for ivanli-cn/catnap: error sending request for url (https://ghcr.io/v2/ivanli-cn/catnap/tags/list)',
        },
        {
          ts: nowIso(-10_200),
          level: 'info',
          msg: `pulling image ${longImageRef}`,
        },
        {
          ts: nowIso(-10_150),
          level: 'warn',
          msg: `retrying request: ${longToken}`,
        },
        {
          ts: nowIso(-10_120),
          level: 'info',
          msg: `running: docker compose pull\n  - service: api\n  - service: worker\n  - service: ui\nprogress: 3/3`,
        },
        {
          ts: nowIso(-10_110),
          level: 'error',
          msg: `panic: unexpected response (429 Too Many Requests)\nstack:\n  at registry_client.rs:123:9\n  at jobs/check.rs:88:17`,
        },
        ...Array.from({ length: 96 }, (_, i) => ({
          ts: nowIso(-10_000 + i * 20),
          level: i % 11 === 0 ? 'error' : i % 7 === 0 ? 'warn' : 'info',
          msg:
            i % 9 === 0
              ? `http error: GET ${longUrl}`
              : i % 5 === 0
                ? `digest mismatch: expected=${digest} got=sha256:${'f'.repeat(64)}`
                : i % 13 === 0
                  ? `json: {"event":"registry_request","status":429,"retry_in_ms":500,"url":"${longUrl}"}`
                  : `line ${String(i + 1).padStart(2, '0')}: ${'x'.repeat(180)}`,
        })),
        { ts: nowIso(-10_000), level: 'info', msg: 'check finished' },
      ],
      logsLastId: 105,
    } satisfies JobDetail,
  }

  return f
}

function buildVersionInferenceOverviewFixture(): Fixture {
  return buildQueueMixed()
}

function buildVersionInferenceResyncRequiredFixture(): Fixture {
  const f = buildQueueMixed()
  const now = nowIso()
  f.versionInferenceEvents = [
    {
      id: 200,
      data: {
        type: 'resync_required',
        ts: now,
        requestedAfterId: 1,
        oldestAvailableId: 180,
        latestEventId: 199,
        reason: 'buffer_overflow',
      },
    },
  ]
  return f
}

function buildVersionInferenceIdleFixture(): Fixture {
  const f = baseEmpty()
  f.versionInferenceOverview = makeVersionInferenceOverview({
    rows: [
      {
        key: 'ghcr.io/acme/api@linux/amd64',
        imageRepo: 'ghcr.io/acme/api',
        hostPlatform: 'linux/amd64',
        status: 'ready',
        serviceCount: 2,
        checkedAt: nowIso(-2 * 60 * 1000),
        updatedAt: nowIso(-2 * 60 * 1000),
      },
    ],
    tasks: [],
    worker: {
      queued: 0,
      running: 0,
      inFlight: 0,
    },
  })
  return f
}

function buildVersionInferenceRunningFixture(): Fixture {
  const f = baseEmpty()
  const now = nowIso()
  f.versionInferenceOverview = makeVersionInferenceOverview({
    rows: [
      {
        key: 'ghcr.io/acme/running@linux/amd64',
        imageRepo: 'ghcr.io/acme/running',
        hostPlatform: 'linux/amd64',
        status: 'running',
        serviceCount: 1,
        reason: 'new_version',
        updatedAt: now,
        progress: {
          phase: 'scanning_manifests',
          message: 'scanning manifests (8/30)',
          current: 8,
          total: 30,
          percent: 26,
          assignedCurrent: 18,
          assignedTotal: 30,
          assignedPercent: 60,
          resultCurrent: 8,
          resultTotal: 30,
          resultPercent: 26,
          updatedAt: now,
        },
      },
      {
        key: 'ghcr.io/acme/cached@linux/amd64',
        imageRepo: 'ghcr.io/acme/cached',
        hostPlatform: 'linux/amd64',
        status: 'ready',
        serviceCount: 1,
        checkedAt: nowIso(-4 * 60 * 1000),
        updatedAt: nowIso(-4 * 60 * 1000),
      },
    ],
    tasks: [
      {
        key: 'ghcr.io/acme/running@linux/amd64',
        imageRepo: 'ghcr.io/acme/running',
        hostPlatform: 'linux/amd64',
        status: 'running',
        reason: 'new_version',
        enqueuedAt: nowIso(-40 * 1000),
        startedAt: nowIso(-35 * 1000),
        updatedAt: now,
        progress: {
          phase: 'scanning_manifests',
          message: 'scanning manifests (8/30)',
          current: 8,
          total: 30,
          percent: 26,
          assignedCurrent: 18,
          assignedTotal: 30,
          assignedPercent: 60,
          resultCurrent: 8,
          resultTotal: 30,
          resultPercent: 26,
          updatedAt: now,
        },
      },
    ],
    worker: {
      queued: 0,
      running: 1,
      inFlight: 1,
    },
  })
  return f
}

function buildVersionInferenceQueueBacklogFixture(): Fixture {
  const f = baseEmpty()
  f.versionInferenceOverview = makeVersionInferenceOverview({
    rows: [
      {
        key: 'ghcr.io/acme/a@linux/amd64',
        imageRepo: 'ghcr.io/acme/a',
        hostPlatform: 'linux/amd64',
        status: 'queued',
        serviceCount: 1,
        reason: 'new_version',
        updatedAt: nowIso(-25 * 1000),
      },
      {
        key: 'ghcr.io/acme/b@linux/amd64',
        imageRepo: 'ghcr.io/acme/b',
        hostPlatform: 'linux/amd64',
        status: 'queued',
        serviceCount: 1,
        reason: 'cache_stale',
        updatedAt: nowIso(-22 * 1000),
      },
      {
        key: 'ghcr.io/acme/c@linux/amd64',
        imageRepo: 'ghcr.io/acme/c',
        hostPlatform: 'linux/amd64',
        status: 'queued',
        serviceCount: 2,
        reason: 'all_failed',
        updatedAt: nowIso(-19 * 1000),
      },
    ],
    tasks: [
      {
        key: 'ghcr.io/acme/a@linux/amd64',
        imageRepo: 'ghcr.io/acme/a',
        hostPlatform: 'linux/amd64',
        status: 'queued',
        reason: 'new_version',
        enqueuedAt: nowIso(-28 * 1000),
        updatedAt: nowIso(-25 * 1000),
      },
      {
        key: 'ghcr.io/acme/b@linux/amd64',
        imageRepo: 'ghcr.io/acme/b',
        hostPlatform: 'linux/amd64',
        status: 'queued',
        reason: 'cache_stale',
        enqueuedAt: nowIso(-24 * 1000),
        updatedAt: nowIso(-22 * 1000),
      },
      {
        key: 'ghcr.io/acme/c@linux/amd64',
        imageRepo: 'ghcr.io/acme/c',
        hostPlatform: 'linux/amd64',
        status: 'queued',
        reason: 'all_failed',
        enqueuedAt: nowIso(-21 * 1000),
        updatedAt: nowIso(-19 * 1000),
      },
    ],
    worker: {
      queued: 3,
      running: 0,
      inFlight: 3,
    },
    gc: {
      lastRunAt: nowIso(-10 * 60 * 1000),
      lastDeleted: 0,
      lastDurationMs: 18,
      lastError: null,
    },
  })
  return f
}

function buildVersionInferenceStaleAllFailedFixture(): Fixture {
  const f = baseEmpty()
  f.versionInferenceOverview = makeVersionInferenceOverview({
    rows: [
      {
        key: 'ghcr.io/acme/stale@linux/amd64',
        imageRepo: 'ghcr.io/acme/stale',
        hostPlatform: 'linux/amd64',
        status: 'stale',
        serviceCount: 1,
        checkedAt: nowIso(-10 * 24 * 60 * 60 * 1000),
        updatedAt: nowIso(-10 * 24 * 60 * 60 * 1000),
        reason: 'cache_stale',
      },
      {
        key: 'ghcr.io/acme/fail@linux/amd64',
        imageRepo: 'ghcr.io/acme/fail',
        hostPlatform: 'linux/amd64',
        status: 'all_failed',
        serviceCount: 1,
        checkedAt: nowIso(-3 * 60 * 1000),
        updatedAt: nowIso(-3 * 60 * 1000),
        reason: 'all_failed',
      },
      {
        key: 'ghcr.io/acme/ready@linux/amd64',
        imageRepo: 'ghcr.io/acme/ready',
        hostPlatform: 'linux/amd64',
        status: 'ready',
        serviceCount: 1,
        checkedAt: nowIso(-2 * 60 * 1000),
        updatedAt: nowIso(-2 * 60 * 1000),
      },
    ],
    tasks: [],
    worker: {
      queued: 0,
      running: 0,
      inFlight: 0,
    },
    gc: {
      lastRunAt: nowIso(-8 * 60 * 1000),
      lastDeleted: 7,
      lastDurationMs: 44,
      lastError: null,
    },
  })
  return f
}

function buildQueueLegacyProgress(): Fixture {
  const f = buildDashboardDemo()

  const legacyJob: JobListItem = {
    id: 'job-legacy-running',
    type: 'check',
    scope: 'service',
    stackId: 'stack-prod',
    serviceId: 'svc-prod-api',
    status: 'running',
    createdBy: 'ivan',
    reason: 'ui',
    createdAt: nowIso(-70_000),
    startedAt: nowIso(-45_000),
    finishedAt: null,
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary: {},
    progress: {
      phase: 'checking',
      message: 'legacy progress payload',
      current: 2,
      total: 5,
      percent: 40,
      currentTarget: 'api',
      updatedAt: nowIso(-1_500),
    },
  }

  const completedJob: JobListItem = {
    id: 'job-legacy-done',
    type: 'check',
    scope: 'service',
    stackId: 'stack-prod',
    serviceId: 'svc-prod-worker',
    status: 'success',
    createdBy: 'ivan',
    reason: 'ui',
    createdAt: nowIso(-140_000),
    startedAt: nowIso(-130_000),
    finishedAt: nowIso(-90_000),
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary: {},
    progress: null,
  }

  f.jobs = [legacyJob, completedJob]
  f.jobById = {
    [legacyJob.id]: {
      ...legacyJob,
      logs: [{ ts: nowIso(-1_500), level: 'info', msg: 'legacy payload received' }],
      logsLastId: 1,
    } satisfies JobDetail,
    [completedJob.id]: {
      ...completedJob,
      logs: [{ ts: nowIso(-91_000), level: 'info', msg: 'done' }],
      logsLastId: 1,
    } satisfies JobDetail,
  }
  return f
}

function buildSettingsConfigured(): Fixture {
  const f = buildDashboardDemo()
  f.notifications = {
    email: { enabled: true, smtpUrl: 'smtp://user:pass@mail.example.com:587/?to=a@example.com&from=Dockrev%20<noreply@example.com>' },
    webhook: { enabled: true, url: 'https://hooks.example.com/dockrev' },
    telegram: { enabled: true, botToken: '123:bot-token', chatId: '987654' },
    webPush: { enabled: true, vapidPublicKey: 'BBOG...mock', vapidPrivateKey: null, vapidSubject: 'mailto:ops@example.com' },
  }
  const repos: GitHubPackagesRepo[] = [
    { fullName: 'IvanLi-CN/dockrev', selected: true, hookId: 1234567, lastSyncAt: nowIso(-60_000), lastError: null },
    { fullName: 'IvanLi-CN/dockrev-supervisor', selected: true, hookId: null, lastSyncAt: null, lastError: null },
    { fullName: 'IvanLi-CN/example-private', selected: true, hookId: null, lastSyncAt: null, lastError: 'permission denied (mock)' },
  ]
  for (let i = 1; i <= 240; i++) {
    repos.push({
      fullName: `IvanLi-CN/repo-${String(i).padStart(3, '0')}`,
      selected: i <= 200,
      hookId: i % 9 === 0 ? 7000000 + i : null,
      lastSyncAt: i % 9 === 0 ? nowIso(-30_000) : null,
      lastError: null,
    })
  }
  f.githubPackagesRepos = repos
  f.githubPackagesSettings = {
    enabled: true,
    callbackUrl: 'https://dockrev.example.com/api/webhooks/github-packages',
    targets: [
      { input: 'IvanLi-CN', kind: 'owner', owner: 'IvanLi-CN', warnings: [] },
      { input: 'https://github.com/IvanLi-CN/dockrev', kind: 'repo', owner: 'IvanLi-CN', warnings: [] },
    ],
    reposTotal: repos.length,
    reposSelectedTotal: repos.filter((r) => r.selected).length,
    patMasked: '******',
    secretMasked: '******',
  }
  return f
}

function buildMultiStackMixed(): Fixture {
  const f = buildDashboardDemo()

  const extraStackId = 'stack-lab'
  const svcOk = {
    id: 'svc-lab-ok',
    name: 'miniflux',
    image: { ref: 'ghcr.io/miniflux/miniflux', tag: '2.2.0', digest: null },
    candidate: null,
    ignore: null,
    archived: false,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]
  const svcArchived = {
    id: 'svc-lab-arch',
    name: 'vaultwarden',
    image: { ref: 'ghcr.io/dani-garcia/vaultwarden', tag: '1.30.0', digest: null },
    candidate: { tag: '1.30.1', digest: 'sha256:9999999999999999999999999999999999999999999999999999999999999999', archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    archived: true,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const labDetail = {
    id: extraStackId,
    name: 'home-lab',
    compose: { type: 'path', composeFiles: ['/srv/lab/compose.yml'], envFile: null },
    services: [svcOk, svcArchived],
  } satisfies StackDetail

  f.stacks = [
    ...f.stacks,
    {
      id: extraStackId,
      name: 'home-lab',
      status: 'healthy',
      services: labDetail.services.length,
      updates: 0,
      lastCheckAt: nowIso(-5_000),
    } satisfies StackListItem,
  ]
  f.stackById[extraStackId] = labDetail
  f.serviceSettingsById[svcOk.id] = svcOk.settings
  f.serviceSettingsById[svcArchived.id] = svcArchived.settings

  f.discoveredProjects = [
    {
      project: 'missing-compose',
      status: 'missing',
      stackId: null,
      configFiles: ['/srv/missing/docker-compose.yml'],
      lastSeenAt: nowIso(-600_000),
      lastScanAt: nowIso(-300_000),
      lastError: 'bind mount missing',
      archived: false,
    },
    {
      project: 'unregistered',
      status: 'active',
      stackId: null,
      configFiles: ['/srv/unregistered/compose.yml'],
      lastSeenAt: nowIso(-90_000),
      lastScanAt: nowIso(-30_000),
      lastError: null,
      archived: false,
    },
  ]

  return f
}

function buildFixture(scenario: Exclude<DockrevApiScenario, 'error'>): Fixture {
  if (scenario === 'empty') return baseEmpty()
  if (scenario === 'no-candidates') return buildNoCandidates()
  if (scenario === 'dashboard-demo') return buildDashboardDemo()
  if (scenario === 'services-inference-pending-candidate-loading') return buildServicesInferencePendingCandidateLoading()
  if (scenario === 'service-detail-compose-fallbacks') return buildServiceDetailComposeFallbacks()
  if (scenario === 'service-detail-version-anomaly') return buildServiceDetailVersionAnomaly()
  if (scenario === 'guide-line-long-names') return buildGuideLineLongNames()
  if (scenario === 'resolved-tag-demo') return buildResolvedTagDemo()
  if (scenario === 'version-inference-overview') return buildVersionInferenceOverviewFixture()
  if (scenario === 'version-inference-resync-required') return buildVersionInferenceResyncRequiredFixture()
  if (scenario === 'version-inference-idle') return buildVersionInferenceIdleFixture()
  if (scenario === 'version-inference-running') return buildVersionInferenceRunningFixture()
  if (scenario === 'version-inference-queue-backlog') return buildVersionInferenceQueueBacklogFixture()
  if (scenario === 'version-inference-stale-all-failed') return buildVersionInferenceStaleAllFailedFixture()
  if (
    scenario === 'version-tags-popover-demo' ||
    scenario === 'version-tags-popover-snapshot-pending' ||
    scenario === 'version-tags-popover-snapshot-missing'
  ) {
    return buildVersionTagsPopoverDemo()
  }
  if (scenario === 'queue-mixed') return buildQueueMixed()
  if (scenario === 'queue-legacy-progress') return buildQueueLegacyProgress()
  if (scenario === 'queue-long-logs') return buildQueueLongLogs()
  if (scenario === 'settings-configured' || scenario === 'settings-configured-resolve-slow') return buildSettingsConfigured()
  if (scenario === 'multi-stack-mixed') return buildMultiStackMixed()
  return buildDashboardDemo()
}

export function installDockrevMockApi(scenario: DockrevApiScenario) {
  const state = scenario === 'error' ? null : buildFixture(scenario)
  let ignoreSeq = 0
  let jobSeq = 0
  const digestSnapshotPendingAttempts = new Map<string, number>()

  globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug()
  if (typeof window !== 'undefined') {
    globalThis.EventSource = MockEventSource as unknown as typeof EventSource
  }

  function findService(serviceId: string) {
    if (!state) return null
    for (const st of Object.values(state.stackById)) {
      const svc = st.services.find((s) => s.id === serviceId)
      if (svc) return { stack: st, svc }
    }
    return null
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
    const recomputeGithubPackagesCounts = () => {
      f.githubPackagesSettings = {
        ...f.githubPackagesSettings,
        reposTotal: f.githubPackagesRepos.length,
        reposSelectedTotal: f.githubPackagesRepos.filter((r) => r.selected).length,
      }
    }
    const ensureGhcrRepoDefaults = (repo: GitHubPackagesRepo) => {
      if (!repo.webhookState) repo.webhookState = 'unknown'
      if (repo.webhookJobId === undefined) repo.webhookJobId = null
      if (repo.lastAuditAt === undefined) repo.lastAuditAt = null
      if (repo.lastOp === undefined) repo.lastOp = null
      if (repo.lastError === undefined) repo.lastError = null
      if (repo.hookId === undefined) repo.hookId = null
      if (repo.lastSyncAt === undefined) repo.lastSyncAt = null
      return repo
    }
    const newGhcrJob = (op: 'register' | 'unregister' | 'audit_all', repoFullName: string | null): JobListItem => {
      jobSeq += 1
      const jobId = `job-ghcr-${jobSeq}`
      const createdAt = nowIso(-200)
      const message = op === 'register' ? 'waiting to register webhook' : op === 'unregister' ? 'waiting to unregister webhook' : 'waiting to audit webhook drift'
      const target = repoFullName ?? '-'
      return {
        id: jobId,
        type: 'github_packages_webhook',
        scope: 'all',
        stackId: null,
        serviceId: null,
        status: 'queued',
        createdBy: 'ivan',
        reason: 'ui',
        createdAt,
        startedAt: null,
        finishedAt: null,
        allowArchMismatch: false,
        backupMode: 'inherit',
        summary: {
          op,
          repos: repoFullName ? [repoFullName] : [],
          progress: {
            phase: 'queued',
            message,
            current: 0,
            total: repoFullName ? 1 : 0,
            percent: 0,
            plannedCurrent: 0,
            plannedTotal: repoFullName ? 1 : 0,
            plannedPercent: 0,
            currentTarget: target,
            updatedAt: createdAt,
          },
        },
        progress: {
          phase: 'queued',
          message,
          current: 0,
          total: repoFullName ? 1 : 0,
          percent: 0,
          plannedCurrent: 0,
          plannedTotal: repoFullName ? 1 : 0,
          plannedPercent: 0,
          currentTarget: target,
          updatedAt: createdAt,
        },
      }
    }
    const insertGhcrQueuedJob = (op: 'register' | 'unregister' | 'audit_all', repoFullName: string | null): string => {
      const job = newGhcrJob(op, repoFullName)
      f.jobs = [job, ...f.jobs]
      f.jobById[job.id] = {
        ...job,
        logs: [
          {
            ts: job.createdAt,
            level: 'event',
            msg: JSON.stringify({
              type: 'job_enqueued',
              jobType: 'github_packages_webhook',
              op,
              target: repoFullName,
              jobId: job.id,
              ts: job.createdAt,
            }),
          },
        ],
        logsLastId: 1,
      }
      return job.id
    }
    const buildGhcrOverview = () => {
      const summary = {
        tracked: 0,
        ok: 0,
        missing: 0,
        error: 0,
        conflict: 0,
        queued: 0,
        running: 0,
        unknown: 0,
      }
      let lastAuditAt: string | null = null
      for (const row of f.githubPackagesRepos) {
        if (!row.selected) continue
        ensureGhcrRepoDefaults(row)
        summary.tracked += 1
        const state = (row.webhookState ?? 'unknown').toLowerCase()
        if (state === 'ok') summary.ok += 1
        else if (state === 'missing') summary.missing += 1
        else if (state === 'error') summary.error += 1
        else if (state === 'conflict') summary.conflict += 1
        else if (state === 'queued') summary.queued += 1
        else if (state === 'running') summary.running += 1
        else summary.unknown += 1
        if (row.lastAuditAt && (!lastAuditAt || row.lastAuditAt > lastAuditAt)) lastAuditAt = row.lastAuditAt
      }

      const ghcrJobs = f.jobs.filter((job) => job.type === 'github_packages_webhook')
      const jobsQueued = ghcrJobs.filter((job) => job.status === 'queued').length
      const jobsRunning = ghcrJobs.filter((job) => job.status === 'running').length
      const runningJobId = ghcrJobs.find((job) => job.status === 'running')?.id ?? null

      return {
        summary,
        jobsQueued,
        jobsRunning,
        runningJobId,
        lastAuditAt,
      }
    }

    // github packages (ghcr) webhook integration
    if (method === 'GET' && urlPath === '/api/github-packages/settings') {
      recomputeGithubPackagesCounts()
      return json(f.githubPackagesSettings)
    }
    if (method === 'PUT' && urlPath === '/api/github-packages/settings') {
      const body = typeof init?.body === 'string' ? init.body : ''
      const parsed = body ? (JSON.parse(body) as PutGitHubPackagesSettingsRequest) : null
      if (parsed) {
        let nextPatMasked: string | null = f.githubPackagesSettings.patMasked ?? null
        if (typeof parsed.pat === 'string' && parsed.pat !== '******' && parsed.pat.trim() !== '') {
          nextPatMasked = '******'
        }
        if (Array.isArray(parsed.targets)) {
          f.githubPackagesSettings.targets = parsed.targets.map((t) => ({
            input: t.input,
            kind: 'owner',
            owner: t.input,
            warnings: [],
          }))
        }
        if (Array.isArray(parsed.repos)) {
          const sel = new Map(parsed.repos.map((r) => [r.fullName, Boolean(r.selected)]))
          for (const r of f.githubPackagesRepos) {
            if (sel.has(r.fullName)) r.selected = Boolean(sel.get(r.fullName))
          }
        }
        f.githubPackagesSettings.enabled = parsed.enabled
        f.githubPackagesSettings.callbackUrl = parsed.callbackUrl
        f.githubPackagesSettings.patMasked = nextPatMasked
        recomputeGithubPackagesCounts()
      }
      return json({ ok: true })
    }
    if (method === 'GET' && urlPath === '/api/github-packages/repos') {
      const params = url?.searchParams ?? new URLSearchParams()
      const page = Math.max(1, Number(params.get('page') ?? '1') || 1)
      const perPage = Math.min(200, Math.max(1, Number(params.get('perPage') ?? '50') || 50))
      const q = (params.get('q') ?? '').trim().toLowerCase()
      const selectedFilter = (params.get('selectedFilter') ?? 'all').trim()

      const matchesQ = (r: GitHubPackagesRepo) => (q ? r.fullName.toLowerCase().includes(q) : true)
      const matchesSelected = (r: GitHubPackagesRepo) => {
        if (selectedFilter === 'selected') return r.selected
        if (selectedFilter === 'unselected') return !r.selected
        return true
      }

      const filtered = f.githubPackagesRepos.filter((r) => matchesQ(r) && matchesSelected(r))
      const offset = (page - 1) * perPage
      const items = filtered.slice(offset, offset + perPage)

      const resp: ListGitHubPackagesReposResponse = {
        page,
        perPage,
        total: f.githubPackagesRepos.length,
        filteredTotal: filtered.length,
        selectedTotal: f.githubPackagesRepos.filter((r) => r.selected).length,
        repos: items,
      }
      recomputeGithubPackagesCounts()
      return json(resp)
    }
    if (method === 'GET' && urlPath === '/api/github-packages/webhook/overview') {
      return json(buildGhcrOverview())
    }
    if (method === 'POST' && urlPath === '/api/github-packages/repos/selected') {
      const parsed = parseJsonBody(init?.body) as SetGitHubPackagesRepoSelectedRequest | null
      const fullName = getString(parsed?.fullName)?.trim() ?? ''
      const selected = getBoolean(parsed?.selected)
      if (!fullName || selected === null) return json({ error: 'invalid input' }, { status: 400 })
      const row = f.githubPackagesRepos.find((r) => r.fullName === fullName)
      if (!row) {
        f.githubPackagesRepos.push(
          ensureGhcrRepoDefaults({
            fullName,
            selected,
            webhookState: selected ? 'queued' : 'unknown',
            webhookJobId: null,
            hookId: null,
            lastSyncAt: null,
            lastAuditAt: null,
            lastOp: selected ? 'register' : null,
            lastError: null,
          }),
        )
      } else {
        ensureGhcrRepoDefaults(row)
        row.selected = selected
      }
      recomputeGithubPackagesCounts()
      let jobId: string | null = null
      if (selected) {
        const target = f.githubPackagesRepos.find((r) => r.fullName === fullName)
        if (target) {
          target.webhookState = 'queued'
          target.lastOp = 'register'
          target.lastError = null
          jobId = insertGhcrQueuedJob('register', fullName)
          target.webhookJobId = jobId
        }
      }
      return json({ ok: true, jobId })
    }
    if (method === 'POST' && urlPath === '/api/github-packages/repos/delete') {
      const parsed = parseJsonBody(init?.body) as { fullName?: unknown } | null
      const fullName = getString(parsed?.fullName)?.trim() ?? ''
      if (!fullName) return json({ error: 'invalid input' }, { status: 400 })
      const row = f.githubPackagesRepos.find((r) => r.fullName === fullName)
      if (!row) return json({ error: 'repo is not tracked' }, { status: 404 })
      ensureGhcrRepoDefaults(row)
      row.webhookState = 'queued'
      row.lastOp = 'unregister'
      row.lastError = null
      const jobId = insertGhcrQueuedJob('unregister', fullName)
      row.webhookJobId = jobId
      recomputeGithubPackagesCounts()
      return json({ ok: true, jobId })
    }
    if (method === 'POST' && urlPath === '/api/github-packages/repos/bulk-selected') {
      const parsed = parseJsonBody(init?.body) as BulkSetGitHubPackagesReposSelectedRequest | null
      const q = (getString(parsed?.q) ?? '').trim().toLowerCase()
      const selectedFilter = (getString(parsed?.selectedFilter) ?? 'all').trim()
      const selected = getBoolean(parsed?.selected)
      if (selected === null) return json({ error: 'invalid input' }, { status: 400 })

      const matchesQ = (r: GitHubPackagesRepo) => (q ? r.fullName.toLowerCase().includes(q) : true)
      const matchesSelected = (r: GitHubPackagesRepo) => {
        if (selectedFilter === 'selected') return r.selected
        if (selectedFilter === 'unselected') return !r.selected
        return true
      }

      let affected = 0
      for (const r of f.githubPackagesRepos) {
        if (!matchesQ(r) || !matchesSelected(r)) continue
        if (r.selected !== selected) {
          r.selected = selected
          affected++
        }
      }
      recomputeGithubPackagesCounts()
      return json({ ok: true, affected })
    }
    if (method === 'POST' && urlPath === '/api/github-packages/targets/add') {
      const parsed = parseJsonBody(init?.body) as AddGitHubPackagesTargetRequest | null
      const inputStr = getString(parsed?.input)?.trim() ?? ''
      if (!inputStr) return json({ error: 'invalid input' }, { status: 400 })
      if (!f.githubPackagesSettings.patMasked) return json({ error: 'pat is required' }, { status: 400 })

      let owner = inputStr
      let repo: string | null = null
      if (inputStr.includes('github.com/')) {
        const m = inputStr.match(/github\.com\/(?:orgs\/)?([^/]+)(?:\/([^/]+))?/i)
        owner = m?.[1] ?? inputStr
        repo = m?.[2]?.replace(/\\.git$/i, '') ?? null
      } else if (inputStr.includes('/')) {
        const parts = inputStr.split('/').filter(Boolean)
        if (parts.length >= 2) {
          owner = parts[0] ?? inputStr
          repo = (parts[1] ?? '').replace(/\\.git$/i, '') || null
        }
      }

      if (!f.githubPackagesSettings.targets.some((t) => t.input === inputStr)) {
        f.githubPackagesSettings.targets.push({
          input: inputStr,
          kind: repo ? 'repo' : 'owner',
          owner,
          warnings: [],
        })
      }

      const before = new Set(f.githubPackagesRepos.map((r) => r.fullName))
      if (repo) {
        const fullName = `${owner}/${repo}`
        if (!before.has(fullName)) f.githubPackagesRepos.push({ fullName, selected: true, hookId: null, lastSyncAt: null, lastError: null })
      } else {
        // add a bunch of repos to simulate "hundreds"
        for (let i = 1; i <= 120; i++) {
          const fullName = `${owner}/added-${String(i).padStart(3, '0')}`
          if (!before.has(fullName)) f.githubPackagesRepos.push({ fullName, selected: true, hookId: null, lastSyncAt: null, lastError: null })
        }
      }

      recomputeGithubPackagesCounts()
      const reposAdded = f.githubPackagesRepos.length - before.size
      return json({ ok: true, kind: repo ? 'repo' : 'owner', owner, reposAdded })
    }
    if (method === 'POST' && urlPath === '/api/github-packages/targets/remove') {
      const parsed = parseJsonBody(init?.body) as RemoveGitHubPackagesTargetRequest | null
      const inputStr = getString(parsed?.input)?.trim() ?? ''
      if (!inputStr) return json({ error: 'invalid input' }, { status: 400 })
      f.githubPackagesSettings.targets = f.githubPackagesSettings.targets.filter((t) => t.input !== inputStr)
      recomputeGithubPackagesCounts()
      return json({ ok: true })
    }
    if (method === 'POST' && urlPath === '/api/github-packages/resolve') {
      if (scenario === 'settings-configured-resolve-slow') {
        await new Promise<void>((resolve) => {
          globalThis.setTimeout(() => resolve(), 900)
        })
      }
      const body = typeof init?.body === 'string' ? init.body : ''
      const parsed = body ? (JSON.parse(body) as { input?: string }) : null
      const inputStr = typeof parsed?.input === 'string' ? parsed.input.trim() : ''
      if (!inputStr) return json({ error: 'invalid input' }, { status: 400 })
      if (!f.githubPackagesSettings.patMasked) return json({ error: 'pat is required' }, { status: 400 })

      const mkOwner = (owner: string): ResolveGitHubPackagesTargetResponse => ({
        kind: 'owner',
        owner,
        repos: f.githubPackagesRepos
          .filter((repo) => repo.fullName.startsWith(`${owner}/`))
          .slice(0, 180)
          .map((repo, idx) => {
            const visibility = repo.fullName.includes('private') || idx % 9 === 0 ? 'private' : 'public'
            const lastActivityAt = idx % 13 === 0 ? null : nowIso(-(idx + 1) * 21_600_000)
            return {
              fullName: repo.fullName,
              selected: repo.selected,
              visibility,
              lastActivityAt,
            }
          }),
        warnings: [],
      })

      if (inputStr.includes('github.com/')) {
        const m = inputStr.match(/github\.com\/(?:orgs\/)?([^/]+)(?:\/([^/]+))?/i)
        const owner = m?.[1] ?? 'unknown'
        const repo = m?.[2]
        if (repo) {
          const fullName = `${owner}/${repo.replace(/\\.git$/i, '')}`
          const existing = f.githubPackagesRepos.find((x) => x.fullName === fullName)
          const resp: ResolveGitHubPackagesTargetResponse = {
            kind: 'repo',
            owner,
            repos: [{ fullName, selected: existing?.selected ?? true, visibility: 'unknown', lastActivityAt: null }],
            warnings: [],
          }
          return json(resp)
        }
        return json(mkOwner(owner))
      }

      return json(mkOwner(inputStr))
    }
    if (method === 'POST' && urlPath === '/api/github-packages/sync') {
      const parsed = parseJsonBody(init?.body) as { repos?: unknown } | null
      const allow = Array.isArray(parsed?.repos)
        ? new Set(parsed?.repos.map((x) => getString(x)?.trim()).filter(Boolean) as string[])
        : null
      const selected = f.githubPackagesRepos.filter((r) => r.selected && (!allow || allow.has(r.fullName)))
      const results = selected.map((r) => {
        ensureGhcrRepoDefaults(r)
        r.webhookState = 'queued'
        r.lastOp = 'register'
        r.lastError = null
        const jobId = insertGhcrQueuedJob('register', r.fullName)
        r.webhookJobId = jobId
        return { repo: r.fullName, action: 'queued', hookId: null, conflictHooks: null, message: `jobId=${jobId}` }
      })
      const resp: SyncGitHubPackagesWebhooksResponse = { ok: true, results }

      return json(resp)
    }

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

      jobSeq += 1
      const jobId = `job-ui-${jobSeq}`
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
      return json({ jobId })
    }

    // discovery
    if (method === 'POST' && urlPath === '/api/discovery/scan') {
      jobSeq += 1
      const jobId = `job-discovery-${jobSeq}`
      const startedAt = nowIso(-500)
      const finishedAt = nowIso(-200)
      const scan = {
        startedAt,
        durationMs: 12,
        summary: { projectsSeen: 0, stacksCreated: 0, stacksUpdated: 0, stacksSkipped: 0, stacksFailed: 0, stacksMarkedMissing: 0 },
        actions: [],
      }
      const job: JobListItem = {
        id: jobId,
        type: 'discovery',
        scope: 'all',
        stackId: null,
        serviceId: null,
        status: 'success',
        createdBy: 'ivan',
        reason: 'ui',
        createdAt: startedAt,
        startedAt,
        finishedAt,
        allowArchMismatch: false,
        backupMode: 'inherit',
        summary: { scan },
      }
      f.jobs = [job, ...f.jobs]
      f.jobById[jobId] = { ...job, logs: [{ ts: startedAt, level: 'info', msg: 'discovery scan finished' }], logsLastId: 1 }
      return json({ jobId })
    }
    if (method === 'GET' && (urlPathWithQuery === '/api/discovery/projects' || urlPathWithQuery.startsWith('/api/discovery/projects?'))) {
      const query = url?.search ? url.search.slice(1) : urlPathWithQuery.includes('?') ? urlPathWithQuery.split('?')[1] : ''
      const params = new URLSearchParams(query)
      const archived = params.get('archived') ?? 'exclude'

      const list = f.discoveredProjects
      let out = list
      if (archived === 'only') out = list.filter((p) => Boolean(p.archived))
      if (archived === 'exclude') out = list.filter((p) => !p.archived)
      return json({ projects: out })
    }
    if (method === 'POST' && urlPath.startsWith('/api/discovery/projects/') && urlPath.endsWith('/archive')) return json({}, { status: 204 })
    if (method === 'POST' && urlPath.startsWith('/api/discovery/projects/') && urlPath.endsWith('/restore')) return json({}, { status: 204 })

    // jobs
    if (method === 'GET' && urlPath === '/api/jobs') return json({ jobs: f.jobs })
    if (method === 'GET' && urlPath.startsWith('/api/jobs/')) {
      const id = decodeURIComponent(urlPath.split('/').slice(3).join('/'))
      const job = f.jobById[id]
      if (!job) return json({ error: 'not found' }, { status: 404 })
      return json({ job })
    }

    // ignores
    if (method === 'GET' && urlPath === '/api/ignores') return json({ rules: f.ignores })
    if (method === 'POST' && urlPath === '/api/ignores') {
      const parsed = parseJsonBody(init?.body)
      const rec = isRecord(parsed) ? parsed : {}
      const scope = isRecord(rec.scope) ? rec.scope : {}
      const match = isRecord(rec.match) ? rec.match : {}
      const serviceId = getString(scope.serviceId)
      ignoreSeq += 1
      const ruleId = `ignore-ui-${ignoreSeq}`
      const rule: IgnoreRule = {
        id: ruleId,
        enabled: getBoolean(rec.enabled) ?? false,
        scope: { type: 'service', serviceId: serviceId ?? 'unknown' },
        match: { kind: getString(match.kind) ?? 'regex', value: getString(match.value) ?? '.*' },
        note: getString(rec.note) ?? null,
      }
      f.ignores = [rule, ...f.ignores]
      if (serviceId) {
        const found = findService(serviceId)
        if (found) {
          found.svc.ignore = { matched: true, ruleId, reason: rule.note ?? 'blocked via UI' }
        }
      }
      return json({ ruleId })
    }
    if (method === 'DELETE' && urlPath === '/api/ignores') {
      const parsed = parseJsonBody(init?.body)
      const rec = isRecord(parsed) ? parsed : {}
      const ruleId = getString(rec.ruleId) ?? ''
      const existing = f.ignores.find((r) => r.id === ruleId) ?? null
      f.ignores = f.ignores.filter((r) => r.id !== ruleId)
      if (existing) {
        const serviceId = existing.scope.serviceId
        const found = findService(serviceId)
        if (found) {
          const still = f.ignores.find((r) => r.scope.serviceId === serviceId) ?? null
          if (still) found.svc.ignore = { matched: true, ruleId: still.id, reason: still.note ?? 'blocked via UI' }
          else found.svc.ignore = null
        }
      }
      return json({ deleted: true })
    }

    // settings
    if (method === 'GET' && urlPath === '/api/settings') return json(f.settings)
    if (method === 'PUT' && urlPath === '/api/settings') {
      const parsed = parseJsonBody(init?.body)
      const rec = isRecord(parsed) ? parsed : null
      const backup = rec && isRecord(rec.backup) ? rec.backup : null
      if (backup) {
        const enabled = getBoolean(backup.enabled)
        const requireSuccess = getBoolean(backup.requireSuccess)
        const baseDir = getString(backup.baseDir)
        const skipTargetsOverBytes = typeof backup.skipTargetsOverBytes === 'number' ? backup.skipTargetsOverBytes : null
        f.settings.backup = {
          enabled: enabled ?? f.settings.backup.enabled,
          requireSuccess: requireSuccess ?? f.settings.backup.requireSuccess,
          baseDir: baseDir ?? f.settings.backup.baseDir,
          skipTargetsOverBytes: skipTargetsOverBytes ?? f.settings.backup.skipTargetsOverBytes,
        }
      }
      return json({ ok: true })
    }

    // deploy welcome / preflight
    if (method === 'GET' && urlPath === '/api/deploy-check/report') {
      return json(f.deployCheckReport)
    }
    if (method === 'GET' && urlPath === '/api/deploy-welcome') {
      return json(f.deployWelcome)
    }
    if (method === 'PUT' && urlPath === '/api/deploy-welcome') {
      const parsed = parseJsonBody(init?.body)
      const rec = isRecord(parsed) ? parsed : {}
      const neverAutoOpen = getBoolean(rec.neverAutoOpen) ?? f.deployWelcome.neverAutoOpen
      f.deployWelcome = { neverAutoOpen, updatedAt: nowIso() }
      return json({ ok: true, ...f.deployWelcome })
    }

    // notifications
    if (method === 'GET' && urlPath === '/api/notifications') return json(f.notifications)
    if (method === 'PUT' && urlPath === '/api/notifications') {
      const parsed = parseJsonBody(init?.body)
      if (isRecord(parsed)) {
        // Best-effort, keep existing values if shape is unexpected.
        const email = isRecord(parsed.email) ? parsed.email : null
        const webhook = isRecord(parsed.webhook) ? parsed.webhook : null
        const telegram = isRecord(parsed.telegram) ? parsed.telegram : null
        const webPush = isRecord(parsed.webPush) ? parsed.webPush : null
        f.notifications = {
          email: {
            enabled: (email && getBoolean(email.enabled)) ?? f.notifications.email.enabled,
            smtpUrl: (email && getString(email.smtpUrl)) ?? f.notifications.email.smtpUrl,
          },
          webhook: {
            enabled: (webhook && getBoolean(webhook.enabled)) ?? f.notifications.webhook.enabled,
            url: (webhook && getString(webhook.url)) ?? f.notifications.webhook.url,
          },
          telegram: {
            enabled: (telegram && getBoolean(telegram.enabled)) ?? f.notifications.telegram.enabled,
            botToken: (telegram && getString(telegram.botToken)) ?? f.notifications.telegram.botToken,
            chatId: (telegram && getString(telegram.chatId)) ?? f.notifications.telegram.chatId,
          },
          webPush: {
            enabled: (webPush && getBoolean(webPush.enabled)) ?? f.notifications.webPush.enabled,
            vapidPublicKey: (webPush && getString(webPush.vapidPublicKey)) ?? f.notifications.webPush.vapidPublicKey,
            vapidPrivateKey: (webPush && getString(webPush.vapidPrivateKey)) ?? f.notifications.webPush.vapidPrivateKey,
            vapidSubject: (webPush && getString(webPush.vapidSubject)) ?? f.notifications.webPush.vapidSubject,
          },
        }
      }
      return json({ ok: true })
    }
    if (method === 'POST' && urlPath === '/api/notifications/test') return json({ ok: true, results: {} })

    // web push
    if (method === 'POST' && urlPath === '/api/web-push/subscriptions') return json({ ok: true })
    if (method === 'DELETE' && urlPath === '/api/web-push/subscriptions') return json({ ok: true })

    // service candidates (removed)
    if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/candidates')) {
      return json({ error: 'not found' }, { status: 404 })
    }

    // service digest tags snapshot (used by version popovers)
    if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/digest-tags-snapshot')) {
      const dbg = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
      dbg.digestTagsSnapshotCalls += 1
      dbg.lastDigestTagsSnapshotUrl = urlPathWithQuery

      const parts = urlPath.split('/').filter(Boolean)
      const serviceId = decodeURIComponent(parts[2])
      const found = findService(serviceId)
      if (!found) return json({ error: 'not found' }, { status: 404 })

      if (scenario === 'version-tags-popover-snapshot-missing') {
        return json({ error: 'not found' }, { status: 404 })
      }

      const digest = (url?.searchParams.get('digest') ?? '').trim()
      const digestNorm = digest ? (digest.includes(':') ? digest : `sha256:${digest}`) : ''
      const isVersionTagsDemoScenario =
        scenario === 'version-tags-popover-demo' || scenario === 'version-tags-popover-snapshot-pending'

      const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

      if (
        scenario === 'version-tags-popover-snapshot-pending' ||
        scenario === 'services-inference-pending-candidate-loading'
      ) {
        const pendingKey = `${serviceId}:${digestNorm || '<missing-digest>'}`
        const attempt = (digestSnapshotPendingAttempts.get(pendingKey) ?? 0) + 1
        digestSnapshotPendingAttempts.set(pendingKey, attempt)
        // Keep pending visible for Storybook verification.
        const maxPendingAttempts = scenario === 'services-inference-pending-candidate-loading' ? 999 : 4
        if (attempt <= maxPendingAttempts) {
          return json(
            {
              status: 'pending',
              digest: digestNorm,
              retryAfterMs: 450,
            },
            { status: 202 },
          )
        }
      }

      // Keep it deterministic:
      // - `repoTags`: all registry tags for the image (superset).
      // - `tags`: tags that match the requested digest (subset).
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
                  // Mimic a real repo: lots of patch tags, plus a few named aliases.
                  const out: string[] = ['5.1', '5.1.10', '5.1.11', '5.1.12', '5.2', 'v5.2.1', 'v5.2.3', 'stable', 'latest']
                  for (let i = 0; i < 40; i++) out.push(`5.2.${i}`)
                  return out
                })()
            : isVersionTagsDemoScenario && serviceId === 'svc-version-tags'
              ? ['v0.8.9-arm64', 'v0.8.8-arm64', 'v0.8.8', '0.8.8', 'stable', 'latest']
              : digestNorm === `sha256:${'a'.repeat(64)}`
                ? ['v0.1.8', '0.1.8']
                : [found.svc.image.tag]

      const tags = !digestNorm
        ? []
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
            : digestNorm === d('b', '9f') && isVersionTagsDemoScenario && serviceId === 'svc-version-tags'
              ? ['v0.8.8-arm64', 'v0.8.8', '0.8.8', 'stable', 'latest']
            : digestNorm === `sha256:${'a'.repeat(64)}`
              ? ['v0.1.8', '0.1.8']
              : [found.svc.image.tag]

      const considered = Math.min(100, repoTags.length)

      return json({
        digest: digestNorm,
        tags,
        checkedAt: nowIso(-5 * 60 * 1000),
        scan: {
          repoTagsTotal: repoTags.length,
          repoTagsConsidered: considered,
          manifestsOk: digestNorm ? considered : 0,
          manifestsTimeout: 0,
          manifestsError: 0,
        },
      })
    }

    // service digest tags (used by version popovers)
    if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/digest-tags')) {
      const dbg = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
      dbg.digestTagsCalls += 1
      dbg.lastDigestTagsUrl = urlPathWithQuery

      const parts = urlPath.split('/').filter(Boolean)
      const serviceId = decodeURIComponent(parts[2])
      const found = findService(serviceId)
      if (!found) return json({ error: 'not found' }, { status: 404 })

      const digest = (url?.searchParams.get('digest') ?? '').trim()
      const digestNorm = digest ? (digest.includes(':') ? digest : `sha256:${digest}`) : ''
      const isVersionTagsDemoScenario =
        scenario === 'version-tags-popover-demo' || scenario === 'version-tags-popover-snapshot-pending'

      const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

      // Keep it deterministic:
      // - `repoTags`: all registry tags for the image (superset).
      // - `tags`: tags that match the requested digest (subset).
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
                  // Mimic a real repo: lots of patch tags, plus a few named aliases.
                  const out: string[] = ['5.1', '5.1.10', '5.1.11', '5.1.12', '5.2', 'v5.2.1', 'v5.2.3', 'stable', 'latest']
                  for (let i = 0; i < 40; i++) out.push(`5.2.${i}`)
                  return out
                })()
            : isVersionTagsDemoScenario && serviceId === 'svc-version-tags'
              ? ['v0.8.9-arm64', 'v0.8.8-arm64', 'v0.8.8', '0.8.8', 'stable', 'latest']
              : digestNorm === `sha256:${'a'.repeat(64)}`
                ? ['v0.1.8', '0.1.8']
                : [found.svc.image.tag]

      const tags = !digestNorm
        ? []
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
            : digestNorm === d('b', '9f') && isVersionTagsDemoScenario && serviceId === 'svc-version-tags'
              ? ['v0.8.8-arm64', 'v0.8.8', '0.8.8', 'stable', 'latest']
            : digestNorm === `sha256:${'a'.repeat(64)}`
              ? ['v0.1.8', '0.1.8']
              : [found.svc.image.tag]

      return json({
        digest: digestNorm,
        tags,
        repoTags,
        scan: {
          repoTagsTotal: repoTags.length,
          repoTagsConsidered: repoTags.length,
          manifestsOk: digestNorm ? repoTags.length : 0,
          manifestsTimeout: 0,
          manifestsError: 0,
        },
      })
    }

    // service settings
    if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/settings')) {
      const parts = urlPath.split('/').filter(Boolean)
      const serviceId = decodeURIComponent(parts[2])
      const st = f.serviceSettingsById[serviceId]
      if (!st) return json({ error: 'not found' }, { status: 404 })
      return json(st)
    }
    if (method === 'PUT' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/settings')) {
      const parts = urlPath.split('/').filter(Boolean)
      const serviceId = decodeURIComponent(parts[2])
      const body = typeof init?.body === 'string' ? init.body : ''
      const parsed = body ? (JSON.parse(body) as ServiceSettings) : null
      if (parsed) f.serviceSettingsById[serviceId] = parsed
      return json({ ok: true })
    }
    if (method === 'POST' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/archive')) {
      const parts = urlPath.split('/').filter(Boolean)
      const serviceId = decodeURIComponent(parts[2])
      const found = findService(serviceId)
      if (found) found.svc.archived = true
      return json({}, { status: 204 })
    }
    if (method === 'POST' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/restore')) {
      const parts = urlPath.split('/').filter(Boolean)
      const serviceId = decodeURIComponent(parts[2])
      const found = findService(serviceId)
      if (found) found.svc.archived = false
      return json({}, { status: 204 })
    }

    return json({ error: `unhandled mock route: ${method} ${urlString}` }, { status: 501 })
  }
}
