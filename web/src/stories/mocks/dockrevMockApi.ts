import type {
  DiscoveredProject,
  IgnoreRule,
  JobDetail,
  JobListItem,
  NotificationConfig,
  ServiceSettings,
  SettingsResponse,
  StackDetail,
  StackListItem,
} from '../../api'

export type DockrevApiScenario =
  | 'default'
  | 'dashboard-demo'
  | 'multi-stack-mixed'
  | 'queue-mixed'
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

function buildQueueMixed(): Fixture {
  const f = buildDashboardDemo()

  const makeJob = (input: Partial<JobListItem> & Pick<JobListItem, 'id' | 'status'>): JobListItem => {
    const base: JobListItem = {
      id: input.id,
      type: input.type ?? 'update',
      scope: input.scope ?? 'service',
      stackId: input.stackId ?? 'stack-prod',
      serviceId: input.serviceId ?? 'svc-prod-api',
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

function buildSettingsConfigured(): Fixture {
  const f = buildDashboardDemo()
  f.notifications = {
    email: { enabled: true, smtpUrl: 'smtp://user:pass@mail.example.com:587/?to=a@example.com&from=Dockrev%20<noreply@example.com>' },
    webhook: { enabled: true, url: 'https://hooks.example.com/dockrev' },
    telegram: { enabled: true, botToken: '123:bot-token', chatId: '987654' },
    webPush: { enabled: true, vapidPublicKey: 'BBOG...mock', vapidPrivateKey: null, vapidSubject: 'mailto:ops@example.com' },
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
  if (scenario === 'queue-mixed') return buildQueueMixed()
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
      return json({ version: '0.0.0-mock' })
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
    if (method === 'POST' && urlPath === '/api/discovery/scan')
      return json({
        startedAt: new Date().toISOString(),
        durationMs: 12,
        summary: { projectsSeen: 0, stacksCreated: 0, stacksUpdated: 0, stacksSkipped: 0, stacksFailed: 0, stacksMarkedMissing: 0 },
        actions: [],
      })
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
