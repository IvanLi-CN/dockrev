import {
  getServiceDigestTagsSnapshot,
  isServiceDigestTagsSnapshotPending,
  type ServiceDigestTagsScanSummary,
} from './api'
import { isSnapshotFreshEnough } from './digestSnapshotFreshness'

export const DIGEST_SNAPSHOT_UPDATED_EVENT = 'dockrev:digest-snapshot-updated'
const MANAGEMENT_EVENTS_BATCH_EVENT = 'dockrev:management-events'

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
  imageRepo?: string
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
  startedAtMs: number
}

type ManagementEventBatch = {
  events: Array<{
    domain: string
    summary: Record<string, unknown>
  }>
  resyncRequired: boolean
}

const trackedByKey = new Map<string, TrackedRefresh>()
let managementListenerInstalled = false

function publishDigestSnapshotUpdated(detail: DigestSnapshotUpdatedDetail) {
  if (typeof window === 'undefined') return
  window.dispatchEvent(
    new CustomEvent<DigestSnapshotUpdatedDetail>(DIGEST_SNAPSHOT_UPDATED_EVENT, { detail }),
  )
}

function clearTracked(key: string) {
  trackedByKey.delete(key)
}

async function refreshTracked(key: string) {
  const tracked = trackedByKey.get(key)
  if (!tracked) return

  try {
    const data = await getServiceDigestTagsSnapshot(tracked.serviceId, tracked.digest)
    const latest = trackedByKey.get(key)
    if (!latest || isServiceDigestTagsSnapshotPending(data)) return

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
      triggerServiceId: latest.serviceId,
      imageRepo: latest.imageRepo,
      digest: latest.digest,
      side: latest.side,
      tags: data.tags ?? [],
      checkedAt: data.checkedAt ?? null,
      scan: data.scan ?? null,
    })
    clearTracked(key)
  } catch {
    // A later SSE event or reconnect snapshot will retry this one REST read.
  }
}

function installManagementListener() {
  if (managementListenerInstalled || typeof window === 'undefined') return
  managementListenerInstalled = true
  window.addEventListener(MANAGEMENT_EVENTS_BATCH_EVENT, (raw: Event) => {
    const detail = raw instanceof CustomEvent
      ? (raw.detail as ManagementEventBatch | undefined)
      : undefined
    if (!detail) return
    for (const [key, tracked] of trackedByKey) {
      const relevant = detail.resyncRequired || detail.events.some((event) => {
        if (event.domain !== 'version_inference' || event.summary.phase !== 'finished') return false
        const eventDigest = typeof event.summary.digest === 'string'
          ? event.summary.digest.trim().toLowerCase()
          : ''
        const eventRepo = typeof event.summary.imageRepo === 'string'
          ? event.summary.imageRepo.trim().toLowerCase()
          : ''
        return eventDigest === tracked.digest && (!tracked.imageRepo || eventRepo === tracked.imageRepo)
      })
      if (relevant) void refreshTracked(key)
    }
  })
}

export function trackDigestSnapshotRefresh(input: TrackDigestSnapshotRefreshInput) {
  if (typeof window === 'undefined') return

  const serviceId = input.serviceId.trim()
  const imageRepo = input.imageRepo?.trim().toLowerCase() ?? ''
  const digest = input.digest.trim().toLowerCase()
  const side = input.side
  if (!serviceId || !digest || (side !== 'current' && side !== 'candidate')) return

  installManagementListener()
  const key = `${serviceId}:${digest}`
  trackedByKey.set(key, {
    key,
    serviceId,
    imageRepo,
    digest,
    side,
    baselineCheckedAt: (input.baselineCheckedAt ?? '').trim() || null,
    startedAtMs: Date.now(),
  })
}
