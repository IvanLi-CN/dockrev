import { imageRepoFromImageRef } from '../../../../imageRepo'
import type { MockRouteContext } from '../context'

export function handleVersionUpdateRoutes(ctx: MockRouteContext): Response | null {
  const { method, urlPath, init, findService, json, nowIso, parseJsonBody, getString } = ctx
  const isObservationsRoute =
    method === 'GET' && /^\/api\/services\/[^/]+\/version-update-observations$/.test(urlPath)
  const isPreviewRoute =
    method === 'POST' && /^\/api\/services\/[^/]+\/version-update\/preview$/.test(urlPath)
  if (!isObservationsRoute && !isPreviewRoute) return null

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
  const releaseTag = getString(parsed && typeof parsed === 'object' ? (parsed as Record<string, unknown>).releaseTag : null) ?? ''
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
