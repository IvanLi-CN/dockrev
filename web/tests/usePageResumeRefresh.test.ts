import { describe, expect, test } from 'bun:test'

import { createPageResumeRefreshController } from '../src/usePageResumeRefresh'

class FakeEventTarget {
  private readonly target = new EventTarget()

  addEventListener(type: string, listener: EventListenerOrEventListenerObject) {
    this.target.addEventListener(type, listener)
  }

  dispatchEvent(event: Event) {
    return this.target.dispatchEvent(event)
  }

  removeEventListener(type: string, listener: EventListenerOrEventListenerObject) {
    this.target.removeEventListener(type, listener)
  }
}

class FakeDocument extends FakeEventTarget {
  visibilityState: DocumentVisibilityState = 'visible'
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

async function flushAsync() {
  await Promise.resolve()
  await Promise.resolve()
}

function pageshowEvent(persisted: boolean) {
  const event = new Event('pageshow')
  Object.defineProperty(event, 'persisted', {
    configurable: true,
    value: persisted,
  })
  return event
}

describe('createPageResumeRefreshController', () => {
  test('refreshes when page returns to visible state', async () => {
    const windowTarget = new FakeEventTarget()
    const documentTarget = new FakeDocument()
    documentTarget.visibilityState = 'hidden'
    let callCount = 0
    let now = 1_000

    const controller = createPageResumeRefreshController({
      documentTarget,
      now: () => now,
      refresh: async () => {
        callCount += 1
      },
      windowTarget,
    })
    controller.attach()

    documentTarget.dispatchEvent(new Event('visibilitychange'))
    await flushAsync()
    expect(callCount).toBe(0)

    now = 1_400
    documentTarget.visibilityState = 'visible'
    documentTarget.dispatchEvent(new Event('visibilitychange'))
    await flushAsync()
    expect(callCount).toBe(1)

    controller.dispose()
  })

  test('merges consecutive focus events into one refresh burst', async () => {
    const windowTarget = new FakeEventTarget()
    const documentTarget = new FakeDocument()
    let callCount = 0
    let now = 2_000

    const controller = createPageResumeRefreshController({
      documentTarget,
      now: () => now,
      refresh: async () => {
        callCount += 1
      },
      windowTarget,
    })
    controller.attach()

    windowTarget.dispatchEvent(new Event('focus'))
    await flushAsync()
    expect(callCount).toBe(1)

    now = 2_100
    windowTarget.dispatchEvent(new Event('focus'))
    await flushAsync()
    expect(callCount).toBe(1)

    now = 2_400
    windowTarget.dispatchEvent(pageshowEvent(true))
    await flushAsync()
    expect(callCount).toBe(2)

    controller.dispose()
  })

  test('ignores non-persisted pageshow but refreshes persisted restores', async () => {
    const windowTarget = new FakeEventTarget()
    const documentTarget = new FakeDocument()
    let callCount = 0

    const controller = createPageResumeRefreshController({
      documentTarget,
      refresh: async () => {
        callCount += 1
      },
      windowTarget,
    })
    controller.attach()

    windowTarget.dispatchEvent(pageshowEvent(false))
    await flushAsync()
    expect(callCount).toBe(0)

    windowTarget.dispatchEvent(pageshowEvent(true))
    await flushAsync()
    expect(callCount).toBe(1)

    controller.dispose()
  })

  test('queues one follow-up refresh even when resume events arrive inside the burst window', async () => {
    const windowTarget = new FakeEventTarget()
    const documentTarget = new FakeDocument()
    const refresh1 = deferred<void>()
    const refresh2 = deferred<void>()
    const pending = [refresh1, refresh2]
    let callCount = 0
    let now = 3_000

    const controller = createPageResumeRefreshController({
      documentTarget,
      now: () => now,
      refresh: async () => {
        callCount += 1
        const next = pending.shift()
        if (!next) return
        await next.promise
      },
      windowTarget,
    })
    controller.attach()

    windowTarget.dispatchEvent(new Event('focus'))
    await flushAsync()
    expect(callCount).toBe(1)

    now = 3_100
    windowTarget.dispatchEvent(new Event('focus'))
    now = 3_150
    windowTarget.dispatchEvent(pageshowEvent(true))
    await flushAsync()
    expect(callCount).toBe(1)

    refresh1.resolve()
    await flushAsync()
    expect(callCount).toBe(2)

    refresh2.resolve()
    await flushAsync()
    expect(callCount).toBe(2)

    controller.dispose()
  })

  test('stops reacting after disposal', async () => {
    const windowTarget = new FakeEventTarget()
    const documentTarget = new FakeDocument()
    let callCount = 0

    const controller = createPageResumeRefreshController({
      documentTarget,
      refresh: async () => {
        callCount += 1
      },
      windowTarget,
    })
    controller.attach()
    controller.dispose()

    windowTarget.dispatchEvent(new Event('focus'))
    documentTarget.dispatchEvent(new Event('visibilitychange'))
    await flushAsync()

    expect(callCount).toBe(0)
  })
})
