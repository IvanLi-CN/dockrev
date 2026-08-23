import type { GitHubPackagesRepo, StackDetail, StackListItem } from '../../../api'
import { nowIso, type Fixture, type DockrevApiScenario } from './shared'
import { buildDashboardDemo, baseEmpty, buildDigestPinnedImageDisplay, buildGuideLineLongNames, buildLinkIconCatalog, buildNoCandidates, buildResolvedTagDemo, buildServiceDetailComposeFallbacks, buildServiceDetailVersionAnomaly, buildServicesInferencePendingCandidateLoading, buildVersionTagsPopoverDemo } from './fixturesBase'
import { RUNNING_JOB_ID, buildOverviewJobsCardExactFiveNonTerminal, buildOverviewJobsCardGlobalLabels, buildOverviewJobsCardHeavyInFlight, buildOverviewJobsCardRunningProgressModes, buildOverviewJobsCardTerminalOnly, buildQueueBackupProgress, buildQueueHealthRollback, buildQueueLegacyProgress, buildQueueLongLogs, buildQueueMixed, buildQueueProgressSmoothing, buildQueueUpdateDownloadDeterminate, buildQueueUpdateIndeterminate, buildQueueUpdateLayerProgress, buildVersionInferenceIdleFixture, buildVersionInferenceOverviewFixture, buildVersionInferenceQueueBacklogFixture, buildVersionInferenceResyncRequiredFixture, buildVersionInferenceRunningFixture, buildVersionInferenceStaleAllFailedFixture } from './fixturesQueues'
import { isCleanupMockScenario } from '../cleanupMockData'

