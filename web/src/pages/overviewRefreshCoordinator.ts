import type { AsyncDataOrigin } from '../asyncData'

export type OverviewDataDomain = 'stacks' | 'jobs' | 'discovery'

export type OverviewRefreshIntent = {
  origin: AsyncDataOrigin
  domains: ReadonlySet<OverviewDataDomain>
  refreshStackList: boolean
  detailStackIds: ReadonlySet<string> | 'all'
}

type Scheduler = {
  setTimeout: (callback: () => void, delayMs: number) => unknown
  clearTimeout: (handle: unknown) => void
}

type ActiveRequest = {
  token: number
  intent: OverviewRefreshIntent
  controller: AbortController
}

export type OverviewRefreshCoordinator = {
  request: (intent: OverviewRefreshIntent) => Promise<void>
  dispose: () => void
}

const defaultScheduler: Scheduler = {
  setTimeout: (callback, delayMs) => globalThis.setTimeout(callback, delayMs),
  clearTimeout: (handle) => globalThis.clearTimeout(handle as ReturnType<typeof setTimeout>),
}

function mergeOrigin(left: AsyncDataOrigin, right: AsyncDataOrigin): AsyncDataOrigin {
  if (left === 'manual' || right === 'manual') return 'manual'
  if (left === 'initial' || right === 'initial') return 'initial'
  if (left === 'recovery' || right === 'recovery') return 'recovery'
  return 'event'
}

export function mergeOverviewRefreshIntents(
  left: OverviewRefreshIntent | null,
  right: OverviewRefreshIntent,
): OverviewRefreshIntent {
  if (!left) return right
  const domains = new Set([...left.domains, ...right.domains])
  let detailStackIds: ReadonlySet<string> | 'all'
  if (left.detailStackIds === 'all' || right.detailStackIds === 'all') {
    detailStackIds = 'all'
  } else {
    detailStackIds = new Set([...left.detailStackIds, ...right.detailStackIds])
  }
  return {
    origin: mergeOrigin(left.origin, right.origin),
    domains,
    refreshStackList: left.refreshStackList || right.refreshStackList,
    detailStackIds,
  }
}

export function createOverviewRefreshCoordinator(
  run: (intent: OverviewRefreshIntent, signal: AbortSignal) => Promise<void>,
  options: { scheduler?: Scheduler; automaticBatchDelayMs?: number } = {},
): OverviewRefreshCoordinator {
  const scheduler = options.scheduler ?? defaultScheduler
  const automaticBatchDelayMs = options.automaticBatchDelayMs ?? 250
  let disposed = false
  let nextToken = 0
  let active: ActiveRequest | null = null
  let pending: OverviewRefreshIntent | null = null
  let timer: unknown = null

  const clearBatchTimer = () => {
    if (timer == null) return
    scheduler.clearTimeout(timer)
    timer = null
  }

  const drain = () => {
    if (disposed || active || !pending) return
    const intent = pending
    pending = null
    const controller = new AbortController()
    const request: ActiveRequest = { token: ++nextToken, intent, controller }
    active = request
    void run(intent, controller.signal).catch(() => undefined).finally(() => {
      if (active?.token !== request.token) return
      active = null
      drain()
    })
  }

  const scheduleDrain = () => {
    if (disposed || active || timer != null) return
    timer = scheduler.setTimeout(() => {
      timer = null
      drain()
    }, automaticBatchDelayMs)
  }

  return {
    request(intent) {
      if (disposed) return Promise.resolve()

      if (intent.origin === 'manual' || intent.origin === 'initial') {
        active?.controller.abort()
        active = null
        pending = null
        clearBatchTimer()

        const controller = new AbortController()
        const request: ActiveRequest = { token: ++nextToken, intent, controller }
        active = request
        return Promise.resolve(run(intent, controller.signal)).finally(() => {
          if (active?.token !== request.token) return
          active = null
          drain()
        })
      }

      pending = mergeOverviewRefreshIntents(pending, intent)
      scheduleDrain()
      return Promise.resolve()
    },
    dispose() {
      disposed = true
      clearBatchTimer()
      pending = null
      active?.controller.abort()
      active = null
    },
  }
}
