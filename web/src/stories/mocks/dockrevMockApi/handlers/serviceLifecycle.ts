import type { JobListItem, ServiceLifecycleAction, ServiceLifecycleStatusResponse, StackDetail } from '../../../../api'
import type { DockrevApiScenario, Fixture } from '../shared'
import { nowIso, parseJsonBody } from '../shared'

type ServiceLookup = { stack: StackDetail; svc: StackDetail['services'][number] } | null

export function handleServiceLifecycleRoute(input: {
  scenario: DockrevApiScenario
  method: string
  urlPath: string
  init: RequestInit | undefined
  fixture: Fixture
  findService: (serviceId: string) => ServiceLookup
  jobSeqRef: { value: number }
  json: (body: unknown, init?: ResponseInit) => Response
}): Response | null {
  const { scenario, method, urlPath, init, fixture, findService, jobSeqRef, json } = input
  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/lifecycle-status')) {
    const serviceId = decodeURIComponent(urlPath.split('/').slice(3, -1).join('/'))
    if (!findService(serviceId)) return json({ error: { code: 'not_found', message: 'service not found' } }, { status: 404 })
    const stateByScenario: Partial<Record<DockrevApiScenario, ServiceLifecycleStatusResponse>> = {
      'service-detail-lifecycle-running': { state: 'running' },
      'service-detail-lifecycle-partial': { state: 'partial', unavailableReason: 'partial_replicas_running' },
      'service-detail-lifecycle-unknown': { state: 'unknown', unavailableReason: 'lifecycle_status_unavailable' },
      'service-detail-lifecycle-active': {
        state: 'running',
        activeJob: { id: 'job-lifecycle-restart', type: 'service_lifecycle', status: 'running', action: 'restart' },
        unavailableReason: 'service_lifecycle_in_progress',
      },
      'service-detail-rollback-active': {
        state: 'running',
        activeJob: { id: 'job-rollback-service', type: 'rollback', status: 'running', action: null },
        unavailableReason: 'rollback_in_progress',
      },
      'service-action-progress': {
        state: 'running',
        activeJob: { id: 'job-1', type: 'update', status: 'running', action: null },
        unavailableReason: 'update_in_progress',
      },
    }
    return json(stateByScenario[scenario] ?? { state: 'stopped' })
  }
  if (method !== 'POST' || !urlPath.startsWith('/api/services/') || !urlPath.endsWith('/lifecycle')) return null

  const serviceId = decodeURIComponent(urlPath.split('/').slice(3, -1).join('/'))
  const found = findService(serviceId)
  if (!found) return json({ error: { code: 'not_found', message: 'service not found' } }, { status: 404 })
  const action = (parseJsonBody(init?.body) as { action?: ServiceLifecycleAction } | null)?.action
  if (action !== 'start' && action !== 'stop' && action !== 'restart') {
    return json({ error: { code: 'invalid_argument', message: 'invalid lifecycle action' } }, { status: 400 })
  }
  jobSeqRef.value += 1
  const jobId = `job-lifecycle-${action}-${jobSeqRef.value}`
  const createdAt = nowIso(-500)
  const job: JobListItem = {
    id: jobId,
    type: 'service_lifecycle',
    scope: 'service',
    stackId: found.stack.id,
    serviceId,
    status: 'running',
    createdBy: 'ivan',
    reason: 'ui',
    createdAt,
    startedAt: createdAt,
    finishedAt: null,
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary: { action, serviceName: found.svc.name },
  }
  fixture.jobs = [job, ...fixture.jobs]
  fixture.jobById[jobId] = {
    ...job,
    logs: [{ ts: createdAt, level: 'info', msg: `Service lifecycle ${action} started.` }],
    logsLastId: 1,
  }
  return json({ jobId })
}
