export const MANAGEMENT_TRANSPORT_DEADLINE_MS = 15_000
export const MANAGEMENT_TRANSPORT_BACKOFF_MS = [1_000, 2_000, 5_000, 10_000, 15_000] as const

export type ManagementTransportConnection = 'connecting' | 'connected' | 'reconnecting'
export type ManagementTransportError =
  | 'eventsource_error'
  | 'open_timeout'
  | 'heartbeat_timeout'
  | 'protocol_invalid'
  | null

export type ManagementEventPayload = {
  type: 'entities_changed'
  domain: string
  entities: Array<{ entityType: string; id: string }>
  version: number
  summary: Record<string, unknown>
}

export type ManagementTransportSnapshot = {
  connection: ManagementTransportConnection
  reconnectAttempt: number
  lastConnectedAt: number | null
  lastActivityAt: number | null
  lastError: ManagementTransportError
}

type EventSourceLike = {
  addEventListener: (type: string, listener: (event: unknown) => void) => void
  removeEventListener: (type: string, listener: (event: unknown) => void) => void
  close: () => void
}

function eventLastEventId(event: unknown): string | null {
  if (!event || typeof event !== 'object' || !('lastEventId' in event)) return null
  const value = (event as { lastEventId?: unknown }).lastEventId
  return typeof value === 'string' && value.length > 0 ? value : null
}

function urlWithAfterId(url: string, afterId: string | null): string {
  if (!afterId) return url
  try {
    const parsed = new URL(url, 'http://localhost')
    parsed.searchParams.set('afterId', afterId)
    if (/^[a-z][a-z\d+.-]*:/i.test(url)) return parsed.toString()
    return `${parsed.pathname}${parsed.search}${parsed.hash}`
  } catch {
    const separator = url.includes('?') ? '&' : '?'
    return `${url}${separator}afterId=${encodeURIComponent(afterId)}`
  }
}

type Scheduler = {
  setTimeout: (callback: () => void, delayMs: number) => unknown
  clearTimeout: (handle: unknown) => void
}

export type ManagementTransportOptions = {
  url: string
  createEventSource: (url: string) => EventSourceLike
  onSnapshot: (snapshot: ManagementTransportSnapshot) => void
  onOpen: () => void
  onManagement: (event: ManagementEventPayload) => void
  onResyncRequired: () => void
  onHeartbeat: () => void
  onProtocolInvalid: () => void
  now?: () => number
  scheduler?: Scheduler
}

export type ManagementEventTransport = {
  start: () => void
  resume: () => void
  retryNow: () => void
  dispose: () => void
  getSnapshot: () => ManagementTransportSnapshot
}

const defaultScheduler: Scheduler = {
  setTimeout: (callback, delayMs) => globalThis.setTimeout(callback, delayMs),
  clearTimeout: (handle) => globalThis.clearTimeout(handle as ReturnType<typeof setTimeout>),
}

function eventData(event: unknown): unknown {
  if (!event || typeof event !== 'object' || !('data' in event)) return undefined
  return (event as { data?: unknown }).data
}

function parseData(event: unknown): unknown {
  const data = eventData(event)
  if (typeof data !== 'string') return undefined
  try {
    return JSON.parse(data) as unknown
  } catch {
    return undefined
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value)
}

function isManagementEvent(value: unknown): value is ManagementEventPayload {
  if (!isRecord(value) || value.type !== 'entities_changed') return false
  if (typeof value.domain !== 'string' || typeof value.version !== 'number' || !Number.isFinite(value.version)) return false
  if (!Array.isArray(value.entities) || !isRecord(value.summary)) return false
  return value.entities.every(
    (entity) => isRecord(entity) && typeof entity.entityType === 'string' && typeof entity.id === 'string',
  )
}

function isHeartbeat(value: unknown): boolean {
  return isRecord(value) && value.type === 'management_heartbeat' && typeof value.generation === 'string'
}

function isResyncRequired(value: unknown): boolean {
  return isRecord(value) && value.type === 'resync_required'
}

