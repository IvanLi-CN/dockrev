import {
  ApiError,
  getServiceDigestTagsSnapshot,
  isServiceDigestTagsSnapshotPending,
  type ServiceDigestTagsScanSummary,
} from './api'
import { isSnapshotFreshEnough } from './digestSnapshotFreshness'

export const DIGEST_SNAPSHOT_UPDATED_EVENT = 'dockrev:digest-snapshot-updated'

export type DigestSnapshotSide = 'current' | 'candidate'

export type DigestSnapshotUpdatedDetail = {
  triggerServiceId: string
  imageRepo: string
  digest: string
  side: DigestSnapshotSide
  tags: string[]
  checkedAt: string | null
  scan: ServiceDigestTagsScanSummary | null
}

type TrackDigestSnapshotRefreshInput = {
  serviceId: string
  imageRepo: string
  digest: string
  side: DigestSnapshotSide
  baselineCheckedAt?: string | null
}

type TrackedRefresh = {
  key: string
  serviceId: string
  imageRepo: string
  digest: string
  side: DigestSnapshotSide
  baselineCheckedAt: string | null
  errors: number
  startedAtMs: number
  timer: number | null
}

const trackedByKey = new Map<string, TrackedRefresh>()

const POLL_FALLBACK_MS = 1200
const MAX_ERRORS = 3
const MAX_TRACK_AGE_MS = 10 * 60 * 1000

function publishDigestSnapshotUpdated(detail: DigestSnapshotUpdatedDetail) {
  if (typeof window === 'undefined') return
  window.dispatchEvent(
    new CustomEvent<DigestSnapshotUpdatedDetail>(DIGEST_SNAPSHOT_UPDATED_EVENT, { detail }),
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

    if (
      !isSnapshotFreshEnough(data.checkedAt ?? null, {
        checkedAt: latest.baselineCheckedAt,
        startedAtMs: latest.startedAtMs,
      })
    ) {
      clearTracked(key)
      return
    }

    publishDigestSnapshotUpdated({
      triggerServiceId: tracked.serviceId,
      imageRepo: tracked.imageRepo,
      digest: tracked.digest,
      side: tracked.side,
      tags: data.tags ?? [],
      checkedAt: data.checkedAt ?? null,
      scan: data.scan ?? null,
    })

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

export function trackDigestSnapshotRefresh(input: TrackDigestSnapshotRefreshInput) {
  if (typeof window === 'undefined') return

  const serviceId = input.serviceId.trim()
  const imageRepo = input.imageRepo.trim().toLowerCase()
  const digest = input.digest.trim().toLowerCase()
  const side = input.side
  if (!serviceId || !imageRepo || !digest) return
  if (side !== 'current' && side !== 'candidate') return

  const key = `${serviceId}:${digest}`
  const existing = trackedByKey.get(key)
  if (existing) {
    existing.imageRepo = imageRepo
    existing.side = side
    existing.baselineCheckedAt = (input.baselineCheckedAt ?? '').trim() || null
    // Manual refresh should always re-arm tracking, even when the same digest key is reused.
    existing.errors = 0
    existing.startedAtMs = Date.now()
    if (existing.timer != null) window.clearTimeout(existing.timer)
    existing.timer = window.setTimeout(() => {
      void pollTracked(key)
    }, 0)
    return
  }

  trackedByKey.set(key, {
    key,
    serviceId,
    imageRepo,
    digest,
    side,
    baselineCheckedAt: (input.baselineCheckedAt ?? '').trim() || null,
    errors: 0,
    startedAtMs: Date.now(),
    timer: window.setTimeout(() => {
      void pollTracked(key)
    }, 0),
  })
}
