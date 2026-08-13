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

export const MANAGEMENT_EVENTS_BATCH_EVENT = 'dockrev:management-events'

export type ManagementEventEntity = {
  entityType: string
  id: string
}

export type ManagementEvent = {
  type: 'entities_changed'
  domain: string
  entities: ManagementEventEntity[]
  version: number
  summary: Record<string, unknown>
}

export type ManagementEventBatch = {
  events: ManagementEvent[]
  resyncRequired: boolean
}

type ManagementEventsState = {
  connection: 'connecting' | 'live' | 'stale'
  lastSynchronizedAt: number | null
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
  const [connection, setConnection] = useState<ManagementEventsState['connection']>('connecting')
  const [lastSynchronizedAt, setLastSynchronizedAt] = useState<number | null>(null)
  const pendingRef = useRef(new Map<string, ManagementEvent>())
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
    const source = new EventSource(managementEventsUrl(), { withCredentials: true })
    const onOpen = () => {
      setConnection('live')
      // A snapshot after each connect closes the REST-to-SSE subscription gap.
      resyncRequiredRef.current = true
      requestFlush()
    }
    const onManagement = (raw: Event) => {
      if (!(raw instanceof MessageEvent) || typeof raw.data !== 'string') return
      try {
        const event = JSON.parse(raw.data) as ManagementEvent
        if (!event || event.type !== 'entities_changed' || !Array.isArray(event.entities)) return
        mergeEvent(pendingRef.current, event)
        requestFlush()
      } catch {
        setConnection('stale')
      }
    }
    const onResync = () => {
      resyncRequiredRef.current = true
      requestFlush()
    }
    const onError = () => setConnection('stale')
    const onVisibility = () => {
      if (document.visibilityState === 'visible') requestFlush()
    }

    source.addEventListener('open', onOpen)
    source.addEventListener('management', onManagement)
    source.addEventListener('resync_required', onResync)
    source.addEventListener('error', onError)
    document.addEventListener('visibilitychange', onVisibility)
    return () => {
      document.removeEventListener('visibilitychange', onVisibility)
      source.close()
    }
  }, [requestFlush])

  const value = useMemo(
    () => ({ connection, lastSynchronizedAt }),
    [connection, lastSynchronizedAt],
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
