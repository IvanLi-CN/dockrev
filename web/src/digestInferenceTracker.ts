import {
  ApiError,
  getServiceDigestTagsSnapshot,
  isServiceDigestTagsSnapshotPending,
  type ServiceDigestTagsScanSummary,
} from './api'
import { inferResolvedTagsFromSnapshot } from './versionDisplay'

export const DIGEST_INFERENCE_UPDATED_EVENT = 'dockrev:digest-inference-updated'

export type DigestInferenceScope = 'current' | 'candidate'

export type DigestInferenceUpdatedDetail = {
  serviceId: string
  digest: string
  scope: DigestInferenceScope
  resolvedTag: string | null
  resolvedTags: string[] | null
  checkedAt: string | null
  scan: ServiceDigestTagsScanSummary | null
}

type TrackDigestInferenceRefreshInput = {
  serviceId: string
  digest: string
  rawTag: string
  scope: DigestInferenceScope
}

type TrackedRefresh = {
  key: string
  serviceId: string
  digest: string
  rawTag: string
  scope: DigestInferenceScope
  errors: number
  startedAtMs: number
  timer: number | null
}

const trackedByKey = new Map<string, TrackedRefresh>()

const POLL_FALLBACK_MS = 1200
const MAX_ERRORS = 3
const MAX_TRACK_AGE_MS = 10 * 60 * 1000

function scanHasFailures(scan: ServiceDigestTagsScanSummary | null | undefined): boolean {
  if (!scan) return false
  return scan.manifestsTimeout > 0 || scan.manifestsError > 0
}

function scanIsComplete(scan: ServiceDigestTagsScanSummary | null | undefined): boolean {
  if (!scan) return false
  return scan.repoTagsConsidered >= scan.repoTagsTotal
}

function publishDigestInferenceUpdated(detail: DigestInferenceUpdatedDetail) {
  if (typeof window === 'undefined') return
  window.dispatchEvent(
    new CustomEvent<DigestInferenceUpdatedDetail>(DIGEST_INFERENCE_UPDATED_EVENT, { detail }),
  )
}

function clearTracked(key: string) {
  const tracked = trackedByKey.get(key)
  if (!tracked) return
  if (tracked.timer != null) window.clearTimeout(tracked.timer)
  trackedByKey.delete(key)
}

async function pollTracked(key: string) {
  const tracked = trackedByKey.get(key)
  if (!tracked) return

  if (Date.now() - tracked.startedAtMs > MAX_TRACK_AGE_MS) {
    clearTracked(key)
    return
  }

  try {
    const data = await getServiceDigestTagsSnapshot(tracked.serviceId, tracked.digest)
    const latest = trackedByKey.get(key)
    if (!latest) return

    latest.errors = 0
    if (isServiceDigestTagsSnapshotPending(data)) {
      const retryAfterMs = Math.max(
        200,
        Math.min(5000, Number(data.retryAfterMs) || POLL_FALLBACK_MS),
      )
      latest.timer = window.setTimeout(() => {
        void pollTracked(key)
      }, retryAfterMs)
      return
    }

    const inferred = inferResolvedTagsFromSnapshot(data.tags, tracked.rawTag)
    const inferredFirst = inferred[0] ?? null
    const failures = scanHasFailures(data.scan)
    const complete = scanIsComplete(data.scan)
    const shouldApply = Boolean(inferredFirst) || (!failures && complete)

    if (shouldApply) {
      publishDigestInferenceUpdated({
        serviceId: tracked.serviceId,
        digest: tracked.digest,
        scope: tracked.scope,
        resolvedTag: inferredFirst,
        resolvedTags: inferred.length > 1 ? inferred : null,
        checkedAt: data.checkedAt ?? null,
        scan: data.scan ?? null,
      })
    }

    clearTracked(key)
  } catch (e: unknown) {
    const latest = trackedByKey.get(key)
    if (!latest) return

    if (e instanceof ApiError && e.status === 404) {
      clearTracked(key)
      return
    }

    latest.errors += 1
    if (latest.errors >= MAX_ERRORS) {
      clearTracked(key)
      return
    }

    latest.timer = window.setTimeout(() => {
      void pollTracked(key)
    }, POLL_FALLBACK_MS)
  }
}

export function trackDigestInferenceRefresh(input: TrackDigestInferenceRefreshInput) {
  if (typeof window === 'undefined') return
  const serviceId = input.serviceId.trim()
  const digest = input.digest.trim()
  if (!serviceId || !digest) return

  const scope = input.scope
  const key = `${serviceId}:${digest}:${scope}`
  const existing = trackedByKey.get(key)
  if (existing) {
    // Keep the most recent raw tag in case the caller is operating on a newer view of the service.
    existing.rawTag = input.rawTag
    return
  }

  trackedByKey.set(key, {
    key,
    serviceId,
    digest,
    rawTag: input.rawTag,
    scope,
    errors: 0,
    startedAtMs: Date.now(),
    timer: window.setTimeout(() => {
      void pollTracked(key)
    }, 0),
  })
}

