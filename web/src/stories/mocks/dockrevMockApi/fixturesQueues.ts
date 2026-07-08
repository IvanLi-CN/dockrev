import type { JobDetail, JobListItem } from '../../../api'
import { makeVersionInferenceOverview, nowIso, type Fixture } from './shared'
import { baseEmpty, buildDashboardDemo } from './fixturesBase'

export function buildQueueMixed(): Fixture {
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
      startedAt: input.startedAt !== undefined ? input.startedAt : nowIso(-110_000),
      finishedAt: input.finishedAt !== undefined ? input.finishedAt : nowIso(-10_000),
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
      id: 'job-runtime-stack',
      type: 'runtime_scan',
      scope: 'stack',
      stackId: 'stack-prod',
      serviceId: null,
      status: 'queued',
      createdAt: nowIso(-95_000),
      startedAt: null,
      finishedAt: null,
      summary: { note: 'queued runtime scan for stack-prod' },
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
    makeJob({
      id: 'job-check-stack-failed',
      type: 'check',
      scope: 'stack',
      stackId: 'stack-prod',
      serviceId: null,
      status: 'failed',
      createdAt: nowIso(-82_000),
      startedAt: nowIso(-80_000),
      finishedAt: nowIso(-70_000),
      resultReason: {
        summary: '任务执行失败',
        detail: 'scheduled checks 执行失败，请查看详细日志定位原因。',
        raw: 'scheduled checks (53/74) aborted with exit code 1',
      },
    }),
    makeJob({
      id: 'job-ghcr-success',
      type: 'github_packages_webhook',
      scope: 'all',
      stackId: null,
      serviceId: null,
      status: 'success',
      createdAt: nowIso(-78_000),
      startedAt: nowIso(-77_000),
      finishedAt: nowIso(-74_000),
    }),
    makeJob({
      id: 'job-rollback-service',
      type: 'rollback',
      scope: 'service',
      stackId: 'stack-prod',
      serviceId: 'svc-prod-api',
      status: 'rolled_back',
      createdAt: nowIso(-62_000),
      startedAt: nowIso(-60_000),
      finishedAt: nowIso(-55_000),
      resultReason: {
        summary: '回滚完成',
        detail: '回滚已完成，目标服务已恢复到指定版本。',
      },
    }),
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

export function buildOverviewJobsCardHeavyInFlight(): Fixture {
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

  const jobs: JobListItem[] = []
  for (let i = 0; i < 12; i += 1) {
    const status = i % 2 === 0 ? 'running' : 'queued'
    jobs.push(
      makeJob({
        id: `overview-inflight-${String(i + 1).padStart(2, '0')}`,
        status,
        createdAt: nowIso(-(10_000 + i * 1_000)),
        startedAt: status === 'running' ? nowIso(-(9_000 + i * 1_000)) : null,
        finishedAt: null,
      }),
    )
  }

  jobs.push(
    makeJob({
      id: 'overview-fallback-success',
      status: 'success',
      createdAt: nowIso(-500),
      startedAt: nowIso(-2_000),
      finishedAt: nowIso(-1_000),
    }),
  )
  jobs.push(
    makeJob({
      id: 'overview-fallback-failed',
      status: 'failed',
      createdAt: nowIso(-1_500),
      startedAt: nowIso(-3_000),
      finishedAt: nowIso(-2_500),
    }),
  )

  f.jobs = jobs
  f.jobById = Object.fromEntries(
    jobs.map((j) => [
      j.id,
      {
        ...j,
        logs: [{ ts: nowIso(-900), level: 'info', msg: `job ${j.id} ready` }],
        logsLastId: 1,
      } satisfies JobDetail,
    ]),
  )

  return f
}

export function buildOverviewJobsCardTerminalOnly(): Fixture {
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

  const terminalStatuses = ['success', 'failed', 'rolled_back', 'success', 'failed', 'success', 'rolled_back']
  const jobs = terminalStatuses.map((status, idx) =>
    makeJob({
      id: `overview-terminal-${String(idx + 1).padStart(2, '0')}`,
      status,
      createdAt: nowIso(-(20_000 + idx * 1_000)),
      startedAt: nowIso(-(19_000 + idx * 1_000)),
      finishedAt: nowIso(-(18_000 + idx * 1_000)),
    }),
  )

  f.jobs = jobs
  f.jobById = Object.fromEntries(
    jobs.map((j) => [
      j.id,
      {
        ...j,
        logs: [{ ts: nowIso(-900), level: 'info', msg: `job ${j.id} ready` }],
        logsLastId: 1,
      } satisfies JobDetail,
    ]),
  )

  return f
}

export function buildOverviewJobsCardExactFiveNonTerminal(): Fixture {
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
    makeJob({ id: 'overview-exact-5-nt-1', status: 'running', createdAt: nowIso(-20_000), finishedAt: null }),
    makeJob({ id: 'overview-exact-5-nt-2', status: 'queued', createdAt: nowIso(-21_000), startedAt: null, finishedAt: null }),
    makeJob({ id: 'overview-exact-5-nt-3', status: 'pending', createdAt: nowIso(-22_000), startedAt: null, finishedAt: null }),
    makeJob({ id: 'overview-exact-5-nt-4', status: 'starting', createdAt: nowIso(-23_000), startedAt: null, finishedAt: null }),
    makeJob({ id: 'overview-exact-5-nt-5', status: 'paused', createdAt: nowIso(-24_000), finishedAt: null }),
    makeJob({ id: 'overview-exact-5-terminal-1', status: 'success', createdAt: nowIso(-5_000) }),
    makeJob({ id: 'overview-exact-5-terminal-2', status: 'failed', createdAt: nowIso(-6_000) }),
  ]

  f.jobs = jobs
  f.jobById = Object.fromEntries(
    jobs.map((j) => [
      j.id,
      {
        ...j,
        logs: [{ ts: nowIso(-900), level: 'info', msg: `job ${j.id} ready` }],
        logsLastId: 1,
      } satisfies JobDetail,
    ]),
  )

  return f
}

