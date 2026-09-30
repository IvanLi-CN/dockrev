import { imageRepoFromImageRef } from '../../../../imageRepo'
import { compareComparableStrictSemverTags } from '../../../../versionDisplay'
import type { JobListItem, ServiceVersionTagObservation } from '../../../../api'
import type { MockRouteContext } from '../context'

const observedVersionDigest = 'sha256:0000000000000000000000000000000000000000000000000000000000000023'

const invalidReleaseTagMessage = 'releaseTag must be a strictly newer comparable SemVer version'

function observationsForService(
  ctx: MockRouteContext,
  serviceId: string,
  configuredTag: string,
): ServiceVersionTagObservation[] {
  if (
    ctx.scenario !== 'service-selected-version-updates' ||
    serviceId !== 'svc-prod-api' ||
    configuredTag !== 'latest'
  ) return []

  return [{ version: '5.2.3', digest: observedVersionDigest, observedAt: ctx.nowIso() }]
}

function previewForRelease(
  found: NonNullable<ReturnType<MockRouteContext['findService']>>,
  releaseTag: string,
  observations: ServiceVersionTagObservation[],
) {
  const currentVersion = found.svc.image.resolvedTag ?? found.svc.image.tag
  const comparison = compareComparableStrictSemverTags(releaseTag, currentVersion)
  if (comparison == null || comparison <= 0) return null

  const imageRepo = imageRepoFromImageRef(found.svc.image.ref) ?? found.svc.image.ref
  const observation = observations.find(
    (item) => compareComparableStrictSemverTags(item.version, releaseTag) === 0,
  )
  const isObserved = observation != null
  return {
    releaseTag,
    classification: isObserved ? 'normal' : 'forced',
    targetDigest: observation?.digest ?? `sha256:${'4'.repeat(64)}`,
    currentDigest: found.svc.image.digest,
    currentVersion,
    imageReference: found.svc.image.ref,
    imageRepo,
    configuredTag: found.svc.image.tag,
  }
}

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
    const observations = observationsForService(ctx, serviceId, found.svc.image.tag)
    const debug = globalThis.__DOCKREV_MOCK_DEBUG__
    if (debug) debug.versionUpdateObservationsCalls += 1
    return json({ imageRepo, configuredTag: found.svc.image.tag, observations })
  }
  const parsed = parseJsonBody(init?.body)
  const record = isRecord(parsed) ? parsed : {}
  if (isSubmitRoute) {
    const releaseTag = getString(record.releaseTag) ?? ''
    const classification = getString(record.classification) ?? ''
    const targetDigest = getString(record.targetDigest) ?? ''
    const forceConfirmed = getBoolean(record.forceConfirmed) ?? false
    const backupMode = getString(record.backupMode) ?? ''
    if (!releaseTag || !['normal', 'forced'].includes(classification) || !['inherit', 'skip', 'force'].includes(backupMode)) {
      return json({ error: 'invalid version update request' }, { status: 400 })
    }
    const preview = previewForRelease(
      found,
      releaseTag,
      observationsForService(ctx, serviceId, found.svc.image.tag),
    )
    if (!preview) {
      return json({
        error: {
          code: 'invalid_argument',
          message: invalidReleaseTagMessage,
          details: {},
        },
      }, { status: 400 })
    }
    const previewMatches =
      classification === preview.classification &&
      targetDigest === preview.targetDigest &&
      getString(record.currentDigest) === preview.currentDigest &&
      getString(record.currentVersion) === preview.currentVersion &&
      getString(record.imageReference) === preview.imageReference &&
      getString(record.imageRepo) === preview.imageRepo &&
      getString(record.configuredTag) === preview.configuredTag
    if (!previewMatches) {
      return json({ error: 'service or version preview changed' }, { status: 409 })
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
      backupMode,
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
  const preview = previewForRelease(
    found,
    releaseTag,
    observationsForService(ctx, serviceId, found.svc.image.tag),
  )
  if (!preview) {
    return json({
      error: {
        code: 'invalid_argument',
        message: invalidReleaseTagMessage,
        details: {},
      },
    }, { status: 400 })
  }
  return json(preview)
}
