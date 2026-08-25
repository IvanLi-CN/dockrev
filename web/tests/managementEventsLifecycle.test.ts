import { describe, expect, test } from 'bun:test'

import {
  createManagementEventTransport,
  MANAGEMENT_TRANSPORT_BACKOFF_MS,
  MANAGEMENT_TRANSPORT_DEADLINE_MS,
  type ManagementTransportSnapshot,
} from '../src/managementEventTransport'

type Listener = (event: unknown) => void

class FakeEventSource {
  readonly listeners = new Map<string, Set<Listener>>()
  closed = false

  addEventListener(type: string, listener: Listener) {
    const listeners = this.listeners.get(type) ?? new Set<Listener>()
    listeners.add(listener)
    this.listeners.set(type, listeners)
  }

  removeEventListener(type: string, listener: Listener) {
    this.listeners.get(type)?.delete(listener)
  }

  close() {
    this.closed = true
  }

  emit(type: string, data?: unknown) {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(data === undefined ? {} : { data })
    }
  }
}

class FakeScheduler {
  private nextId = 1
  private tasks = new Map<number, { callback: () => void; delay: number }>()

  setTimeout = (callback: () => void, delay: number) => {
    const id = this.nextId++
    this.tasks.set(id, { callback, delay })
    return id
  }

  clearTimeout = (handle: unknown) => {
    this.tasks.delete(handle as number)
  }

  pendingDelays() {
    return Array.from(this.tasks.values()).map((task) => task.delay)
  }

  run(delay: number) {
    const entry = Array.from(this.tasks.entries()).find(([, task]) => task.delay === delay)
    if (!entry) throw new Error(`No timer with delay ${delay}`)
    const [id, task] = entry
    this.tasks.delete(id)
    task.callback()
  }

  runAll() {
    while (this.tasks.size > 0) {
      const [id, task] = this.tasks.entries().next().value as [number, { callback: () => void; delay: number }]
      this.tasks.delete(id)
      task.callback()
    }
  }
}

function createHarness() {
  const scheduler = new FakeScheduler()
  const sources: FakeEventSource[] = []
  const snapshots: ManagementTransportSnapshot[] = []
  const syncReasons: string[] = []
  const transport = createManagementEventTransport({
    url: '/api/events',
    createEventSource: () => {
      const source = new FakeEventSource()
      sources.push(source)
      return source
    },
    scheduler,
    now: () => 10_000,
    onSnapshot: (snapshot) => snapshots.push(snapshot),
    onOpen: () => syncReasons.push('open'),
    onManagement: () => syncReasons.push('management'),
    onResyncRequired: () => syncReasons.push('resync'),
    onHeartbeat: () => syncReasons.push('heartbeat'),
    onProtocolInvalid: () => syncReasons.push('protocol_invalid'),
  })
  return { scheduler, sources, snapshots, syncReasons, transport }
}

function validManagementEvent() {
  return JSON.stringify({
    type: 'entities_changed',
    domain: 'stacks',
    entities: [{ entityType: 'stack', id: 'stack-1' }],
    version: 1,
    summary: {},
  })
}

function validHeartbeat() {
  return JSON.stringify({ type: 'management_heartbeat', generation: 'test' })
}

describe('createManagementEventTransport', () => {
  test('closes failed sources and replaces them with bounded backoff', () => {
    const { scheduler, sources, snapshots, transport } = createHarness()
    transport.start()
    expect(sources).toHaveLength(1)

    sources[0].emit('error')
    expect(sources[0].closed).toBe(true)
    expect(snapshots.at(-1)?.connection).toBe('reconnecting')
    expect(snapshots.at(-1)?.reconnectAttempt).toBe(1)
    expect(scheduler.pendingDelays()).toEqual([MANAGEMENT_TRANSPORT_BACKOFF_MS[0]])

    for (const delay of MANAGEMENT_TRANSPORT_BACKOFF_MS) {
      scheduler.run(delay)
      expect(sources.at(-1)?.closed).toBe(false)
      sources.at(-1)?.emit('error')
      expect(sources.at(-1)?.closed).toBe(true)
    }
    expect(sources).toHaveLength(MANAGEMENT_TRANSPORT_BACKOFF_MS.length + 1)
    expect(snapshots.at(-1)?.reconnectAttempt).toBe(6)
    transport.dispose()
  })

  test('replaces a source that never opens or stops receiving activity', () => {
    const { scheduler, sources, snapshots, transport } = createHarness()
    transport.start()
    scheduler.run(MANAGEMENT_TRANSPORT_DEADLINE_MS)
    expect(sources[0].closed).toBe(true)
    expect(snapshots.at(-1)?.lastError).toBe('open_timeout')
    scheduler.run(MANAGEMENT_TRANSPORT_BACKOFF_MS[0])
    sources[1].emit('open')
    scheduler.run(MANAGEMENT_TRANSPORT_DEADLINE_MS)
    expect(sources[1].closed).toBe(true)
    expect(snapshots.at(-1)?.lastError).toBe('heartbeat_timeout')
    transport.dispose()
  })

  test('keeps valid sessions connected for bad payloads and requests recovery once per payload', () => {
    const { scheduler, sources, snapshots, syncReasons, transport } = createHarness()
    transport.start()
    sources[0].emit('open')
    sources[0].emit('management', validManagementEvent())
    sources[0].emit('management_heartbeat', validHeartbeat())
    sources[0].emit('management', '{not-json')
    sources[0].emit('management_heartbeat', JSON.stringify({ type: 'wrong' }))
    expect(snapshots.at(-1)?.connection).toBe('connected')
    expect(snapshots.at(-1)?.lastError).toBe('protocol_invalid')
    expect(sources).toHaveLength(1)
    expect(syncReasons).toEqual(['open', 'management', 'heartbeat', 'protocol_invalid', 'protocol_invalid'])
    expect(scheduler.pendingDelays()).toHaveLength(1)
    transport.dispose()
  })

  test('ignores late callbacks, rebuilds on resume, and cancels timers on dispose', () => {
    const { scheduler, sources, syncReasons, transport } = createHarness()
    transport.start()
    const oldSource = sources[0]
    oldSource.emit('open')
    transport.resume()
    expect(oldSource.closed).toBe(true)
    expect(sources).toHaveLength(2)
    oldSource.emit('open')
    oldSource.emit('management', validManagementEvent())
    expect(syncReasons).toEqual(['open'])
    sources[1].emit('open')
    expect(syncReasons).toEqual(['open', 'open'])
    transport.dispose()
    expect(sources[1].closed).toBe(true)
    expect(scheduler.pendingDelays()).toEqual([])
    sources[1].emit('error')
    expect(sources).toHaveLength(2)
  })
})
