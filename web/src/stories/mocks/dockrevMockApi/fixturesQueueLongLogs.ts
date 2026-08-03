import type { JobDetail, JobListItem } from '../../../api'
import type { Fixture } from './shared'
import { nowIso } from './shared'
import { buildDashboardDemo } from './fixturesBase'

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

  const jobLiveLong = makeJob({
    id: 'job-live-long',
    status: 'running',
    createdAt: nowIso(-70_000),
    startedAt: nowIso(-69_000),
    finishedAt: null,
  })

  const digest = `sha256:${'9'.repeat(64)}`
  const longToken = `tok_${'a'.repeat(220)}`
  const longImageRef = `ghcr.io/ivanli-cn/example/super/long/repo/name/that/should/wrap@${digest}`
  const longUrl =
    'https://registry.example.com/v2/ivanli-cn/example/manifests/sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef?ns=docker.io&service=registry&scope=repository%3Aivanli-cn%2Fexample%3Apull&offline_token=true&client_id=dockrev-ui&foo=bar&bar=baz&bar2=quux'

  const baseLongLogs = [
    { ts: nowIso(-12_000), level: 'info', msg: 'check started' },
    {
      ts: nowIso(-11_500),
      level: 'warn',
      msg: `list tags failed for library/postgres: error sending request for url (${longUrl})`,
    },
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
      msg: 'panic: unexpected response (429 Too Many Requests)\nstack:\n  at registry_client.rs:123:9\n  at jobs/check.rs:88:17',
    },
    {
      ts: nowIso(-10_080),
      level: 'event',
      msg: 'event audit: registry snapshot was refreshed',
    },
    ...Array.from({ length: 98 }, (_, i) => ({
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
  ]
  const archivedLongLogs = [...baseLongLogs, { ts: nowIso(-10_000), level: 'info', msg: 'check finished' }]
  const liveLongLogs = [...baseLongLogs, { ts: nowIso(-10_000), level: 'info', msg: 'waiting for next registry event' }]

  f.jobs = [jobShort, jobLiveLong, jobLong]
  f.jobById = {
    [jobShort.id]: {
      ...jobShort,
      logs: [{ ts: nowIso(-12_000), level: 'info', msg: 'check started' }],
      logsLastId: 1,
    } satisfies JobDetail,
    [jobLiveLong.id]: {
      ...jobLiveLong,
      logs: liveLongLogs,
      logsLastId: liveLongLogs.length,
    } satisfies JobDetail,
    [jobLong.id]: {
      ...jobLong,
      logs: archivedLongLogs,
      logsLastId: archivedLongLogs.length,
    } satisfies JobDetail,
  }

  return f
}
