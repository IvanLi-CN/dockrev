import { describe, expect, test } from 'bun:test'

import {
  createPwaUpdateActivator,
  createPwaUpdateLifecycleController,
  shouldHidePwaUpdateBubble,
  shouldApplyUpdateOnPathnameNavigation,
  type ServiceWorkerLike,
  type ServiceWorkerRegistrationLike,
} from '../src/pwaUpdateLifecycle'

class FakeWorker implements ServiceWorkerLike {
  state: ServiceWorkerState = 'installing'
  private readonly target = new EventTarget()

  addEventListener(type: 'statechange', listener: () => void) {
    this.target.addEventListener(type, listener)
  }

  removeEventListener(type: 'statechange', listener: () => void) {
    this.target.removeEventListener(type, listener)
  }

  setState(state: ServiceWorkerState) {
    this.state = state
    this.target.dispatchEvent(new Event('statechange'))
  }
}

class FakeRegistration implements ServiceWorkerRegistrationLike {
  active: ServiceWorkerLike | null = new FakeWorker()
  installing: ServiceWorkerLike | null = null
  waiting: ServiceWorkerLike | null = null
  private readonly target = new EventTarget()

  addEventListener(type: 'updatefound', listener: () => void) {
    this.target.addEventListener(type, listener)
  }

  removeEventListener(type: 'updatefound', listener: () => void) {
    this.target.removeEventListener(type, listener)
  }

  startUpdate(worker: FakeWorker) {
    this.installing = worker
    this.target.dispatchEvent(new Event('updatefound'))
  }
}

describe('PwaUpdateLifecycleController', () => {
  test('stays downloading until Workbox promotes the worker to waiting', () => {
    const registration = new FakeRegistration()
    const phases: string[] = []
    const controller = createPwaUpdateLifecycleController({
      hasControllingWorker: () => true,
      onPhaseChange: (phase) => phases.push(phase),
    })
    controller.attach(registration)

    const worker = new FakeWorker()
    registration.startUpdate(worker)
    worker.setState('installed')
    expect(phases).toEqual(['downloading'])

    registration.waiting = worker
    worker.setState('installed')
    expect(phases).toEqual(['downloading', 'ready'])
  })

  test('reports a redundant update as failed and leaves the active worker intact', () => {
    const registration = new FakeRegistration()
    const active = registration.active
    const phases: string[] = []
    const controller = createPwaUpdateLifecycleController({
      hasControllingWorker: () => true,
      onPhaseChange: (phase) => phases.push(phase),
    })
    controller.attach(registration)

    const worker = new FakeWorker()
    registration.startUpdate(worker)
    worker.setState('redundant')

    expect(phases).toEqual(['downloading', 'failed'])
    expect(registration.active).toBe(active)
  })

  test('does not expose the first worker installation as an update', () => {
    const registration = new FakeRegistration()
    registration.active = null
    const phases: string[] = []
    const controller = createPwaUpdateLifecycleController({
      hasControllingWorker: () => false,
      onPhaseChange: (phase) => phases.push(phase),
    })
    controller.attach(registration)

    registration.startUpdate(new FakeWorker())
    expect(phases).toEqual([])
  })
})

describe('PwaUpdateActivator', () => {
  test('does not activate before a waiting worker is ready', async () => {
    let calls = 0
    const activator = createPwaUpdateActivator({
      activate: async () => {
        calls += 1
      },
      hasWaitingWorker: () => false,
      isReady: () => true,
    })

    await activator.request()
    expect(calls).toBe(0)
  })

  test('shares one activation request between manual and navigation triggers', async () => {
    let calls = 0
    let resolveActivation!: () => void
    const activation = new Promise<void>((resolve) => {
      resolveActivation = resolve
    })
    const activator = createPwaUpdateActivator({
      activate: async () => {
        calls += 1
        await activation
      },
      hasWaitingWorker: () => true,
      isReady: () => true,
    })

    const manual = activator.request()
    const navigation = activator.request()
    expect(calls).toBe(1)
    expect(navigation).toBe(manual)

    resolveActivation()
    await manual
  })

  test('allows a retry after activation fails', async () => {
    let calls = 0
    const activator = createPwaUpdateActivator({
      activate: async () => {
        calls += 1
        if (calls === 1) throw new Error('activation failed')
      },
      hasWaitingWorker: () => true,
      isReady: () => true,
    })

    await expect(activator.request()).rejects.toThrow('activation failed')
    await activator.request()
    expect(calls).toBe(2)
  })
})

describe('pathname navigation update gate', () => {
  test('ignores initial render and repeated pathnames', () => {
    expect(shouldApplyUpdateOnPathnameNavigation(null, '/services')).toBe(false)
    expect(shouldApplyUpdateOnPathnameNavigation('/services', '/services')).toBe(false)
  })

  test('allows app navigation and browser history path changes', () => {
    expect(shouldApplyUpdateOnPathnameNavigation('/services', '/queue')).toBe(true)
    expect(shouldApplyUpdateOnPathnameNavigation('/queue/job-1', '/services')).toBe(true)
  })
})

describe('offline update prompt visibility', () => {
  test('hides incomplete offline updates after hover and focus leave', () => {
    expect(shouldHidePwaUpdateBubble({ engaged: false, isOnline: false, phase: 'downloading' })).toBe(true)
    expect(shouldHidePwaUpdateBubble({ engaged: false, isOnline: false, phase: 'failed' })).toBe(true)
  })

  test('keeps an engaged incomplete prompt and every ready prompt visible', () => {
    expect(shouldHidePwaUpdateBubble({ engaged: true, isOnline: false, phase: 'downloading' })).toBe(false)
    expect(shouldHidePwaUpdateBubble({ engaged: false, isOnline: false, phase: 'ready' })).toBe(false)
  })
})
