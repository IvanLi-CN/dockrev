import type { CleanupApplyRequest, JobListItem } from '../../../api'
import {
  buildCleanupMockScanResponse,
  resolveCleanupMockApply,
  type CleanupMockRuntimeState,
  type CleanupMockScenario,
} from '../../cleanupMockData'
import { parseCleanupScanRequest, partialCleanupResponse, pendingCleanupScanResponse } from '../installHelpers'
import { json, nowIso, parseJsonBody, type Fixture } from '../shared'

export type CleanupRouteContext = {
  cleanupScenario: CleanupMockScenario | null
  /** Management-event publisher owned by the dispatcher's bus. */
  publishCleanupEvent: (summary: Record<string, unknown>) => void
  cleanupRuntime: CleanupMockRuntimeState
  fixture: Fixture
  method: string
  urlPath: string
  init: RequestInit | undefined
}

/**
 * Cleanup routes and their simulated writes. Split out of `install.ts` to keep
 * that dispatcher inside the repo file budget; returns null when the request is
 * not a cleanup route so the dispatcher keeps going.
 */
export async function handleCleanupRoutes(ctx: CleanupRouteContext): Promise<Response | null> {
  const { cleanupScenario, cleanupRuntime, fixture: f, method, urlPath, init } = ctx
  const url = (() => {
    try {
      return new URL(urlPath, 'http://mock.local')
    } catch {
      return null
    }
  })()
  if (cleanupScenario && method === 'POST' && urlPath === '/api/cleanups/scan-runs') {
    const request = parseCleanupScanRequest(init?.body)
    cleanupRuntime.nextScanRunSeq += 1
    const scanId = `mock-cleanup-scan-${cleanupRuntime.nextScanRunSeq}`
    const discoveryScenario =
      cleanupScenario === 'cleanup-console-discovery' ||
      cleanupScenario === 'cleanup-console-discovery-failed'
    const previousSnapshot =
      cleanupScenario === 'cleanup-console-scan-pending' || discoveryScenario
        ? null
        : buildCleanupMockScanResponse(cleanupScenario, request, 1)
    const ready = buildCleanupMockScanResponse(cleanupScenario, request, cleanupScenario === 'cleanup-console-stale' ? 2 : 1)
    const holdPartial =
      (cleanupScenario === 'cleanup-console-scan-slow' && cleanupRuntime.nextScanRunSeq > 1) ||
      cleanupScenario === 'cleanup-console-expiring' ||
      cleanupScenario === 'cleanup-console-confirm-pending' ||
      discoveryScenario
    const events: Array<{ id: number; event: string; data: unknown }> = [
      {
        id: 1,
        event: 'scan_started',
        data: { scanId, phase: 'scan_started', response: previousSnapshot },
      },
    ]
    if (cleanupScenario !== 'cleanup-console-scan-pending') {
      events.push({
        id: 2,
        event: 'scan_partial',
        data: { scanId, phase: 'scan_partial', response: partialCleanupResponse(ready) },
      })
      if (cleanupScenario === 'cleanup-console-discovery-failed') {
        events.push({
          id: 3,
          event: 'scan_failed',
          data: { scanId, phase: 'scan_failed', message: 'cleanup scan failed' },
        })
      } else if (!holdPartial) {
        events.push({
          id: 3,
          event: 'scan_ready',
          data: { scanId, phase: 'scan_ready', response: ready },
        })
      }
    }
    cleanupRuntime.scanRuns.set(scanId, events)
    if (cleanupScenario !== 'cleanup-console-scan-pending' && !holdPartial) {
      ctx.publishCleanupEvent({ phase: 'ready', scanId })
    }
    return json({ scanId, previousSnapshot, retryAfterMs: 450 })
  }
  if (cleanupScenario && method === 'GET' && urlPath.match(/^\/api\/cleanups\/scan-runs\/[^/]+\/events$/)) {
    const scanId = decodeURIComponent(urlPath.split('/')[4] ?? '')
    const afterId = Number.parseInt(url?.searchParams.get('afterId') ?? '0', 10)
    const events = (cleanupRuntime.scanRuns.get(scanId) ?? []).filter((event) => event.id > (Number.isFinite(afterId) ? afterId : 0))
    const body = events
      .map((event) => `id: ${event.id}\nevent: ${event.event}\ndata: ${JSON.stringify(event.data)}\n\n`)
      .join('')
    return new Response(body || ': keep-alive\n\n', {
      status: 200,
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'x-accel-buffering': 'no',
      },
    })
  }
  if (cleanupScenario && method === 'POST' && urlPath === '/api/cleanups/scan') {
    const request = parseCleanupScanRequest(init?.body)
    if (request.reason === 'confirm' && cleanupScenario === 'cleanup-console-confirm-pending') {
      const ready = buildCleanupMockScanResponse(cleanupScenario, request)
      if (!cleanupRuntime.confirmPendingConsumed) {
        cleanupRuntime.confirmPendingConsumed = true
        return json(
          {
            ...ready,
            status: 'pending',
            refreshing: true,
            retryAfterMs: 450,
            scannedAt: null,
            estimatedReclaimableBytes: null,
            hasUnknownSize: false,
            serverDiskUsage: null,
            stackGroups: [],
            unownedGroup: null,
            confirmationFingerprint: null,
          },
        )
      }
      return json(ready)
    }
    if (request.reason === 'confirm' && cleanupScenario === 'cleanup-console-confirm-failed') {
      if (request.refresh && !cleanupRuntime.confirmFailureConsumed) {
        cleanupRuntime.confirmFailureConsumed = true
        return json(
          { error: { code: 'internal_error', message: 'cleanup snapshot refresh failed' } },
          { status: 503 },
        )
      }
      return json(buildCleanupMockScanResponse(cleanupScenario, request))
    }
    if (
      (cleanupScenario === 'cleanup-console-discovery' ||
        cleanupScenario === 'cleanup-console-discovery-failed') &&
      (request.reason === 'page' || request.reason === 'confirm')
    ) {
      const ready = buildCleanupMockScanResponse(cleanupScenario, request)
      return json(pendingCleanupScanResponse(ready), { status: 202 })
    }
    if (cleanupScenario === 'cleanup-console-scan-pending') {
      const ready = buildCleanupMockScanResponse('cleanup-console', request)
      if (request.reason === 'page') {
        return json(pendingCleanupScanResponse(ready), { status: 202 })
      }
      return json({ ...ready, refreshing: true, retryAfterMs: 450 }, { status: 200 })
    }
    if (cleanupScenario === 'cleanup-console-scan-slow') {
      await new Promise<void>((resolve) => {
        globalThis.setTimeout(() => resolve(), 1600)
      })
    }
    return json(buildCleanupMockScanResponse(cleanupScenario, request))
  }
  if (cleanupScenario && method === 'POST' && urlPath === '/api/cleanups/apply') {
    if (cleanupScenario === 'cleanup-console-apply-slow') {
      return new Promise<Response>(() => {})
    }
    const parsed = parseJsonBody(init?.body) as CleanupApplyRequest | null
    if (!parsed) {
      return json({ error: { code: 'invalid_argument', message: 'invalid cleanup apply payload', details: null } }, { status: 400 })
    }
    const result = resolveCleanupMockApply(cleanupScenario, parsed, cleanupRuntime)
    if (!result.ok) return json(result.body, { status: result.status })
    const createdAt = nowIso(-400)
    const jobId = result.jobId
    const job: JobListItem = {
      id: jobId,
      type: 'cleanup_apply',
      scope: parsed.scope,
      stackId: parsed.stackId ?? null,
      serviceId: parsed.serviceId ?? null,
      status: 'running',
      createdBy: 'ivan',
      reason: 'ui',
      createdAt,
      startedAt: nowIso(-200),
      finishedAt: null,
      allowArchMismatch: false,
      backupMode: 'inherit',
      summary: {
        preset: parsed.preset,
        scope: parsed.scope,
        reclaimedBytesEstimated: 0,
        deletedCountsByKind: {},
        skippedInUse: [],
        groupedTargets: [],
      },
    }
    f.jobs = [job, ...f.jobs]
    f.jobById[jobId] = {
      ...job,
      logs: [
        { ts: createdAt, level: 'info', msg: 'cleanup confirm accepted' },
        { ts: nowIso(-100), level: 'info', msg: 'cleanup job queued by UI mock' },
      ],
      logsLastId: 2,
    }
    return json({ jobId })
  }
  return null
}
