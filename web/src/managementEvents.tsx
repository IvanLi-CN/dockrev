import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { apiBaseUrl } from './api'
import {
  createManagementEventTransport,
  type ManagementEventTransport,
  type ManagementEventPayload,
  type ManagementTransportError,
  type ManagementTransportSnapshot,
} from './managementEventTransport'

export const MANAGEMENT_EVENTS_BATCH_EVENT = 'dockrev:management-events'

export type ManagementEventEntity = {
  entityType: string
  id: string
}

export type ManagementEvent = ManagementEventPayload

export type ManagementEventBatch = {
  events: ManagementEvent[]
  resyncRequired: boolean
}

type ManagementEventsState = {
  connection: 'connecting' | 'connected' | 'reconnecting'
  lastSynchronizedAt: number | null
  reconnectAttempt: number
  lastConnectedAt: number | null
  lastActivityAt: number | null
  lastError: ManagementTransportError
  retryNow: () => void
}

const ManagementEventsContext = createContext<ManagementEventsState | null>(null)

function managementEventsUrl(): string {
  return `${apiBaseUrl().replace(/\/$/, '')}/api/events`
}

function mergeEvent(batch: Map<string, ManagementEvent>, event: ManagementEvent) {
  const entities = event.entities.length > 0 ? event.entities : [{ entityType: 'domain', id: event.domain }]
  for (const entity of entities) {
    batch.set(`${event.domain}:${entity.entityType}:${entity.id}`, {
      ...event,
      entities: [entity],
    })
  }
}

export function ManagementEventsProvider({ children }: { children: ReactNode }) {
  const [transportSnapshot, setTransportSnapshot] = useState<ManagementTransportSnapshot>({
    connection: 'connecting',
    reconnectAttempt: 0,
    lastConnectedAt: null,
    lastActivityAt: null,
    lastError: null,
  })
  const [lastSynchronizedAt, setLastSynchronizedAt] = useState<number | null>(null)
  const pendingRef = useRef(new Map<string, ManagementEvent>())
  const transportRef = useRef<ManagementEventTransport | null>(null)
  const resyncRequiredRef = useRef(false)
  const flushQueuedRef = useRef(false)

  const flush = useCallback(() => {
    flushQueuedRef.current = false
    if (typeof document !== 'undefined' && document.visibilityState !== 'visible') return
    const events = Array.from(pendingRef.current.values())
    const resyncRequired = resyncRequiredRef.current
    pendingRef.current.clear()
    resyncRequiredRef.current = false
    if (events.length === 0 && !resyncRequired) return
    window.dispatchEvent(
      new CustomEvent<ManagementEventBatch>(MANAGEMENT_EVENTS_BATCH_EVENT, {
        detail: { events, resyncRequired },
      }),
    )
    setLastSynchronizedAt(Date.now())
  }, [])

  const requestFlush = useCallback(() => {
    if (flushQueuedRef.current) return
    flushQueuedRef.current = true
    queueMicrotask(flush)
  }, [flush])

  useEffect(() => {
    const transport = createManagementEventTransport({
      url: managementEventsUrl(),
      createEventSource: (url) => new EventSource(url, { withCredentials: true }),
      onSnapshot: setTransportSnapshot,
      onOpen: () => {
      // A snapshot after each connect closes the REST-to-SSE subscription gap.
      resyncRequiredRef.current = true
      requestFlush()
      },
      onManagement: (event) => {
        mergeEvent(pendingRef.current, event)
        requestFlush()
      },
      onResyncRequired: () => {
      resyncRequiredRef.current = true
      requestFlush()
      },
      onHeartbeat: () => {},
      onProtocolInvalid: () => {
        resyncRequiredRef.current = true
        requestFlush()
      },
    })
    transportRef.current = transport
    const onVisibility = () => {
      if (document.visibilityState !== 'visible') return
      transport.resume()
      resyncRequiredRef.current = true
      requestFlush()
    }

    transport.start()
    document.addEventListener('visibilitychange', onVisibility)
    return () => {
      document.removeEventListener('visibilitychange', onVisibility)
      transport.dispose()
      transportRef.current = null
    }
  }, [requestFlush])

  const retryNow = useCallback(() => {
    transportRef.current?.retryNow()
  }, [])

  const value = useMemo(
    () => ({
      ...transportSnapshot,
      lastSynchronizedAt,
      retryNow,
    }),
    [lastSynchronizedAt, retryNow, transportSnapshot],
  )
  return <ManagementEventsContext.Provider value={value}>{children}</ManagementEventsContext.Provider>
}

export function useManagementEvents(): ManagementEventsState {
  const state = useContext(ManagementEventsContext)
  if (!state) throw new Error('ManagementEventsProvider is required')
  return state
}

export function useManagementEventBatch(listener: (batch: ManagementEventBatch) => void) {
  const listenerRef = useRef(listener)
  useEffect(() => {
    listenerRef.current = listener
  }, [listener])
  useEffect(() => {
    const handle = (event: Event) => {
      const batch = event instanceof CustomEvent ? (event.detail as ManagementEventBatch | undefined) : undefined
      if (batch) listenerRef.current(batch)
    }
    window.addEventListener(MANAGEMENT_EVENTS_BATCH_EVENT, handle)
    return () => window.removeEventListener(MANAGEMENT_EVENTS_BATCH_EVENT, handle)
  }, [])
}
