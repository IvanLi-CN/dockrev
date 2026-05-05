import type { IgnoreRule, JobDetail, JobListItem, StackDetail, StackListItem } from '../../../api'
import {
  makeDefaultDeployCheckReport,
  makeDefaultDeployWelcome,
  makeDefaultGitHubPackagesSettings,
  makeDefaultNotifications,
  makeDefaultSettings,
  makeVersionInferenceOverview,
  nowIso,
  type Fixture,
} from './shared'

export function baseEmpty(): Fixture {
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
    stackSettingsById: {},
    rollbackTargetByServiceId: {},
    repoLinkInferenceByServiceId: {},
    serviceTagSuggestionsById: {},
    deployCheckReport: makeDefaultDeployCheckReport(),
    deployWelcome: makeDefaultDeployWelcome(),
    versionInferenceOverview: makeVersionInferenceOverview(),
    versionInferenceEvents: [],
  }
}

export function buildDashboardDemo(): Fixture {
  const f = baseEmpty()
  const lastCheckAt = '2026-01-18T06:10:00.000Z'

  const prodStackId = 'stack-prod'
  const infraStackId = 'stack-infra'

  const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

  const serviceProdApi = {
    id: 'svc-prod-api',
    name: 'api',
    image: { ref: 'ghcr.io/acme/api:5.2.1', tag: '5.2.1', digest: d('a', 'b1') },
    candidate: { tag: '5.2.3', digest: d('b', '9f'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    homepage: {
      group: 'Developer',
      name: 'Acme API',
      icon: 'si-github',
      href: 'https://api.example.com',
      description: 'Primary API gateway',
    },
    settings: { autoRollback: true, backupTargets: { bindPaths: { '/var/lib/api/data': 'inherit' }, volumeNames: {} }, repoUrl: null },
  } satisfies StackDetail['services'][number]

  const serviceProdWeb = {
    id: 'svc-prod-web',
    name: 'web',
    image: { ref: 'harbor.local/ops/web', tag: '5.2', digest: d('c', 'c2') },
    candidate: { tag: '5.2.7', digest: d('d', '7a'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    homepage: {
      group: 'Frontend',
      name: 'Web Console',
      icon: 'mdi-monitor-dashboard',
      href: 'https://web.example.com',
      description: 'User-facing dashboard',
    },
    settings: { autoRollback: true, backupTargets: { bindPaths: { '/var/lib/web/uploads': 'force' }, volumeNames: { 'vol:web-data': 'inherit' } }, repoUrl: null },
  } satisfies StackDetail['services'][number]

  const serviceProdWorker = {
    id: 'svc-prod-worker',
    name: 'worker',
    image: { ref: 'ghcr.io/acme/worker:5.2.0', tag: '5.2.0', digest: d('e', 'aa') },
    candidate: { tag: '5.2.2', digest: d('f', '0d'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: { matched: true, ruleId: 'ignore-prod-worker', reason: '备份失败（fail-closed）' },
    settings: { autoRollback: false, backupTargets: { bindPaths: {}, volumeNames: {} }, repoUrl: 'https://github.com/acme/worker' },
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
    homepage: {
      group: 'Monitoring',
      name: 'Loki',
      icon: 'mdi-file-document-multiple-outline',
      href: 'https://logs.example.com',
      description: 'Centralized logs',
    },
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} }, repoUrl: 'https://github.com/grafana/loki' },
  } satisfies StackDetail['services'][number]

  const infraSvcB = {
    id: 'svc-infra-prom',
    name: 'prometheus',
    image: { ref: 'quay.io/prometheus/prometheus', tag: '2.49.0', digest: 'sha256:3333333333333333333333333333333333333333333333333333333333333333' },
    candidate: { tag: '2.50.0', digest: 'sha256:4444444444444444444444444444444444444444444444444444444444444444', archMatch: 'mismatch', arch: ['linux/arm64'] },
    ignore: null,
    homepage: {
      group: 'Monitoring',
      name: 'Prometheus',
      icon: 'prometheus.svg',
      href: 'https://metrics.example.com',
      description: 'Metrics and alerting',
    },
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} }, repoUrl: null },
  } satisfies StackDetail['services'][number]

  const infraSvcC = {
    id: 'svc-infra-postgres',
    name: 'postgres',
    image: { ref: 'docker.io/library/postgres:16', tag: '16', digest: d('p', '16') },
    candidate: { tag: '18.1', digest: d('p', '18'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    homepage: {
      group: 'Data',
      name: 'Postgres',
      icon: 'https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/svg/postgres.svg',
      href: 'https://db.example.com',
      description: 'Primary relational database',
    },
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} }, repoUrl: null },
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
  f.stackSettingsById = {
    [prodStackId]: {
      autoUpdatePolicy: {
        mode: 'override',
        enabled: true,
        rules: [
          {
            id: 'stable-semver',
            name: 'Stable semver',
            enabled: true,
            matcher: { type: 'semver', pattern: '>=5.2.0, <6.0.0' },
            action: 'delayed',
            delay: { minAgeSeconds: 3600, minVersionLag: 2 },
          },
        ],
      },
    },
    [infraStackId]: {
      autoUpdatePolicy: {
        mode: 'override',
        enabled: false,
        rules: [],
      },
    },
  }
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
    serviceId: 'svc-dashboard-background-update',
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

  const recentAutoPolicyJob = {
    id: 'job-auto-policy-api-5-2-3',
    type: 'update',
    scope: 'service',
    stackId: prodStackId,
    serviceId: serviceProdApi.id,
    status: 'success',
    createdBy: 'auto-policy',
    reason: 'auto_policy',
    createdAt: nowIso(-3_600_000),
    startedAt: nowIso(-3_590_000),
    finishedAt: nowIso(-3_480_000),
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary: {
      targets: [{ serviceId: serviceProdApi.id, from: '5.2.1', to: '5.2.3' }],
    },
  } satisfies JobListItem

  const recentWebhookJob = {
    id: 'job-webhook-api-5-2-2',
    type: 'update',
    scope: 'service',
    stackId: prodStackId,
    serviceId: serviceProdApi.id,
    status: 'success',
    createdBy: 'ghcr-webhook',
    reason: 'webhook',
    createdAt: nowIso(-9_000_000),
    startedAt: nowIso(-8_990_000),
    finishedAt: nowIso(-8_880_000),
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary: {
      targets: [{ serviceId: serviceProdApi.id, from: '5.2.0', to: '5.2.2' }],
    },
  } satisfies JobListItem

  const recentStackJob = {
    id: 'job-stack-prod-batch',
    type: 'update',
    scope: 'stack',
    stackId: prodStackId,
    serviceId: null,
    status: 'failed',
    createdBy: 'ivan',
    reason: 'ui',
    createdAt: nowIso(-18_000_000),
    startedAt: nowIso(-17_990_000),
    finishedAt: nowIso(-17_940_000),
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary: {
      targets: [
        { serviceId: serviceProdApi.id, from: '5.1.9', to: '5.2.0' },
        { serviceId: serviceProdWeb.id, from: '5.1', to: '5.2' },
      ],
    },
  } satisfies JobListItem

  f.jobs = [job1, recentAutoPolicyJob, recentWebhookJob, recentStackJob]
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

export function buildLinkIconCatalog(): Fixture {
  const fixture = buildDashboardDemo()

  const applyRepoUrl = (stackId: string, serviceId: string, repoUrl: string | null) => {
    const service = fixture.stackById[stackId]?.services.find((item) => item.id === serviceId)
    if (!service) return
    service.settings = { ...service.settings, repoUrl }
    fixture.serviceSettingsById[serviceId] = {
      ...fixture.serviceSettingsById[serviceId],
      repoUrl,
    }
  }

  applyRepoUrl('stack-prod', 'svc-prod-api', 'https://codeberg.org/acme/api')
  applyRepoUrl('stack-prod', 'svc-prod-web', 'https://gitlab.com/ops/web')
  applyRepoUrl('stack-prod', 'svc-prod-worker', 'https://github.com/acme/worker')
  applyRepoUrl('stack-infra', 'svc-infra-loki', 'https://github.com/grafana/loki')

  return fixture
}

export function buildDigestPinnedImageDisplay(): Fixture {
  const fixture = buildLinkIconCatalog()
  const service = fixture.stackById['stack-prod']?.services.find((item) => item.id === 'svc-prod-api')
  if (service) {
    service.image = {
      ...service.image,
      ref: 'ghcr.io/acme/api@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
      tag: 'latest',
    }
  }
  return fixture
}

export function buildGuideLineLongNames(): Fixture {
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

export function buildServiceDetailComposeFallbacks(): Fixture {
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

export function buildServiceDetailVersionAnomaly(): Fixture {
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

export function buildNoCandidates(): Fixture {
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

export function buildResolvedTagDemo(): Fixture {
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

export function buildServicesInferencePendingCandidateLoading(): Fixture {
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

export function buildVersionTagsPopoverDemo(options?: {
  sameDigest?: boolean
  candidateTag?: string
}): Fixture {
  const f = baseEmpty()
  const lastCheckAt = nowIso(-60_000)

  const stackId = 'stack-version-tags'
  const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`
  const sameDigest = options?.sameDigest ?? false

  const service = {
    id: 'svc-version-tags',
    name: 'axonhub',
    image: {
      ref: 'docker.io/looplj/axonhub',
      tag: '0.8',
      digest: d('a', 'b1'),
    },
    candidate: {
      tag: options?.candidateTag ?? 'v0.8.8-arm64',
      digest: sameDigest ? d('a', 'b1') : d('b', '9f'),
      archMatch: 'match',
      arch: ['linux/arm64'],
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
      services: detail.services.length,
      updates: 1,
      lastCheckAt,
    } satisfies StackListItem,
  ]
  f.stackById = { [stackId]: detail }
  f.serviceSettingsById = { [service.id]: service.settings }

  return f
}