export function buildSettingsConfigured(): Fixture {
  const f = buildDashboardDemo()
  f.notifications = {
    email: { enabled: true, smtpUrl: 'smtp://user:pass@mail.example.com:587/?to=a@example.com&from=Dockrev%20<noreply@example.com>' },
    webhook: { enabled: true, url: 'https://hooks.example.com/dockrev' },
    telegram: { enabled: true, botToken: '123:bot-token', botTokenConfigured: true, chatId: '-1009876543210' },
    webPush: { enabled: true, vapidPublicKey: 'BBOG...mock', vapidPrivateKey: null, vapidSubject: 'mailto:ops@example.com' },
  }
  const repos: GitHubPackagesRepo[] = [
    {
      fullName: 'IvanLi-CN/dockrev',
      selected: true,
      webhookState: 'ok',
      webhookJobId: null,
      hookId: 1234567,
      lastSyncAt: nowIso(-60_000),
      lastAuditAt: nowIso(-1_800_000),
      lastOp: 'register',
      lastError: null,
    },
    {
      fullName: 'IvanLi-CN/dockrev-supervisor',
      selected: true,
      webhookState: 'missing',
      webhookJobId: null,
      hookId: null,
      lastSyncAt: null,
      lastAuditAt: nowIso(-2_100_000),
      lastOp: 'audit_all',
      lastError: 'webhook not found on GitHub (mock)',
    },
    {
      fullName: 'IvanLi-CN/example-private',
      selected: true,
      webhookState: 'error',
      webhookJobId: null,
      hookId: null,
      lastSyncAt: null,
      lastAuditAt: nowIso(-2_400_000),
      lastOp: 'register',
      lastError: 'permission denied (mock)',
    },
    {
      fullName: 'IvanLi-CN/webhook-conflict-demo',
      selected: true,
      webhookState: 'conflict',
      webhookJobId: null,
      hookId: null,
      lastSyncAt: null,
      lastAuditAt: nowIso(-1_500_000),
      lastOp: 'audit_all',
      lastError: 'multiple matching webhooks found (mock)',
    },
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

export function buildAggregateDockrevGuard(): Fixture {
  const f = baseEmpty()
  const lastCheckAt = '2026-01-18T06:10:00.000Z'
  const stackId = 'stack-aggregate-guard'
  const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

  const appService = {
    id: 'svc-aggregate-guard-api',
    name: 'api',
    image: { ref: 'ghcr.io/acme/api', tag: '5.2.1', digest: d('a', '01') },
    candidate: { tag: '5.2.3', digest: d('b', '02'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    archived: false,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const dockrevService = {
    id: 'svc-aggregate-guard-dockrev',
    name: 'dockrev',
    image: { ref: 'ghcr.io/ivanli-cn/dockrev', tag: '0.5.0', digest: d('c', '03') },
    candidate: { tag: '0.5.1', digest: d('d', '04'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    archived: false,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const detail = {
    id: stackId,
    name: 'aggregate-demo',
    compose: { type: 'path', composeFiles: ['/srv/aggregate-demo/compose.yml'], envFile: null },
    services: [appService, dockrevService],
  } satisfies StackDetail

  f.stacks = [
    {
      id: stackId,
      name: detail.name,
      status: 'healthy',
      services: detail.services.length,
      updates: 2,
      lastCheckAt,
    } satisfies StackListItem,
  ]
  f.stackById = { [stackId]: detail }
  f.serviceSettingsById = {
    [appService.id]: appService.settings,
    [dockrevService.id]: dockrevService.settings,
  }
  f.serviceBackupTargetsById = Object.fromEntries(
    Object.keys(f.serviceSettingsById).map((serviceId) => [
      serviceId,
      {
        bindPaths: [],
        volumeNames: [],
        storage: {
          baseDir: '/srv/dockrev/backups',
          artifactPattern: '/srv/dockrev/backups/<stackId>/<timestamp>.tar.zst',
          compression: 'zstd',
          keepLast: 1,
          deleteAfterStableSeconds: 3600,
        },
      },
    ]),
  )

  return f
}

export function buildAggregateDockrevOnly(): Fixture {
  const f = baseEmpty()
  const lastCheckAt = '2026-01-18T06:10:00.000Z'
  const stackId = 'stack-aggregate-dockrev-only'
  const d = (fill: string, last2: string) => `sha256:${fill.repeat(62)}${last2}`

  const dockrevService = {
    id: 'svc-aggregate-dockrev-only',
    name: 'dockrev',
    image: { ref: 'ghcr.io/ivanli-cn/dockrev', tag: '0.5.0', digest: d('e', '05') },
    candidate: { tag: '0.5.1', digest: d('f', '06'), archMatch: 'match', arch: ['linux/amd64'] },
    ignore: null,
    archived: false,
    settings: { autoRollback: true, backupTargets: { bindPaths: {}, volumeNames: {} } },
  } satisfies StackDetail['services'][number]

  const detail = {
    id: stackId,
    name: 'dockrev-only',
    compose: { type: 'path', composeFiles: ['/srv/dockrev-only/compose.yml'], envFile: null },
    services: [dockrevService],
  } satisfies StackDetail

  f.stacks = [
    {
      id: stackId,
      name: detail.name,
      status: 'healthy',
      services: detail.services.length,
      updates: 1,
      lastCheckAt,
    } satisfies StackListItem,
  ]
  f.stackById = { [stackId]: detail }
  f.serviceSettingsById = { [dockrevService.id]: dockrevService.settings }

  return f
}

export function buildSettingsNotificationChannelErrors(): Fixture {
  const f = buildSettingsConfigured()
  f.notifications = {
    email: { enabled: false, smtpUrl: null },
    webhook: { enabled: false, url: null },
    telegram: { enabled: false, botToken: null, botTokenConfigured: false, chatId: null },
    webPush: { enabled: false, vapidPublicKey: 'BBOG...mock', vapidPrivateKey: null, vapidSubject: null },
  }
  return f
}

export function buildMultiStackMixed(): Fixture {
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

export function buildArchivedStackDetailNavigation(): Fixture {
  const f = buildMultiStackMixed()
  const stackId = 'stack-lab'
  const stack = f.stacks.find((item) => item.id === stackId)
  if (stack) {
    stack.archived = true
    stack.archivedServices =
      f.stackById[stackId]?.services.filter((service) => Boolean(service.archived))
        .length ?? 0
  }
  if (f.stackById[stackId]) {
    f.stackById[stackId].archived = true
  }
  return f
}

export function buildOverviewDiscoveryReadable(): Fixture {
  const f = buildDashboardDemo()

  f.discoveredProjects = [
    {
      project: 'forward-auth',
      status: 'active',
      stackId: 'stack-prod',
      configFiles: ['/srv/prod/docker-compose.yml', '/srv/prod/docker-compose.ops.yml', '/tmp/dockrev-override-forward-auth.yml'],
      lastSeenAt: nowIso(-45_000),
      lastScanAt: nowIso(-25_000),
      lastError:
        'warning:warning:config_files_conflict_fallback_common: no canonical superset found; all extra files unreadable; using common compose files. Hint: mount the override path into dockrev (same absolute path, read-only), and set DOCKREV_SUPERVISOR_STATE_PATH to the same mounted absolute path in both dockrev and supervisor.',
      archived: false,
    },
    {
      project: 'missing-compose',
      status: 'missing',
      stackId: null,
      configFiles: ['/srv/missing/docker-compose.yml', '/srv/missing/docker-compose.ops.yml'],
      lastSeenAt: nowIso(-6 * 60 * 60 * 1000),
      lastScanAt: nowIso(-60_000),
      lastError: 'bind mount missing for /srv/missing/docker-compose.ops.yml',
      archived: false,
    },
    {
      project: 'invalid-compose',
      status: 'invalid',
      stackId: null,
      configFiles: ['/srv/invalid/docker-compose.yml'],
      lastSeenAt: nowIso(-120_000),
      lastScanAt: nowIso(-95_000),
      lastError: 'yaml parse error: unexpected indent near services.api.environment (line 18)',
      archived: false,
    },
    {
      project: 'detached-lab',
      status: 'invalid',
      stackId: null,
      configFiles: ['/srv/lab/docker-compose.yml', '/srv/lab/compose.ops.yml'],
      lastSeenAt: nowIso(-180_000),
      lastScanAt: nowIso(-90_000),
      lastError:
        'config_files_extra_unreadable: /srv/lab/compose.ops.yml permission denied; compose project kept invalid until the unreadable override is fixed',
      archived: false,
    },
    {
      project: 'core-services',
      status: 'active',
      stackId: 'stack-infra',
      configFiles: ['/srv/infra/docker-compose.yml'],
      lastSeenAt: nowIso(-15_000),
      lastScanAt: nowIso(-15_000),
      lastError: null,
      archived: false,
    },
    ...Array.from({ length: 7 }, (_, index) => ({
      project: `stopped-service-${String(index + 1).padStart(2, '0')}`,
      status: 'stopped' as const,
      stackId: index === 0 ? 'stack-prod' : 'stack-infra',
      configFiles: [`/srv/stopped-${index + 1}/compose.yml`],
      lastSeenAt: nowIso(-(index + 2) * 60_000),
      lastScanAt: nowIso(-(index + 1) * 60_000),
      lastError: null,
      archived: false,
    })),
  ]

  return f
}

export function buildOverviewDiscoveryStaleTempReconcile(): Fixture {
  const f = buildDashboardDemo()
  f.discoveredProjects = [
    {
      project: 'file-storage',
      status: 'active',
      stackId: 'stack-prod',
      configFiles: [
        '/srv/file-storage/docker-compose.yml',
        '/tmp/dockrev-override-file-storage.yml',
      ],
      lastSeenAt: nowIso(-45_000),
      lastScanAt: nowIso(-25_000),
      lastError:
        'warning:config_files_stale_dockrev_temp_override services=[file-storage-notes-webdav,file-storage-syncthing-webdav] temporary Dockrev override was deleted; reconcile is available',
      archived: false,
    },
  ]
  return f
}

export function buildFixture(scenario: Exclude<DockrevApiScenario, 'error'>): Fixture {
  if (scenario === 'empty') return baseEmpty()
  if (isCleanupMockScenario(scenario)) return baseEmpty()
  if (scenario === 'no-candidates') return buildNoCandidates()
  if (
    scenario === 'dashboard-demo' ||
    scenario === 'dashboard-demo-slow-update' ||
    scenario === 'overview-homepage-slow-refresh' ||
    scenario === 'service-detail-lifecycle-running' ||
    scenario === 'service-detail-lifecycle-stopped' ||
    scenario === 'service-detail-lifecycle-partial' ||
    scenario === 'service-detail-lifecycle-unknown' ||
    scenario === 'service-detail-lifecycle-active' ||
    scenario === 'stack-detail-lifecycle-running' ||
    scenario === 'stack-detail-lifecycle-stopped' ||
    scenario === 'stack-detail-lifecycle-partial' ||
    scenario === 'stack-detail-lifecycle-unknown' ||
    scenario === 'stack-detail-lifecycle-active'
  ) return buildDashboardDemo()
  if (scenario === 'dashboard-demo-hydrated-update') {
    const fixture = buildDashboardDemo()
    fixture.jobs = fixture.jobs.map((job) =>
      job.id === 'job-1' ? { ...job, serviceId: 'svc-prod-api', summary: { targetDisplayTag: '5.2.3' } } : job,
    )
    const runningJob = fixture.jobById['job-1']
    if (runningJob) {
      fixture.jobById['job-1'] = {
        ...runningJob,
        serviceId: 'svc-prod-api',
        summary: { targetDisplayTag: '5.2.3' },
      }
    }
    return fixture
  }
  if (scenario === 'service-action-progress') {
    const fixture = buildDashboardDemo()
    const service = fixture.stackById['stack-prod']?.services.find((item) => item.id === 'svc-prod-api')
    fixture.jobs = fixture.jobs.map((job) =>
      job.id === 'job-1' ? { ...job, serviceId: 'svc-prod-api', summary: { targetDisplayTag: '5.2.3' } } : job,
    )
    const runningJob = fixture.jobById['job-1']
    if (runningJob) {
      fixture.jobById['job-1'] = {
        ...runningJob,
        serviceId: 'svc-prod-api',
        summary: { targetDisplayTag: '5.2.3' },
      }
    }
    if (service) service.candidate = null
    const currentDigest = service?.image.digest ?? ''
    fixture.rollbackTargetByServiceId['svc-prod-api'] = {
      available: false,
      currentDigest,
      currentDisplayTag: service?.image.resolvedTag ?? service?.image.tag ?? null,
      targetDigest: null,
      targetDisplayTag: null,
      sourceUpdateJobId: null,
      sourceFinishedAt: null,
      unavailableReason: 'update_in_progress',
      activeJobId: 'job-1',
      activeJobStatus: 'running',
    }
    return fixture
  }
  if (
    scenario === 'service-detail-rollback-available' ||
    scenario === 'service-detail-rollback-confirm-open'
  ) {
    const fixture = buildDashboardDemo()
    const currentDigest = fixture.stackById['stack-prod']?.services.find((service) => service.id === 'svc-prod-api')?.image.digest ?? ''
    fixture.rollbackTargetByServiceId['svc-prod-api'] = {
      available: true,
      currentDigest,
      currentDisplayTag: '5.2.1',
      targetDigest: 'sha256:0000000000000000000000000000000000000000000000000000000000000010',
      targetDisplayTag: '5.2.0',
      sourceUpdateJobId: 'job-auto-policy-api-5-2-3',
      sourceFinishedAt: '2026-07-12T13:45:00.000Z',
      unavailableReason: null,
      activeJobId: null,
      activeJobStatus: null,
    }
    return fixture
  }
  if (scenario === 'service-detail-history-rollback-action') {
    const fixture = buildDashboardDemo()
    const currentDigest = fixture.stackById['stack-prod']?.services.find((service) => service.id === 'svc-prod-api')?.image.digest ?? ''
    fixture.rollbackTargetByServiceId['svc-prod-api'] = {
      available: true,
      currentDigest,
      currentDisplayTag: '5.2.1',
      targetDigest: 'sha256:0000000000000000000000000000000000000000000000000000000000000010',
      targetDisplayTag: '5.2.0',
      sourceUpdateJobId: 'job-auto-policy-api-5-2-3',
      sourceFinishedAt: '2026-07-12T13:45:00.000Z',
      unavailableReason: null,
      activeJobId: null,
      activeJobStatus: null,
    }
    return fixture
  }
  if (scenario === 'service-detail-rollback-unavailable') {
    const fixture = buildDashboardDemo()
    const currentDigest = fixture.stackById['stack-prod']?.services.find((service) => service.id === 'svc-prod-api')?.image.digest ?? ''
    fixture.rollbackTargetByServiceId['svc-prod-api'] = {
      available: false,
      currentDigest,
      currentDisplayTag: '5.2.1',
      targetDigest: null,
      targetDisplayTag: null,
      sourceUpdateJobId: null,
      sourceFinishedAt: null,
      unavailableReason: 'no_matching_update_history',
      activeJobId: null,
      activeJobStatus: null,
    }
    return fixture
  }
  if (scenario === 'service-detail-rollback-stale-after-update') {
    const fixture = buildDashboardDemo()
    fixture.jobs = fixture.jobs.filter((job) => job.id !== 'job-all-api-5-2-4')
    delete fixture.jobById['job-all-api-5-2-4']
    const service = fixture.stackById['stack-prod']?.services.find((item) => item.id === 'svc-prod-api')
    const currentDigest = service?.image.digest ?? ''
    const currentDisplayTag = service?.image.resolvedTag ?? service?.image.tag ?? null
    fixture.rollbackTargetByServiceId['svc-prod-api'] = {
      available: false,
      currentDigest,
      currentDisplayTag,
      targetDigest: null,
      targetDisplayTag: null,
      sourceUpdateJobId: null,
      sourceFinishedAt: null,
      unavailableReason: 'no_matching_update_history',
      activeJobId: null,
      activeJobStatus: null,
    }
    return fixture
  }
  if (scenario === 'service-detail-rollback-active') {
    const fixture = buildDashboardDemo()
    const currentDigest = fixture.stackById['stack-prod']?.services.find((service) => service.id === 'svc-prod-api')?.image.digest ?? ''
    fixture.rollbackTargetByServiceId['svc-prod-api'] = {
      available: false,
      currentDigest,
      currentDisplayTag: '5.2.1',
      targetDigest: 'sha256:0000000000000000000000000000000000000000000000000000000000000010',
      targetDisplayTag: '5.2.0',
      sourceUpdateJobId: 'job-update-rollback-source',
      sourceFinishedAt: '2026-04-05T08:12:00.000Z',
      unavailableReason: 'rollback_in_progress',
      activeJobId: 'job-rollback-service',
      activeJobStatus: 'running',
    }
    const sourceJob = fixture.jobById['job-rollback-api-5-2-2']
    if (sourceJob) {
      const activeJob = { ...sourceJob, id: 'job-rollback-service', status: 'running', finishedAt: null }
      fixture.jobs = [activeJob, ...fixture.jobs]
      fixture.jobById[activeJob.id] = activeJob
    }
    return fixture
  }
  if (scenario === 'link-icon-catalog') return buildLinkIconCatalog()
  if (scenario === 'digest-pinned-image-display') return buildDigestPinnedImageDisplay()
  if (scenario === 'repo-link-editing') {
    const fixture = buildDashboardDemo()
    const serviceId = 'svc-prod-api'
    const found = fixture.stackById['stack-prod']?.services.find((service) => service.id === serviceId)
    if (found) {
      found.settings = { ...found.settings, repoUrl: null }
    }
    fixture.serviceSettingsById[serviceId] = {
      ...fixture.serviceSettingsById[serviceId],
      repoUrl: null,
    }
    fixture.repoLinkInferenceByServiceId[serviceId] = {
      repoUrl: 'https://github.com/acme/api',
      strategy: 'ghcr_exact',
      reason: null,
    }
    return fixture
  }
  if (scenario === 'services-inference-pending-candidate-loading') return buildServicesInferencePendingCandidateLoading()
  if (scenario === 'service-detail-compose-fallbacks') return buildServiceDetailComposeFallbacks()
  if (scenario === 'service-detail-version-anomaly') return buildServiceDetailVersionAnomaly()
  if (scenario === 'service-detail-resource-monitor-disabled') {
    const fixture = buildDashboardDemo()
    fixture.settings.resourceMonitor = {
      ...fixture.settings.resourceMonitor,
      enabled: false,
    }
    return fixture
  }
  if (scenario === 'service-detail-resource-monitor-empty') return buildDashboardDemo()
  if (scenario === 'service-detail-resource-monitor-stream-error') return buildDashboardDemo()
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
  if (scenario === 'version-tags-popover-same-digest') {
    return buildVersionTagsPopoverDemo({ sameDigest: true, candidateTag: 'stable' })
  }
  if (scenario === 'archived-stack-detail-navigation') return buildArchivedStackDetailNavigation()
  if (scenario === 'queue-mixed') return buildQueueMixed()
  if (scenario === 'overview-jobs-card-heavy-inflight') return buildOverviewJobsCardHeavyInFlight()
  if (scenario === 'overview-jobs-card-running-progress-modes') return buildOverviewJobsCardRunningProgressModes()
  if (scenario === 'overview-jobs-card-terminal-only') return buildOverviewJobsCardTerminalOnly()
  if (scenario === 'overview-jobs-card-global-labels') return buildOverviewJobsCardGlobalLabels()
  if (scenario === 'overview-jobs-card-exact-five-non-terminal') return buildOverviewJobsCardExactFiveNonTerminal()
  if (scenario === 'queue-progress-smoothing') return buildQueueProgressSmoothing()
  if (scenario === 'queue-health-rollback') return buildQueueHealthRollback()
  if (scenario === 'queue-legacy-progress') return buildQueueLegacyProgress()
  if (scenario === 'queue-update-layer-progress') return buildQueueUpdateLayerProgress()
  if (scenario === 'queue-update-cancelled') {
    const fixture = buildQueueUpdateLayerProgress()
    const job = fixture.jobById[RUNNING_JOB_ID]
    if (job) {
      const finishedAt = nowIso()
      const cancelled = {
        ...job,
        status: 'cancelled',
        finishedAt,
        stop: { canStop: false, state: 'requested', requestedAt: finishedAt, requestedBy: 'ivan' },
      }
      fixture.jobById[RUNNING_JOB_ID] = cancelled
      fixture.jobs = fixture.jobs.map((item) =>
        item.id === RUNNING_JOB_ID ? { ...item, status: 'cancelled', finishedAt } : item,
      )
    }
    return fixture
  }
  if (scenario === 'queue-update-indeterminate') return buildQueueUpdateIndeterminate()
  if (scenario === 'queue-update-download-determinate') return buildQueueUpdateDownloadDeterminate()
  if (scenario === 'queue-long-logs') return buildQueueLongLogs()
  if (scenario === 'queue-backup-progress') return buildQueueBackupProgress()
  if (
    scenario === 'settings-configured' ||
    scenario === 'settings-configured-load-slow' ||
    scenario === 'settings-configured-resolve-slow'
  ) {
    return buildSettingsConfigured()
  }
  if (scenario === 'settings-notification-channel-errors') return buildSettingsNotificationChannelErrors()
  if (scenario === 'multi-stack-mixed') return buildMultiStackMixed()
  if (scenario === 'overview-discovery-readable') return buildOverviewDiscoveryReadable()
  if (scenario === 'overview-discovery-stale-temp-reconcile') return buildOverviewDiscoveryStaleTempReconcile()
  if (scenario === 'aggregate-dockrev-guard') return buildAggregateDockrevGuard()
  if (scenario === 'aggregate-dockrev-only') return buildAggregateDockrevOnly()
  return buildDashboardDemo()
}
