import {
  ApiError,
  getServiceDigestTagsSnapshot,
  isServiceDigestTagsSnapshotPending,
  type ServiceDigestTagsScanSummary,
} from './api'

export const DIGEST_SNAPSHOT_UPDATED_EVENT = 'dockrev:digest-snapshot-updated'
export const DIGEST_SNAPSHOT_REFRESH_REQUESTED_EVENT = 'dockrev:digest-snapshot-refresh-requested'

export type DigestSnapshotUpdatedDetail = {
  triggerServiceId: string
  imageRepo: string
  digest: string
  tags: string[]
  checkedAt: string | null
  scan: ServiceDigestTagsScanSummary | null
}

export type DigestSnapshotRefreshRequestedDetail = {
  triggerServiceId: string
  imageRepo: string
  digest: string
}

type TrackDigestSnapshotRefreshInput = {
  serviceId: string
  imageRepo: string
  digest: string
}

type TrackedRefresh = {
  key: string
  triggerServiceId: string
  imageRepo: string
  digest: string
  serviceIds: Set<string>
  activeServiceId: string
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

export function publishDigestSnapshotRefreshRequested(detail: DigestSnapshotRefreshRequestedDetail) {
  if (typeof window === 'undefined') return
  window.dispatchEvent(
    new CustomEvent<DigestSnapshotRefreshRequestedDetail>(DIGEST_SNAPSHOT_REFRESH_REQUESTED_EVENT, {
      detail,
    }),
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
    const data = await getServiceDigestTagsSnapshot(tracked.activeServiceId, tracked.digest)
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

    publishDigestSnapshotUpdated({
      triggerServiceId: tracked.triggerServiceId,
      imageRepo: tracked.imageRepo,
      digest: tracked.digest,
      tags: data.tags ?? [],
      checkedAt: data.checkedAt ?? null,
      scan: data.scan ?? null,
    })

    clearTracked(key)
  } catch (e: unknown) {
    const latest = trackedByKey.get(key)
    if (!latest) return

    if (e instanceof ApiError && e.status === 404) {
      latest.serviceIds.delete(latest.activeServiceId)
      const next = latest.serviceIds.values().next().value as string | undefined
      if (next) {
        latest.activeServiceId = next
        latest.errors = 0
        latest.timer = window.setTimeout(() => {
          void pollTracked(key)
        }, 0)
        return
      }
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
  if (!serviceId || !imageRepo || !digest) return

  const key = `${imageRepo}@${digest}`
  const existing = trackedByKey.get(key)
  if (existing) {
    existing.triggerServiceId = serviceId
    existing.serviceIds.add(serviceId)
    existing.activeServiceId = serviceId
    return
  }

  const serviceIds = new Set<string>()
  serviceIds.add(serviceId)
  trackedByKey.set(key, {
    key,
    triggerServiceId: serviceId,
    imageRepo,
    digest,
    serviceIds,
    activeServiceId: serviceId,
    errors: 0,
    startedAtMs: Date.now(),
    timer: window.setTimeout(() => {
      void pollTracked(key)
    }, 0),
  })
}
