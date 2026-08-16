import type { MockRouteContext } from '../context'
import { buildCompactMockJob } from '../shared'

export async function handleJobStateRoutes(ctx: MockRouteContext): Promise<Response | null> {
  const {
    isRecord,
    jobSeqRef,
    json,
    makeMockDebug,
    method,
    nowIso,
    state: f,
    url,
    urlPath,
    urlPathWithQuery,
  } = ctx

  if (method === 'POST' && urlPath === '/api/discovery/scan') {
    jobSeqRef.value += 1
    const jobId = `job-discovery-${jobSeqRef.value}`
    const startedAt = nowIso(-500)
    const finishedAt = nowIso(-200)
    const scan = {
      startedAt,
      durationMs: 12,
      summary: {
        projectsSeen: 0,
        stacksCreated: 0,
        stacksUpdated: 0,
        stacksSkipped: 0,
        stacksFailed: 0,
        stacksStopped: 0,
        stacksMarkedMissing: 0,
      },
      actions: [],
    }
    const job = {
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
    f.jobById[jobId] = {
      ...job,
      logs: [{ ts: startedAt, level: 'info', msg: 'discovery scan finished' }],
      logsLastId: 1,
    }
    return json({ jobId })
  }

  if (method === 'GET' && urlPath === '/api/jobs') {
    const debug = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
    debug.jobsListCalls += 1
    debug.jobsListUrls.push(urlPathWithQuery)
    const limit = Math.min(200, Math.max(1, Number(url?.searchParams.get('limit') ?? '100') || 100))
    const types = new Set((url?.searchParams.get('type') ?? '').split(',').filter(Boolean))
    const [status, serviceId, stackId] = ['status', 'serviceId', 'stackId'].map((name) => url?.searchParams.get(name) ?? null)
    const cursor = url?.searchParams.get('cursor') ?? ''
    const start = cursor.startsWith('mock:') ? Number(cursor.slice(5)) || 0 : 0
    const filtered = f.jobs.filter((job) => {
      const summary = isRecord(job.summary) ? job.summary : {}
      const targetServiceIds = Array.isArray(summary.targets)
        ? summary.targets.flatMap((target: unknown) => (isRecord(target) && typeof target.serviceId === 'string' ? [target.serviceId] : []))
        : []
      return !(types.size > 0 && !types.has(job.type)) && !(status && job.status !== status) && !(stackId && job.stackId !== stackId) && !(serviceId && job.serviceId !== serviceId && !targetServiceIds.includes(serviceId))
    }).sort((a, b) => b.createdAt.localeCompare(a.createdAt) || b.id.localeCompare(a.id))
    const jobs = filtered
      .slice(start, start + limit)
      .map((job) => (url?.searchParams.get('view') === 'compact' ? buildCompactMockJob(job, f) : job))
    const nextCursor = start + limit < filtered.length ? `mock:${start + limit}` : null
    return json({ jobs, nextCursor })
  }

  if (method === 'POST' && urlPath.startsWith('/api/jobs/') && urlPath.endsWith('/stop')) {
    const id = decodeURIComponent(urlPath.split('/').slice(3, -1).join('/'))
    const job = f.jobById[id]
    if (!job || !job.stop?.canStop) return json({ error: 'conflict' }, { status: 409 })
    job.stop = { canStop: false, state: 'requested', requestedAt: nowIso(), requestedBy: 'ivan' }
    return json({ jobId: id, state: 'requested' }, { status: 202 })
  }

  if (method === 'GET' && urlPath.startsWith('/api/jobs/')) {
    const id = decodeURIComponent(urlPath.split('/').slice(3).join('/'))
    const job = f.jobById[id]
    if (!job) return json({ error: 'not found' }, { status: 404 })
    if (job.type === 'update' && job.status === 'running' && !job.stop) {
      job.stop = { canStop: true, state: 'available' }
    }
    return json({ job })
  }

  return null
}