export function createManagementEventTransport(options: ManagementTransportOptions): ManagementEventTransport {
  const now = options.now ?? Date.now
  const scheduler = options.scheduler ?? defaultScheduler
  let snapshot: ManagementTransportSnapshot = {
    connection: 'connecting',
    reconnectAttempt: 0,
    lastConnectedAt: null,
    lastActivityAt: null,
    lastError: null,
  }
  let source: EventSourceLike | null = null
  let session = 0
  let openTimer: unknown = null
  let activityTimer: unknown = null
  let retryTimer: unknown = null
  let disposed = false
  let lastEventId: string | null = null

  const publish = (next: Partial<ManagementTransportSnapshot>) => {
    snapshot = { ...snapshot, ...next }
    options.onSnapshot(snapshot)
  }

  const clearTimer = (kind: 'open' | 'activity' | 'retry') => {
    const key = kind === 'open' ? 'openTimer' : kind === 'activity' ? 'activityTimer' : 'retryTimer'
    const handle = kind === 'open' ? openTimer : kind === 'activity' ? activityTimer : retryTimer
    if (handle == null) return
    scheduler.clearTimeout(handle)
    if (key === 'openTimer') openTimer = null
    if (key === 'activityTimer') activityTimer = null
    if (key === 'retryTimer') retryTimer = null
  }

  const clearTimers = () => {
    clearTimer('open')
    clearTimer('activity')
    clearTimer('retry')
  }

  const closeSource = () => {
    clearTimer('open')
    clearTimer('activity')
    if (!source) return
    const current = source
    source = null
    session += 1
    current.close()
  }

  const armActivityDeadline = (token: number) => {
    clearTimer('activity')
    activityTimer = scheduler.setTimeout(() => {
      if (disposed || token !== session || !source) return
      fail(token, 'heartbeat_timeout')
    }, MANAGEMENT_TRANSPORT_DEADLINE_MS)
  }

  const markActivity = (token: number) => {
    if (disposed || token !== session || !source) return
    publish({ lastActivityAt: now() })
    armActivityDeadline(token)
  }

  const scheduleRetry = () => {
    clearTimer('retry')
    const delay = MANAGEMENT_TRANSPORT_BACKOFF_MS[Math.min(snapshot.reconnectAttempt - 1, MANAGEMENT_TRANSPORT_BACKOFF_MS.length - 1)]
    retryTimer = scheduler.setTimeout(() => {
      retryTimer = null
      openSource()
    }, delay)
  }

  function fail(token: number, error: Exclude<ManagementTransportError, 'protocol_invalid' | null>) {
    if (disposed || token !== session || !source) return
    closeSource()
    publish({
      connection: 'reconnecting',
      reconnectAttempt: snapshot.reconnectAttempt + 1,
      lastError: error,
    })
    scheduleRetry()
  }

  const protocolInvalid = () => {
    publish({ lastError: 'protocol_invalid' })
    options.onProtocolInvalid()
  }

  const handleOpen = (token: number) => {
    if (disposed || token !== session || !source) return
    clearTimer('open')
    publish({
      connection: 'connected',
      reconnectAttempt: 0,
      lastConnectedAt: now(),
      lastActivityAt: now(),
      lastError: null,
    })
    armActivityDeadline(token)
    options.onOpen()
  }

  const handleManagement = (token: number, event: unknown) => {
    if (disposed || token !== session || !source) return
    lastEventId = eventLastEventId(event) ?? lastEventId
    const payload = parseData(event)
    markActivity(token)
    if (!isManagementEvent(payload)) {
      protocolInvalid()
      return
    }
    options.onManagement(payload)
  }

  const handleHeartbeat = (token: number, event: unknown) => {
    if (disposed || token !== session || !source) return
    lastEventId = eventLastEventId(event) ?? lastEventId
    const payload = parseData(event)
    markActivity(token)
    if (!isHeartbeat(payload)) {
      protocolInvalid()
      return
    }
    options.onHeartbeat()
  }

  const handleResyncRequired = (token: number, event: unknown) => {
    if (disposed || token !== session || !source) return
    lastEventId = eventLastEventId(event) ?? lastEventId
    const payload = parseData(event)
    markActivity(token)
    if (!isResyncRequired(payload)) {
      protocolInvalid()
      return
    }
    options.onResyncRequired()
  }

  function openSource() {
    if (disposed) return
    clearTimer('retry')
    closeSource()
    const token = session
    const next = options.createEventSource(urlWithAfterId(options.url, lastEventId))
    source = next
    publish({ connection: snapshot.reconnectAttempt > 0 ? 'reconnecting' : 'connecting' })
    next.addEventListener('open', () => handleOpen(token))
    next.addEventListener('management', (event) => handleManagement(token, event))
    next.addEventListener('management_heartbeat', (event) => handleHeartbeat(token, event))
    next.addEventListener('resync_required', (event) => handleResyncRequired(token, event))
    next.addEventListener('error', () => fail(token, 'eventsource_error'))
    openTimer = scheduler.setTimeout(() => {
      if (token === session && source) fail(token, 'open_timeout')
    }, MANAGEMENT_TRANSPORT_DEADLINE_MS)
  }

  return {
    start() {
      if (disposed || source) return
      openSource()
    },
    resume() {
      if (disposed) return
      closeSource()
      publish({ connection: 'reconnecting', lastError: null })
      openSource()
    },
    retryNow() {
      if (disposed) return
      clearTimer('retry')
      closeSource()
      publish({ connection: 'reconnecting' })
      openSource()
    },
    dispose() {
      if (disposed) return
      disposed = true
      clearTimers()
      closeSource()
    },
    getSnapshot() {
      return snapshot
    },
  }
}
