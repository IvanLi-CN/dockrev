import type { JobListItem, ServiceLifecycleAction, ServiceLifecycleStatusResponse, StackDetail } from '../../../../api'
import type { DockrevApiScenario, Fixture } from '../shared'
import { makeMockDebug, nowIso, parseJsonBody } from '../shared'

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
      'dashboard-demo-hydrated-update': {
        state: 'running',
        activeJob: { id: 'job-1', type: 'update', status: 'running', action: null },
        unavailableReason: 'update_in_progress',
      },
    }
    return json(stateByScenario[scenario] ?? { state: findService(serviceId)?.svc.lifecycleState ?? 'unknown' })
  }

  const stackLifecycleMatch = urlPath.startsWith('/api/stacks/') && urlPath.endsWith('/lifecycle')
  const stackLifecycleStatusMatch = urlPath.startsWith('/api/stacks/') && urlPath.endsWith('/lifecycle-status')
  if (method === 'GET' && stackLifecycleStatusMatch) {
    const stackId = decodeURIComponent(urlPath.split('/').slice(3, -1).join('/'))
    const stack = fixture.stackById[stackId]
    if (!stack) return json({ error: { code: 'not_found', message: 'stack not found' } }, { status: 404 })
    const stateByScenario: Partial<Record<DockrevApiScenario, ServiceLifecycleStatusResponse>> = {
      'stack-detail-lifecycle-running': { state: 'running' },
      'stack-detail-lifecycle-stopped': { state: 'stopped' },
      'stack-detail-lifecycle-partial': { state: 'partial', unavailableReason: 'stack_services_have_mixed_states' },
      'stack-detail-lifecycle-unknown': { state: 'unknown', unavailableReason: 'lifecycle_status_unavailable' },
      'stack-detail-lifecycle-active': {
        state: 'running',
        activeJob: { id: 'job-stack-lifecycle-restart', type: 'stack_lifecycle', status: 'running', action: 'restart' },
        unavailableReason: 'stack_lifecycle_in_progress',
      },
    }
    if (stateByScenario[scenario]) return json(stateByScenario[scenario])
    const states = stack.services.map((service) => service.lifecycleState ?? 'unknown')
    const state = states.length === 0 || states.includes('unknown')
      ? 'unknown'
      : states.every((value) => value === 'running')
        ? 'running'
        : states.every((value) => value === 'stopped')
          ? 'stopped'
          : 'partial'
    return json({ state, unavailableReason: state === 'partial' ? 'stack_services_have_mixed_states' : undefined })
  }

  const serviceLifecycleMatch = urlPath.startsWith('/api/services/') && urlPath.endsWith('/lifecycle')
  if (method !== 'POST' || (!serviceLifecycleMatch && !stackLifecycleMatch)) return null

  const targetId = decodeURIComponent(urlPath.split('/').slice(3, -1).join('/'))
  const found = serviceLifecycleMatch ? findService(targetId) : null
  const stack = stackLifecycleMatch ? fixture.stackById[targetId] : found?.stack
  if (!stack || (serviceLifecycleMatch && !found)) return json({ error: { code: 'not_found', message: 'lifecycle target not found' } }, { status: 404 })
  const action = (parseJsonBody(init?.body) as { action?: ServiceLifecycleAction } | null)?.action
  if (action !== 'start' && action !== 'stop' && action !== 'restart') {
    return json({ error: { code: 'invalid_argument', message: 'invalid lifecycle action' } }, { status: 400 })
  }
  jobSeqRef.value += 1
  const kind = stackLifecycleMatch ? 'stack' : 'service'
  const jobId = `job-${kind}-lifecycle-${action}-${jobSeqRef.value}`
  const createdAt = nowIso(-500)
  const debug = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
  debug.lastLifecycleRequest = { kind, id: targetId, action }
  const job: JobListItem = {
    id: jobId,
    type: stackLifecycleMatch ? 'stack_lifecycle' : 'service_lifecycle',
    scope: stackLifecycleMatch ? 'stack' : 'service',
    stackId: stack.id,
    serviceId: found?.svc.id ?? null,
    status: 'running',
    createdBy: 'ivan',
    reason: 'ui',
    createdAt,
    startedAt: createdAt,
    finishedAt: null,
    allowArchMismatch: false,
    backupMode: 'inherit',
    summary: stackLifecycleMatch ? { action, stackName: stack.name } : { action, serviceName: found?.svc.name },
  }
  fixture.jobs = [job, ...fixture.jobs]
  fixture.jobById[jobId] = {
    ...job,
    logs: [{ ts: createdAt, level: 'info', msg: `Service lifecycle ${action} started.` }],
    logsLastId: 1,
  }
  return json({ jobId })
}
