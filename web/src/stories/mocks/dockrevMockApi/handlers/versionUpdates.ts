import { imageRepoFromImageRef } from '../../../../imageRepo'
import type { JobListItem } from '../../../../api'
import type { MockRouteContext } from '../context'

export function handleVersionUpdateRoutes(ctx: MockRouteContext): Response | null {
  const {
    method,
    urlPath,
    init,
    findService,
    json,
    nowIso,
    parseJsonBody,
    getString,
    getBoolean,
    isRecord,
    makeMockDebug,
    jobSeqRef,
    state,
  } = ctx
  const isObservationsRoute =
    method === 'GET' && /^\/api\/services\/[^/]+\/version-update-observations$/.test(urlPath)
  const isPreviewRoute =
    method === 'POST' && /^\/api\/services\/[^/]+\/version-update\/preview$/.test(urlPath)
  const isSubmitRoute =
    method === 'POST' && /^\/api\/services\/[^/]+\/version-update$/.test(urlPath)
  if (!isObservationsRoute && !isPreviewRoute && !isSubmitRoute) return null

  const parts = urlPath.split('/').filter(Boolean)
  const serviceId = decodeURIComponent(parts[2] ?? '')
  const found = findService(serviceId)
  if (!found) return json({ error: 'not found' }, { status: 404 })
  const imageRepo = imageRepoFromImageRef(found.svc.image.ref) ?? found.svc.image.ref
  if (isObservationsRoute) {
    const observations = serviceId === 'svc-prod-api' && found.svc.image.tag === 'latest'
      ? [{ version: '5.2.3', digest: 'sha256:0000000000000000000000000000000000000000000000000000000000000023', observedAt: nowIso() }]
      : []
    return json({ imageRepo, configuredTag: found.svc.image.tag, observations })
  }
  const parsed = parseJsonBody(init?.body)
  const record = isRecord(parsed) ? parsed : {}
  if (isSubmitRoute) {
    const releaseTag = getString(record.releaseTag) ?? ''
    const classification = getString(record.classification) ?? ''
    const targetDigest = getString(record.targetDigest) ?? ''
    const forceConfirmed = getBoolean(record.forceConfirmed) ?? false
    if (!releaseTag || !targetDigest || !['normal', 'forced'].includes(classification)) {
      return json({ error: 'invalid version update request' }, { status: 400 })
    }
    if (classification === 'forced' && !forceConfirmed) {
      return json({ error: 'force confirmation required' }, { status: 400 })
    }

    const debug = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
    debug.lastUpdateRequest = record
    debug.lastUpdateUrl = urlPath
    debug.lastUpdateMethod = method

    jobSeqRef.value += 1
    const jobId = `job-selected-version-${jobSeqRef.value}`
    const startedAt = nowIso(-500)
    const job: JobListItem = {
      id: jobId,
      type: 'update',
      scope: 'service',
      stackId: found.stack.id,
      serviceId,
      status: 'running',
      createdBy: 'ivan',
      reason: 'selected_version',
      createdAt: startedAt,
      startedAt,
      finishedAt: null,
      allowArchMismatch: false,
      backupMode: getString(record.backupMode) ?? 'inherit',
      summary: {
        targets: [{
          serviceId,
          targetVersion: releaseTag,
          targetTag: found.svc.image.tag,
          targetDigest,
          skipTargetTagPull: true,
          classification,
          imageRepo,
        }],
      },
    }
    state.jobs = [job, ...state.jobs]
    state.jobById[jobId] = {
      ...job,
      logs: [{ ts: startedAt, level: 'info', msg: `Selected version ${releaseTag} queued for deployment.` }],
      logsLastId: 1,
    }
    return json({ jobId })
  }

  const releaseTag = getString(record.releaseTag) ?? ''
  const isObserved = serviceId === 'svc-prod-api' && releaseTag === '5.2.3' && found.svc.image.tag === 'latest'
  const digestFill = isObserved ? '2' : '4'
  return json({
    releaseTag,
    classification: isObserved ? 'normal' : 'forced',
    targetDigest: `sha256:${digestFill.repeat(64)}`,
    currentDigest: found.svc.image.digest,
    currentVersion: found.svc.image.resolvedTag ?? found.svc.image.tag,
    imageReference: found.svc.image.ref,
    imageRepo,
    configuredTag: found.svc.image.tag,
  })
}
