import type { ServiceLifecycleSnapshotResponse } from '../../../../api'
import type { MockRouteContext } from '../context'

export function projectLifecycleSnapshot(
  snapshot: ServiceLifecycleSnapshotResponse,
  serviceId: string,
  since: string | null,
  until: string | null,
): ServiceLifecycleSnapshotResponse {
  const sinceMs = Date.parse(since ?? '')
  const untilMs = Date.parse(until ?? '')
  const events = snapshot.events.filter((event) => {
    const observedAt = Date.parse(event.observedAt)
    return Number.isFinite(observedAt) &&
      (!Number.isFinite(sinceMs) || observedAt >= sinceMs) &&
      (!Number.isFinite(untilMs) || observedAt <= untilMs)
  })
  const availabilityIntervals = snapshot.availabilityIntervals.filter((interval) => {
    const startedAt = Date.parse(interval.startedAt)
    const stoppedAt = Date.parse(interval.stoppedAt)
    if (!Number.isFinite(startedAt) || !Number.isFinite(stoppedAt)) return false
    const left = Math.min(startedAt, stoppedAt)
    const right = Math.max(startedAt, stoppedAt)
    return (!Number.isFinite(sinceMs) || right >= sinceMs) && (!Number.isFinite(untilMs) || left <= untilMs)
  })
  const lastEventId = events.at(-1)?.id ?? null
  return {
    ...snapshot,
    serviceId,
    ...(since ? { since } : {}),
    ...(until ? { until } : {}),
    events,
    availabilityIntervals,
    lastEventId,
    nextCursor: lastEventId,
  }
}

export function handleLifecycleEventsRoute(
  ctx: Pick<MockRouteContext, 'method' | 'urlPath' | 'url' | 'state' | 'json' | 'makeMockDebug'>,
): Response | null {
  const { method, urlPath, url, state, json, makeMockDebug } = ctx
  if (method !== 'GET' || !urlPath.startsWith('/api/services/')) return null
  const parts = urlPath.split('/').filter(Boolean)
  const serviceId = decodeURIComponent(parts[2] ?? '')
  const snapshot = state.serviceLogsByServiceId[serviceId]?.lifecycle ?? null
  if (!snapshot) return null

  if (urlPath.endsWith('/lifecycle-events')) {
    const debug = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
    debug.lifecycleSnapshotCalls += 1
    debug.lifecycleSnapshotUrls.push(ctx.url ? `${ctx.url.pathname}${ctx.url.search}` : urlPath)
    return json(
      projectLifecycleSnapshot(
        snapshot,
        serviceId,
        url?.searchParams.get('since') ?? null,
        url?.searchParams.get('until') ?? null,
      ),
    )
  }

  if (!urlPath.endsWith('/lifecycle-events/events')) return null
  const afterId = Number.parseInt(url?.searchParams.get('afterId') ?? '0', 10) || 0
  const streamEvents = state.serviceLogsByServiceId[serviceId]?.lifecycleSseEvents ?? snapshot.events
  const payload = streamEvents
    .filter((event) => event.id > afterId)
    .map((event) => `id: ${event.id}\nevent: lifecycle_event\ndata: ${JSON.stringify({ type: 'event', event })}\n\n`)
    .join('')
  return new Response(payload || ': keep-alive\n\n', {
    status: 200,
    headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache', 'x-accel-buffering': 'no' },
  })
}