export function buildOverviewJobsCardRunningProgressModes(): Fixture {
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
      id: 'overview-running-determinate',
      status: 'running',
      createdAt: nowIso(-18_000),
      startedAt: nowIso(-17_000),
      finishedAt: null,
      progress: {
        phase: 'apply',
        message: 'updating services',
        current: 6,
        total: 8,
        percent: 75,
        plannedCurrent: 7,
        plannedTotal: 8,
        plannedPercent: 88,
        currentTarget: 'worker',
        updatedAt: nowIso(-800),
      },
    }),
    makeJob({
      id: 'overview-running-indeterminate',
      status: 'running',
      createdAt: nowIso(-25_000),
      startedAt: nowIso(-24_000),
      finishedAt: null,
      progress: {
        phase: 'prepare',
        message: 'waiting service metadata',
        current: 0,
        total: 0,
        percent: 0,
        plannedCurrent: null,
        plannedTotal: null,
        plannedPercent: null,
        currentTarget: null,
        updatedAt: nowIso(-1_200),
      },
    }),
    makeJob({
      id: 'overview-queued',
      status: 'queued',
      createdAt: nowIso(-22_000),
      startedAt: null,
      finishedAt: null,
      progress: null,
    }),
    makeJob({
      id: 'overview-success',
      status: 'success',
      createdAt: nowIso(-60_000),
      startedAt: nowIso(-58_000),
      finishedAt: nowIso(-40_000),
      progress: null,
    }),
  ]

  f.jobs = jobs
  f.jobById = Object.fromEntries(
    jobs.map((j) => [
      j.id,
      {
        ...j,
        logs: [{ ts: nowIso(-900), level: 'info', msg: `job ${j.id} ready` }],
        logsLastId: 1,
      } satisfies JobDetail,
    ]),
  )

  return f
}

export function buildQueueProgressSmoothing(): Fixture {
  const f = buildQueueMixed()
  const runningJob = f.jobs.find((job) => job.id === 'job-running')
  if (!runningJob) return f
  const nextProgress = {
    phase: 'pulling',
    message: 'updating images',
    current: 40,
    total: 100,
    percent: 40,
    plannedCurrent: 68,
    plannedTotal: 100,
    plannedPercent: 68,
    currentTarget: 'worker',
    updatedAt: nowIso(-600),
  }
  runningJob.progress = nextProgress
  const runningDetail = f.jobById['job-running']
  if (runningDetail) runningDetail.progress = { ...nextProgress }
  return f
}

export function buildQueueUpdateLayerProgress(): Fixture {
  const f = buildQueueMixed()
  const runningJob = f.jobs.find((job) => job.id === 'job-running')
  if (!runningJob) return f

  const nextProgress = {
    phase: 'apply',
    message: 'pulling image for worker · downloaded 4.2MB · layers 2/6 · ad6b1fa7e521 Downloading',
    current: 2,
    total: 5,
    percent: 40,
    plannedCurrent: 2,
    plannedTotal: 5,
    plannedPercent: 40,
    currentTarget: 'worker',
    download: {
      currentBytes: 4_397_728,
      totalBytes: null,
      completedLayers: 2,
      totalLayers: 6,
      activeLayers: ['ad6b1fa7e521 Downloading'],
      status: 'layers 2/6',
    },
    updatedAt: nowIso(-600),
  }

  runningJob.type = 'update'
  runningJob.scope = 'stack'
  runningJob.progress = nextProgress

  const runningDetail = f.jobById['job-running']
  if (runningDetail) {
    f.jobById['job-running'] = {
      ...runningDetail,
      type: 'update',
      scope: 'stack',
      progress: { ...nextProgress },
    }
  }

  return f
}

