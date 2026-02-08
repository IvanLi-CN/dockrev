import type {
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
  | 'guide-line-long-names'
  | 'resolved-tag-demo'
  | 'version-tags-popover-demo'
  | 'multi-stack-mixed'
  | 'queue-mixed'
  | 'queue-long-logs'
  | 'settings-configured'
  | 'no-candidates'
  | 'empty'
  | 'error'

const realFetch = globalThis.fetch.bind(globalThis)

type MockDebug = {
  lastUpdateRequest: unknown | null
  lastUpdateUrl: string | null
  lastUpdateMethod: string | null
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
    } satisfies JobDetail,
  }

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
      tag: 'latest',
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
      tag: 'latest',
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
    }
    return base
  }

  const jobs: JobListItem[] = [
    makeJob({ id: 'job-running', status: 'running', finishedAt: null, startedAt: nowIso(-20_000), createdAt: nowIso(-40_000) }),
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
    jobs.map((j) => [
      j.id,
      {
        ...j,
        logs:
          j.status === 'failed'
            ? [
                { ts: nowIso(-20_000), level: 'info', msg: 'Pulling images...' },
                { ts: nowIso(-10_000), level: 'error', msg: 'Backup failed (fail-closed).' },
              ]
            : [{ ts: nowIso(-12_000), level: 'info', msg: 'Done.' }],
      } satisfies JobDetail,
    ]),
  )

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
    }
    return base
  }

  const jobShort = makeJob({
    id: 'job-short',
    status: 'running',
    finishedAt: null,
    createdAt: nowIso(-40_000),
    startedAt: nowIso(-20_000),
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
  if (scenario === 'guide-line-long-names') return buildGuideLineLongNames()
  if (scenario === 'resolved-tag-demo') return buildResolvedTagDemo()
  if (scenario === 'version-tags-popover-demo') return buildVersionTagsPopoverDemo()
  if (scenario === 'queue-mixed') return buildQueueMixed()
  if (scenario === 'queue-long-logs') return buildQueueLongLogs()
  if (scenario === 'settings-configured') return buildSettingsConfigured()
  if (scenario === 'multi-stack-mixed') return buildMultiStackMixed()
  return buildDashboardDemo()
}

export function installDockrevMockApi(scenario: DockrevApiScenario) {
  const state = scenario === 'error' ? null : buildFixture(scenario)
  let ignoreSeq = 0
  let jobSeq = 0

  globalThis.__DOCKREV_MOCK_DEBUG__ = { lastUpdateRequest: null, lastUpdateUrl: null, lastUpdateMethod: null }

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
        target: { image: 'ghcr.io/ivanli-cn/dockrev', tag: 'latest', digest: null },
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
    if (method === 'POST' && urlPath === '/api/github-packages/repos/selected') {
      const parsed = parseJsonBody(init?.body) as SetGitHubPackagesRepoSelectedRequest | null
      const fullName = getString(parsed?.fullName)?.trim() ?? ''
      const selected = getBoolean(parsed?.selected)
      if (!fullName || selected === null) return json({ error: 'invalid input' }, { status: 400 })
      const row = f.githubPackagesRepos.find((r) => r.fullName === fullName)
      if (!row) {
        f.githubPackagesRepos.push({ fullName, selected, hookId: null, lastSyncAt: null, lastError: null })
      } else {
        row.selected = selected
      }
      recomputeGithubPackagesCounts()
      return json({ ok: true })
    }
    if (method === 'POST' && urlPath === '/api/github-packages/repos/delete') {
      const parsed = parseJsonBody(init?.body) as { fullName?: unknown } | null
      const fullName = getString(parsed?.fullName)?.trim() ?? ''
      if (!fullName) return json({ error: 'invalid input' }, { status: 400 })
      const idx = f.githubPackagesRepos.findIndex((r) => r.fullName === fullName)
      const row = idx >= 0 ? f.githubPackagesRepos[idx] : null
      if (idx >= 0) f.githubPackagesRepos.splice(idx, 1)
      recomputeGithubPackagesCounts()
      return json({ ok: true, deletedHookIds: row?.hookId ? [row.hookId] : [] })
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
      const body = typeof init?.body === 'string' ? init.body : ''
      const parsed = body ? (JSON.parse(body) as { input?: string }) : null
      const inputStr = typeof parsed?.input === 'string' ? parsed.input.trim() : ''
      if (!inputStr) return json({ error: 'invalid input' }, { status: 400 })
      if (!f.githubPackagesSettings.patMasked) return json({ error: 'pat is required' }, { status: 400 })

      const mkOwner = (owner: string): ResolveGitHubPackagesTargetResponse => ({
        kind: 'owner',
        owner,
        repos: ['dockrev', 'dockrev-supervisor', 'example-private'].map((r) => {
          const fullName = `${owner}/${r}`
          const existing = f.githubPackagesRepos.find((x) => x.fullName === fullName)
          return { fullName, selected: existing?.selected ?? false }
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
            repos: [{ fullName, selected: existing?.selected ?? true }],
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
      const results = selected.map((r) => ({ repo: r.fullName, action: r.hookId ? 'noop' : 'created', hookId: r.hookId ?? 7654321 }))
      const resp: SyncGitHubPackagesWebhooksResponse = { ok: true, results }

      for (const it of results) {
        const rr = f.githubPackagesRepos.find((r) => r.fullName === it.repo)
        if (rr && !rr.hookId) rr.hookId = it.hookId ?? null
        if (rr) rr.lastSyncAt = nowIso()
        if (rr) rr.lastError = null
      }

      return json(resp)
    }

    if (urlPath === '/api/version' && method === 'GET') {
      // Use an existing repo tag so the version link in UI can be exercised in Storybook.
      return json({ version: '0.5.0' })
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
      globalThis.__DOCKREV_MOCK_DEBUG__ = {
        lastUpdateRequest: parsed,
        lastUpdateUrl: urlPath,
        lastUpdateMethod: method,
      }
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
      f.jobById[jobId] = { ...job, logs: [{ ts: startedAt, level: 'info', msg: 'discovery scan finished' }] }
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

    // service candidates
    if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/candidates')) {
      const parts = urlPath.split('/').filter(Boolean)
      const serviceId = decodeURIComponent(parts[2])
      const found = findService(serviceId)
      if (!found) return json({ error: 'not found' }, { status: 404 })

      const base = found.svc.candidate
      const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`
      const candidateDigest = base?.digest ?? d('b', '9f')
      const candidates =
        scenario === 'version-tags-popover-demo' && serviceId === 'svc-version-tags'
          ? [
              { tag: 'v0.8.9-arm64', digest: d('b', 'b0'), archMatch: 'match', arch: ['linux/arm64'], ignored: false },
              { tag: 'v0.8.8-arm64', digest: candidateDigest, archMatch: 'match', arch: ['linux/arm64'], ignored: false },
              { tag: 'v0.8.8', digest: candidateDigest, archMatch: 'match', arch: ['linux/arm64'], ignored: false },
              { tag: '0.8.8', digest: candidateDigest, archMatch: 'match', arch: ['linux/arm64'], ignored: false },
            ]
          : serviceId === 'svc-prod-api'
          ? [
              { tag: '5.3.0', digest: d('b', 'b0'), archMatch: 'match', arch: ['linux/amd64'], ignored: false },
              { tag: '5.2.4', digest: d('b', 'a0'), archMatch: 'match', arch: ['linux/amd64'], ignored: false },
              { tag: '5.2.3', digest: d('b', '9f'), archMatch: 'match', arch: ['linux/amd64'], ignored: false },
            ]
          : base
            ? [{ ...base, ignored: false }]
            : []

      return json({ candidates })
    }

    // service digest tags (used by version popovers)
    if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/digest-tags')) {
      const parts = urlPath.split('/').filter(Boolean)
      const serviceId = decodeURIComponent(parts[2])
      const found = findService(serviceId)
      if (!found) return json({ error: 'not found' }, { status: 404 })

      const digest = (url?.searchParams.get('digest') ?? '').trim()
      if (!digest) return json({ error: 'digest is required' }, { status: 400 })
      const digestNorm = digest.includes(':') ? digest : `sha256:${digest}`

      const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

      // Keep it deterministic: map known digests to many tags so Storybook can exercise long lists.
      const tags =
        digestNorm === d('c', 'c2')
          ? (() => {
              const out: string[] = ['5.2', 'v5.2.1', 'stable', 'latest']
              for (let i = 0; i < 40; i++) out.push(`5.2.${i}`)
              return out
            })()
          : digestNorm === d('a', 'b1')
            ? ['5.2.1', 'v5.2.1']
            : digestNorm === d('b', '9f') && scenario === 'version-tags-popover-demo' && serviceId === 'svc-version-tags'
              ? ['v0.8.8-arm64', 'v0.8.8', '0.8.8', 'stable', 'latest']
            : digestNorm === `sha256:${'a'.repeat(64)}`
              ? ['v0.1.8', '0.1.8']
              : [found.svc.image.tag]

      return json({
        digest: digestNorm,
        tags,
        scan: {
          repoTagsTotal: tags.length,
          manifestsOk: tags.length,
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
