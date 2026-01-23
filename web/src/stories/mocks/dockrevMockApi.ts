import type {
  IgnoreRule,
  JobDetail,
  JobListItem,
  NotificationConfig,
  ServiceSettings,
  SettingsResponse,
  StackDetail,
  StackListItem,
} from '../../api'

export type DockrevApiScenario = 'default' | 'empty' | 'error'

const realFetch = globalThis.fetch.bind(globalThis)

type Fixture = {
  stacks: StackListItem[]
  stackById: Record<string, StackDetail>
  jobs: JobListItem[]
  jobById: Record<string, JobDetail>
  ignores: IgnoreRule[]
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

function fixture(scenario: Exclude<DockrevApiScenario, 'error'>): Fixture {
  if (scenario === 'empty') {
    return {
      stacks: [],
      stackById: {},
      jobs: [],
      jobById: {},
      ignores: [],
      settings: {
        backup: { enabled: true, requireSuccess: true, baseDir: '/var/lib/dockrev/backup', skipTargetsOverBytes: 104857600 },
        auth: { forwardHeaderName: 'X-Forwarded-User', allowAnonymousInDev: true },
      },
      notifications: {
        email: { enabled: false, smtpUrl: null },
        webhook: { enabled: false, url: null },
        telegram: { enabled: false, botToken: null, chatId: null },
        webPush: { enabled: false, vapidPublicKey: null, vapidPrivateKey: null, vapidSubject: null },
      },
      serviceSettingsById: {},
    }
  }

  const stackId = 'stack-1'
  const serviceA = {
    id: 'svc-a',
    name: 'api',
    image: { ref: 'ghcr.io/acme/api', tag: 'v1.2.3', digest: 'sha256:1111111111111111111111111111111111111111111111111111111111111111' },
    candidate: { tag: 'v1.2.4', digest: 'sha256:2222222222222222222222222222222222222222222222222222222222222222', archMatch: 'match', arch: ['amd64'] },
    ignore: null,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const serviceB = {
    id: 'svc-b',
    name: 'worker',
    image: { ref: 'ghcr.io/acme/worker', tag: 'v2.0.0', digest: 'sha256:3333333333333333333333333333333333333333333333333333333333333333' },
    candidate: { tag: 'v2.1.0', digest: 'sha256:4444444444444444444444444444444444444444444444444444444444444444', archMatch: 'unknown', arch: ['amd64', 'arm64'] },
    ignore: null,
    settings: { autoRollback: false, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const serviceC = {
    id: 'svc-c',
    name: 'ui',
    image: { ref: 'ghcr.io/acme/ui', tag: 'v0.9.0', digest: null },
    candidate: { tag: 'v1.0.0', digest: 'sha256:5555555555555555555555555555555555555555555555555555555555555555', archMatch: 'mismatch', arch: ['arm64'] },
    ignore: { matched: true, ruleId: 'ignore-1', reason: 'pinned via policy' },
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const stackDetail = {
    id: stackId,
    name: 'prod',
    compose: {
      type: 'path',
      composeFiles: ['/srv/prod/compose.yml'],
      envFile: '/srv/prod/.env',
    },
    services: [serviceA, serviceB, serviceC],
  } satisfies StackDetail

  const stackListItem = {
    id: stackId,
    name: 'prod',
    status: 'healthy',
    services: stackDetail.services.length,
    updates: 2,
    lastCheckAt: new Date().toISOString(),
  } satisfies StackListItem

  const job1 = {
    id: 'job-1',
    type: 'update',
    scope: 'service',
    stackId,
    serviceId: serviceA.id,
    status: 'running',
    createdBy: 'ivan',
    reason: 'ui',
    createdAt: new Date(Date.now() - 60_000).toISOString(),
    startedAt: new Date(Date.now() - 30_000).toISOString(),
    finishedAt: null,
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary: {},
  } satisfies JobListItem

  const jobDetail = {
    ...job1,
    logs: [
      { ts: new Date(Date.now() - 28_000).toISOString(), level: 'info', msg: 'Pulling images...' },
      { ts: new Date(Date.now() - 12_000).toISOString(), level: 'info', msg: 'Waiting for healthcheck...' },
    ],
  } satisfies JobDetail

  const ignoreRule = {
    id: 'ignore-1',
    enabled: true,
    scope: { type: 'service', serviceId: serviceC.id },
    match: { kind: 'regex', value: '.*' },
    note: 'pinned',
  } satisfies IgnoreRule

  const serviceSettingsById = {
    [serviceA.id]: serviceA.settings,
    [serviceB.id]: serviceB.settings,
    [serviceC.id]: serviceC.settings,
  } satisfies Record<string, ServiceSettings>

  const settings = {
    backup: { enabled: true, requireSuccess: true, baseDir: '/var/lib/dockrev/backup', skipTargetsOverBytes: 104857600 },
    auth: { forwardHeaderName: 'X-Forwarded-User', allowAnonymousInDev: true },
  } satisfies SettingsResponse

  const notifications = {
    email: { enabled: false, smtpUrl: null },
    webhook: { enabled: false, url: null },
    telegram: { enabled: false, botToken: null, chatId: null },
    webPush: { enabled: false, vapidPublicKey: null, vapidPrivateKey: null, vapidSubject: null },
  } satisfies NotificationConfig

  return {
    stacks: [stackListItem],
    stackById: { [stackId]: stackDetail },
    jobs: [job1],
    jobById: { [job1.id]: jobDetail },
    ignores: [ignoreRule],
    settings,
    notifications,
    serviceSettingsById,
  }
}

export function installDockrevMockApi(scenario: DockrevApiScenario) {
  globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
    const method = (init?.method ?? (input instanceof Request ? input.method : 'GET')).toUpperCase()
    const urlString = typeof input === 'string' ? input : input instanceof URL ? input.toString() : input.url

    if (!urlString.startsWith('/api/')) return realFetch(input, init)

    if (scenario === 'error') {
      return json({ error: 'mock error' }, { status: 500 })
    }

    const f = fixture(scenario)

    // stacks
    if (method === 'GET' && (urlString === '/api/stacks' || urlString.startsWith('/api/stacks?'))) {
      const query = urlString.includes('?') ? urlString.split('?')[1] : ''
      const params = new URLSearchParams(query)
      const archived = params.get('archived') ?? 'exclude'

      let stacks = f.stacks
      if (archived === 'only') stacks = stacks.filter((s) => Boolean(s.archived))
      if (archived === 'exclude') stacks = stacks.filter((s) => !s.archived)

      return json({ stacks })
    }
    if (method === 'GET' && urlString.startsWith('/api/stacks/')) {
      const id = decodeURIComponent(urlString.split('?')[0].split('/').slice(3).join('/'))
      const st = f.stackById[id]
      if (!st) return json({ error: 'not found' }, { status: 404 })
      return json({ stack: st })
    }
    if (method === 'POST' && urlString.startsWith('/api/stacks/') && urlString.endsWith('/archive')) return json({}, { status: 204 })
    if (method === 'POST' && urlString.startsWith('/api/stacks/') && urlString.endsWith('/restore')) return json({}, { status: 204 })

    // checks / updates
    if (method === 'POST' && urlString === '/api/checks') return json({ checkId: 'check-1' })
    if (method === 'POST' && urlString === '/api/updates') return json({ jobId: 'job-1' })

    // discovery
    if (method === 'POST' && urlString === '/api/discovery/scan')
      return json({
        startedAt: new Date().toISOString(),
        durationMs: 12,
        summary: { projectsSeen: 0, stacksCreated: 0, stacksUpdated: 0, stacksSkipped: 0, stacksFailed: 0, stacksMarkedMissing: 0 },
        actions: [],
      })
    if (method === 'GET' && (urlString === '/api/discovery/projects' || urlString.startsWith('/api/discovery/projects?')))
      return json({ projects: [] })
    if (method === 'POST' && urlString.startsWith('/api/discovery/projects/') && urlString.endsWith('/archive')) return json({}, { status: 204 })
    if (method === 'POST' && urlString.startsWith('/api/discovery/projects/') && urlString.endsWith('/restore')) return json({}, { status: 204 })

    // jobs
    if (method === 'GET' && urlString === '/api/jobs') return json({ jobs: f.jobs })
    if (method === 'GET' && urlString.startsWith('/api/jobs/')) {
      const id = decodeURIComponent(urlString.split('/').slice(3).join('/'))
      const job = f.jobById[id]
      if (!job) return json({ error: 'not found' }, { status: 404 })
      return json({ job })
    }

    // ignores
    if (method === 'GET' && urlString === '/api/ignores') return json({ rules: f.ignores })
    if (method === 'POST' && urlString === '/api/ignores') return json({ ruleId: `ignore-${Math.random().toString(16).slice(2)}` })
    if (method === 'DELETE' && urlString === '/api/ignores') return json({ deleted: true })

    // settings
    if (method === 'GET' && urlString === '/api/settings') return json(f.settings)
    if (method === 'PUT' && urlString === '/api/settings') return json({ ok: true })

    // notifications
    if (method === 'GET' && urlString === '/api/notifications') return json(f.notifications)
    if (method === 'PUT' && urlString === '/api/notifications') return json({ ok: true })
    if (method === 'POST' && urlString === '/api/notifications/test') return json({ ok: true, results: {} })

    // web push
    if (method === 'POST' && urlString === '/api/web-push/subscriptions') return json({ ok: true })
    if (method === 'DELETE' && urlString === '/api/web-push/subscriptions') return json({ ok: true })

    // service settings
    if (method === 'GET' && urlString.startsWith('/api/services/') && urlString.endsWith('/settings')) {
      const parts = urlString.split('/').filter(Boolean)
      const serviceId = decodeURIComponent(parts[2])
      const st = f.serviceSettingsById[serviceId]
      if (!st) return json({ error: 'not found' }, { status: 404 })
      return json(st)
    }
    if (method === 'PUT' && urlString.startsWith('/api/services/') && urlString.endsWith('/settings')) return json({ ok: true })
    if (method === 'POST' && urlString.startsWith('/api/services/') && urlString.endsWith('/archive')) return json({}, { status: 204 })
    if (method === 'POST' && urlString.startsWith('/api/services/') && urlString.endsWith('/restore')) return json({}, { status: 204 })

    return json({ error: `unhandled mock route: ${method} ${urlString}` }, { status: 501 })
  }
}