export function buildQueueUpdateIndeterminate(): Fixture {
  const f = buildQueueMixed()
  const runningJob = f.jobs.find((job) => job.id === 'job-running')
  if (!runningJob) return f

  const nextProgress = {
    phase: 'apply',
    message: 'applying updates for stack stack-prod',
    current: 2,
    total: 5,
    percent: 40,
    plannedCurrent: 4,
    plannedTotal: 5,
    plannedPercent: null,
    currentTarget: 'worker',
    download: {
      currentBytes: 4_397_728,
      totalBytes: null,
      completedLayers: 0,
      totalLayers: 6,
      activeLayers: ['ad6b1fa7e521 Downloading'],
      status: 'layers 0/6',
    },
    updatedAt: nowIso(-600),
  }

  runningJob.type = 'update'
  runningJob.scope = 'stack'
  runningJob.progress = nextProgress

  const runningDetail = f.jobById['job-running']
  if (runningDetail) {
    f.jobById['job-running'] = {
      ...runningDetail,
      type: 'update',
      scope: 'stack',
      progress: { ...nextProgress },
    }
  }

  return f
}

export function buildQueueUpdateDownloadDeterminate(): Fixture {
  const f = buildQueueMixed()
  const runningJob = f.jobs.find((job) => job.id === 'job-running')
  if (!runningJob) return f

  const nextProgress = {
    phase: 'apply',
    message: 'pulling image for api (53%)',
    current: 2,
    total: 5,
    percent: 40,
    plannedCurrent: 2,
    plannedTotal: 5,
    plannedPercent: 40,
    currentTarget: 'api',
    download: {
      currentBytes: 3_298_919,
      totalBytes: 6_175_785,
      completedLayers: 1,
      totalLayers: 3,
      activeLayers: ['d2cad1f9f7c9 Downloading'],
      status: 'layers 1/3',
    },
    updatedAt: nowIso(-600),
  }

  runningJob.type = 'update'
  runningJob.scope = 'stack'
  runningJob.progress = nextProgress

  const runningDetail = f.jobById['job-running']
  if (runningDetail) {
    f.jobById['job-running'] = {
      ...runningDetail,
      type: 'update',
      scope: 'stack',
      progress: { ...nextProgress },
    }
  }

  return f
}

export function buildQueueLongLogs(): Fixture {
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

export function buildVersionInferenceOverviewFixture(): Fixture {
  return buildQueueMixed()
}

export function buildVersionInferenceResyncRequiredFixture(): Fixture {
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

export function buildVersionInferenceIdleFixture(): Fixture {
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

export function buildVersionInferenceRunningFixture(): Fixture {
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

export function buildVersionInferenceQueueBacklogFixture(): Fixture {
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

export function buildVersionInferenceStaleAllFailedFixture(): Fixture {
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

export function buildQueueLegacyProgress(): Fixture {
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

export function buildQueueHealthRollback(): Fixture {
  const f = buildDashboardDemo()

  const job: JobListItem = {
    id: 'job-health-rollback',
    type: 'update',
    scope: 'service',
    stackId: 'stack-prod',
    serviceId: 'svc-prod-api',
    status: 'rolled_back',
    createdBy: 'ivan',
    reason: 'ui',
    createdAt: nowIso(-95_000),
    startedAt: nowIso(-80_000),
    finishedAt: nowIso(-62_000),
    allowArchMismatch: false,
    backupMode: 'skip',
    summary: {
      mode: 'apply',
      progress: {
        phase: 'done',
        message: 'update rolled back after healthcheck failure',
        current: 1,
        total: 1,
        percent: 100,
        plannedCurrent: 1,
        plannedTotal: 1,
        plannedPercent: 100,
        currentTarget: null,
        updatedAt: nowIso(-62_000),
      },
      stacks: [
        {
          stackId: 'stack-prod',
          backup: { status: 'skipped', reason: 'disabled' },
          update: {
            changedServices: 1,
            oldDigests: { 'svc-prod-api': 'sha256:old' },
            newDigests: { 'svc-prod-api': 'sha256:new' },
            finalDigests: { 'svc-prod-api': 'sha256:old' },
            failureStep: 'healthcheck',
            rollback: {
              trigger: 'healthcheck',
              toDigests: { 'svc-prod-api': 'sha256:old' },
            },
            targetTagsPulled: [],
            pullTagsPulled: [],
            pullTagWarnings: [],
            skippedVersionAnomaly: [],
          },
        },
      ],
    },
    progress: {
      phase: 'done',
      message: 'update rolled back after healthcheck failure',
      current: 1,
      total: 1,
      percent: 100,
      plannedCurrent: 1,
      plannedTotal: 1,
      plannedPercent: 100,
      currentTarget: 'api',
      updatedAt: nowIso(-62_000),
    },
    resultReason: {
      summary: '健康检查失败，已回滚',
      detail: '健康检查未通过，已停止本次变更并恢复到回滚前状态。',
    },
  }

  f.jobs = [job]
  f.jobById = {
    [job.id]: {
      ...job,
      logs: [
        { ts: nowIso(-78_000), level: 'info', msg: 'starting service api' },
        { ts: nowIso(-74_000), level: 'info', msg: 'waiting for healthcheck on api' },
        { ts: nowIso(-71_000), level: 'warn', msg: 'healthcheck failed for api; rolling back' },
        { ts: nowIso(-67_000), level: 'warn', msg: 'service api rolled back after healthcheck failure' },
      ],
      logsLastId: 4,
    } satisfies JobDetail,
  }

  return f
}
