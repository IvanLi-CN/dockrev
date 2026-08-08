export type PwaUpdatePhase = 'idle' | 'downloading' | 'ready' | 'failed'

type Listener = () => void

export interface ServiceWorkerLike {
  state: ServiceWorkerState
  addEventListener(type: 'statechange', listener: Listener): void
  removeEventListener(type: 'statechange', listener: Listener): void
}

export interface ServiceWorkerRegistrationLike {
  active: ServiceWorkerLike | null
  installing: ServiceWorkerLike | null
  waiting: ServiceWorkerLike | null
  addEventListener(type: 'updatefound', listener: Listener): void
  removeEventListener(type: 'updatefound', listener: Listener): void
}

export interface PwaUpdateLifecycleControllerOptions {
  hasControllingWorker: () => boolean
  onPhaseChange: (phase: PwaUpdatePhase) => void
}

export interface PwaUpdateActivatorOptions {
  activate: () => Promise<void>
  hasWaitingWorker: () => boolean
  isReady: () => boolean
}

/** Serializes user and navigation activation requests for the same waiting worker. */
export class PwaUpdateActivator {
  private inFlight: Promise<void> | null = null
  private readonly options: PwaUpdateActivatorOptions

  constructor(options: PwaUpdateActivatorOptions) {
    this.options = options
  }

  request() {
    if (!this.options.isReady() || !this.options.hasWaitingWorker()) return Promise.resolve()
    if (this.inFlight) return this.inFlight

    this.inFlight = this.options.activate().finally(() => {
      this.inFlight = null
    })
    return this.inFlight
  }
}

/** Watches an update installation without treating `installed` as cache-complete. */
export class PwaUpdateLifecycleController {
  private currentWorker: ServiceWorkerLike | null = null
  private readonly options: PwaUpdateLifecycleControllerOptions
  private registration: ServiceWorkerRegistrationLike | null = null

  constructor(options: PwaUpdateLifecycleControllerOptions) {
    this.options = options
  }

  attach(registration: ServiceWorkerRegistrationLike) {
    this.dispose()
    this.registration = registration
    registration.addEventListener('updatefound', this.onUpdateFound)

    if (registration.waiting) this.options.onPhaseChange('ready')
  }

  dispose() {
    this.registration?.removeEventListener('updatefound', this.onUpdateFound)
    this.currentWorker?.removeEventListener('statechange', this.onWorkerStateChange)
    this.currentWorker = null
    this.registration = null
  }

  private readonly onUpdateFound = () => {
    const worker = this.registration?.installing
    if (!worker || worker === this.currentWorker) return

    // The first worker establishes the shell; it is not an application update.
    if (!this.registration?.active && !this.options.hasControllingWorker()) return

    this.currentWorker?.removeEventListener('statechange', this.onWorkerStateChange)
    this.currentWorker = worker
    worker.addEventListener('statechange', this.onWorkerStateChange)
    this.options.onPhaseChange('downloading')

    this.reportWorkerState()
  }

  private readonly onWorkerStateChange = () => {
    this.reportWorkerState()
  }

  private reportWorkerState() {
    const worker = this.currentWorker
    const registration = this.registration
    if (!worker || !registration) return

    if (worker.state === 'redundant') {
      this.options.onPhaseChange('failed')
      return
    }

    // Workbox only exposes `waiting` after its install/precache transaction succeeds.
    if (registration.waiting === worker) this.options.onPhaseChange('ready')
  }
}

export function createPwaUpdateLifecycleController(options: PwaUpdateLifecycleControllerOptions) {
  return new PwaUpdateLifecycleController(options)
}

export function createPwaUpdateActivator(options: PwaUpdateActivatorOptions) {
  return new PwaUpdateActivator(options)
}

export function shouldApplyUpdateOnPathnameNavigation(previousPathname: string | null, nextPathname: string) {
  return previousPathname !== null && previousPathname !== nextPathname
}

export function phaseAfterSuccessfulUpdateCheck(
  currentPhase: PwaUpdatePhase,
  hasWaitingWorker: boolean,
): PwaUpdatePhase {
  if (hasWaitingWorker) return 'ready'
  return currentPhase === 'failed' ? 'idle' : currentPhase
}

export function shouldHidePwaUpdateBubble(options: {
  engaged: boolean
  isOnline: boolean
  phase: Exclude<PwaUpdatePhase, 'idle'> | null
}) {
  return !options.isOnline && options.phase !== 'ready' && !options.engaged
}
