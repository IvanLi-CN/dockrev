import { afterEach, describe, expect, test } from 'bun:test'

import {
  createJobDetailRefreshCoordinator,
  isJobDetailRefreshCancelled,
  JobDetailRefreshCancelledError,
  JobDetailRefreshTimeoutError,
} from '../src/jobDetailRefreshCoordinator'

type TimerCallback = () => void

function createFakeTimers() {
  let now = 0
  let nextId = 1
  const timers = new Map<number, { at: number; callback: TimerCallback }>()
  const setTimeout = ((callback: TimerCallback, delay = 0) => {
    const id = nextId++
    timers.set(id, { at: now + Math.max(0, delay), callback })
    return id
  }) as unknown as typeof globalThis.setTimeout
  const clearTimeout = ((id: number) => {
    timers.delete(id)
  }) as unknown as typeof globalThis.clearTimeout
  const advance = (ms: number) => {
    now += ms
    while (true) {
      const due = Array.from(timers.entries())
        .filter(([, timer]) => timer.at <= now)
        .sort(([, left], [, right]) => left.at - right.at)[0]
      if (!due) return
      timers.delete(due[0])
      due[1].callback()
    }
  }
  return { setTimeout, clearTimeout, advance, pending: () => timers.size }
}

async function flushMicrotasks() {
  await Promise.resolve()
  await Promise.resolve()
  await Promise.resolve()
}

afterEach(() => {
  // Keep the test suite's process-level timers untouched; every coordinator uses injected timers.
})

describe('Job Detail refresh coordinator', () => {
  test('joins automatic initial and resync reads into one request', async () => {
    const timers = createFakeTimers()
    let resolveLoad: ((value: string) => void) | null = null
    let loadCount = 0
    const coordinator = createJobDetailRefreshCoordinator({
      load: () => {
        loadCount += 1
        return new Promise<string>((resolve) => {
          resolveLoad = resolve
        })
      },
      setTimeout: timers.setTimeout,
      clearTimeout: timers.clearTimeout,
    })

    const initial = coordinator.automatic()
    await flushMicrotasks()
    const resync = coordinator.automatic()
    expect(resync).toBe(initial)
    expect(loadCount).toBe(1)

    resolveLoad?.('snapshot')
    await expect(initial).resolves.toBe('snapshot')
    await expect(resync).resolves.toBe('snapshot')
    coordinator.dispose()
  })

  test('bounds automatic reads and retries exactly once after the deadline', async () => {
    const timers = createFakeTimers()
    const signals: AbortSignal[] = []
    let loadCount = 0
    const coordinator = createJobDetailRefreshCoordinator({
      load: (signal) => {
        loadCount += 1
        signals.push(signal)
        return new Promise<string>((_resolve, reject) => {
          signal.addEventListener('abort', () => reject(signal.reason), { once: true })
        })
      },
      setTimeout: timers.setTimeout,
      clearTimeout: timers.clearTimeout,
    })

    const result = coordinator.automatic()
    await flushMicrotasks()
    timers.advance(10_000)
    await flushMicrotasks()
    expect(signals[0]?.aborted).toBe(true)
    expect(loadCount).toBe(1)
    expect(timers.pending()).toBe(1)

    timers.advance(999)
    await flushMicrotasks()
    expect(loadCount).toBe(1)
    timers.advance(1)
    await flushMicrotasks()
    expect(loadCount).toBe(2)

    timers.advance(10_000)
    await expect(result).rejects.toBeInstanceOf(JobDetailRefreshTimeoutError)
    expect(signals[1]?.aborted).toBe(true)
    coordinator.dispose()
  })

  test('manual refresh replaces automatic work and cancels stale sequence state', async () => {
    const timers = createFakeTimers()
    const signals: AbortSignal[] = []
    const resolvers: Array<(value: string) => void> = []
    const coordinator = createJobDetailRefreshCoordinator({
      load: (signal) => {
        signals.push(signal)
        return new Promise<string>((resolve, reject) => {
          resolvers.push(resolve)
          signal.addEventListener('abort', () => reject(signal.reason), { once: true })
        })
      },
      setTimeout: timers.setTimeout,
      clearTimeout: timers.clearTimeout,
    })

    const automatic = coordinator.automatic()
    await flushMicrotasks()
    const manual = coordinator.manual()
    await flushMicrotasks()
    expect(signals[0]?.aborted).toBe(true)
    expect(isJobDetailRefreshCancelled(await automatic.catch((error) => error))).toBe(true)

    resolvers[1]?.('manual snapshot')
    await expect(manual).resolves.toBe('manual snapshot')
    coordinator.dispose()
    expect(timers.pending()).toBe(0)
  })

  test('manual refresh settles a loader that ignores abort', async () => {
    const timers = createFakeTimers()
    const coordinator = createJobDetailRefreshCoordinator({
      load: () => new Promise<string>(() => {}),
      setTimeout: timers.setTimeout,
      clearTimeout: timers.clearTimeout,
    })

    const automatic = coordinator.automatic()
    await flushMicrotasks()
    const manual = coordinator.manual()
    await expect(automatic).rejects.toBeInstanceOf(JobDetailRefreshCancelledError)
    expect(manual).toBeDefined()
    coordinator.dispose()
    await expect(manual).rejects.toBeInstanceOf(JobDetailRefreshCancelledError)
  })

  test('dispose cancels a pending retry wait', async () => {
    const timers = createFakeTimers()
    let rejectLoad: ((error: Error) => void) | null = null
    const coordinator = createJobDetailRefreshCoordinator({
      load: () => new Promise<string>((_resolve, reject) => {
        rejectLoad = reject
      }),
      setTimeout: timers.setTimeout,
      clearTimeout: timers.clearTimeout,
    })
    const automatic = coordinator.automatic()
    await flushMicrotasks()
    rejectLoad?.(new Error('temporary failure'))
    await flushMicrotasks()
    expect(timers.pending()).toBe(1)
    coordinator.dispose()
    await expect(automatic).rejects.toBeInstanceOf(JobDetailRefreshCancelledError)
    expect(timers.pending()).toBe(0)
  })
})
