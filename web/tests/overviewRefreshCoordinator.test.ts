import { describe, expect, test } from 'bun:test'
import {
  createOverviewRefreshCoordinator,
  mergeOverviewRefreshIntents,
  type OverviewRefreshIntent,
} from '../src/pages/overviewRefreshCoordinator'

function intent(overrides: Partial<OverviewRefreshIntent> = {}): OverviewRefreshIntent {
  return {
    origin: 'event',
    domains: new Set(['stacks']),
    refreshStackList: false,
    detailStackIds: new Set(['stack-1']),
    ...overrides,
  }
}

class Scheduler {
  private nextId = 1
  private timers = new Map<number, () => void>()

  setTimeout = (callback: () => void) => {
    const id = this.nextId++
    this.timers.set(id, callback)
    return id
  }

  clearTimeout = (id: unknown) => {
    this.timers.delete(id as number)
  }

  runAll() {
    const callbacks = Array.from(this.timers.values())
    this.timers.clear()
    callbacks.forEach((callback) => callback())
  }
}

describe('overview refresh coordinator', () => {
  test('merges domains, targets and list refresh intent', () => {
    const merged = mergeOverviewRefreshIntents(
      intent(),
      intent({
        domains: new Set(['jobs']),
        refreshStackList: true,
        detailStackIds: new Set(['stack-2']),
      }),
    )
    expect(Array.from(merged.domains).sort()).toEqual(['jobs', 'stacks'])
    expect(merged.refreshStackList).toBe(true)
    expect(Array.from(merged.detailStackIds === 'all' ? [] : merged.detailStackIds).sort()).toEqual(['stack-1', 'stack-2'])
  })

  test('coalesces automatic events and lets manual refresh preempt them', async () => {
    const scheduler = new Scheduler()
    const calls: Array<{ origin: string; signal: AbortSignal }> = []
    const deferred: Array<() => void> = []
    const coordinator = createOverviewRefreshCoordinator(
      async (next, signal) => {
        calls.push({ origin: next.origin, signal })
        await new Promise<void>((resolve) => deferred.push(resolve))
      },
      { scheduler, automaticBatchDelayMs: 250 },
    )

    void coordinator.request(intent({ detailStackIds: new Set(['stack-1']) }))
    void coordinator.request(intent({ detailStackIds: new Set(['stack-2']), domains: new Set(['jobs']) }))
    scheduler.runAll()
    expect(calls).toHaveLength(1)
    expect(calls[0]?.origin).toBe('event')

    const manualPromise = coordinator.request(intent({ origin: 'manual', refreshStackList: true, detailStackIds: 'all' }))
    expect(calls).toHaveLength(2)
    expect(calls[0]?.signal.aborted).toBe(true)
    expect(calls[1]?.origin).toBe('manual')
    deferred.forEach((resolve) => resolve())
    await manualPromise
    coordinator.dispose()
  })
})
