import type { MockRouteContext } from '../context'

export function handleLifecycleEventsRoute(
  ctx: Pick<MockRouteContext, 'method' | 'urlPath' | 'url' | 'state' | 'json'>,
): Response | null {
  const { method, urlPath, url, state, json } = ctx
  if (method !== 'GET' || !urlPath.startsWith('/api/services/')) return null
  const parts = urlPath.split('/').filter(Boolean)
  const serviceId = decodeURIComponent(parts[2] ?? '')
  const snapshot = state.serviceLogsByServiceId[serviceId]?.lifecycle ?? null
  if (!snapshot) return null

  if (urlPath.endsWith('/lifecycle-events')) {
    const sinceMs = Date.parse(url?.searchParams.get('since') ?? '')
    const untilMs = Date.parse(url?.searchParams.get('until') ?? '')
    const events = snapshot.events.filter((event) => {
      const observedAt = Date.parse(event.observedAt)
      return (
        !Number.isFinite(observedAt) ||
        (!Number.isFinite(sinceMs) || observedAt >= sinceMs) &&
          (!Number.isFinite(untilMs) || observedAt <= untilMs)
      )
    })
    const eventIds = new Set(events.map((event) => event.id))
    const availabilityIntervals = snapshot.availabilityIntervals.filter(
      (interval) => eventIds.has(interval.startEventId) || eventIds.has(interval.stopEventId),
    )
    return json({ ...snapshot, serviceId, events, availabilityIntervals })
  }

  if (!urlPath.endsWith('/lifecycle-events/events')) return null
  const afterId = Number.parseInt(url?.searchParams.get('afterId') ?? '0', 10) || 0
  const payload = snapshot.events
    .filter((event) => event.id > afterId)
    .map((event) => `id: ${event.id}\nevent: lifecycle_event\ndata: ${JSON.stringify({ type: 'event', event })}\n\n`)
    .join('')
  return new Response(payload || ': keep-alive\n\n', {
    status: 200,
    headers: { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache', 'x-accel-buffering': 'no' },
  })
}
