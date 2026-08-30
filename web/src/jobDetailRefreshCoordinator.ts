export type JobDetailRefreshMode = 'automatic' | 'manual'

export type JobDetailRefreshCoordinatorOptions<T> = {
  load: (signal: AbortSignal) => Promise<T>
  timeoutMs?: number
  retryDelayMs?: number
  setTimeout?: typeof globalThis.setTimeout
  clearTimeout?: typeof globalThis.clearTimeout
}

export class JobDetailRefreshCancelledError extends Error {
  constructor() {
    super('job detail refresh cancelled')
    this.name = 'JobDetailRefreshCancelledError'
  }
}

export class JobDetailRefreshTimeoutError extends Error {
  constructor(timeoutMs: number) {
    super(`job detail refresh timed out after ${timeoutMs}ms`)
    this.name = 'JobDetailRefreshTimeoutError'
  }
}

type ActiveRequest<T> = {
  mode: JobDetailRefreshMode
  promise: Promise<T>
  controller: AbortController
}

const DEFAULT_TIMEOUT_MS = 10_000
const DEFAULT_RETRY_DELAY_MS = 1_000

function abortController(controller: AbortController, reason: Error) {
  try {
    controller.abort(reason)
  } catch {
    controller.abort()
  }
}

export function isJobDetailRefreshCancelled(error: unknown): boolean {
  return error instanceof JobDetailRefreshCancelledError ||
    (typeof DOMException !== 'undefined' && error instanceof DOMException && error.name === 'AbortError')
}

export function createJobDetailRefreshCoordinator<T>(
  options: JobDetailRefreshCoordinatorOptions<T>,
) {
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS
  const retryDelayMs = options.retryDelayMs ?? DEFAULT_RETRY_DELAY_MS
  const schedule = options.setTimeout ?? globalThis.setTimeout
  const cancelSchedule = options.clearTimeout ?? globalThis.clearTimeout
  let active: ActiveRequest<T> | null = null
  let retryTimer: ReturnType<typeof globalThis.setTimeout> | null = null
  let retryWaitReject: ((error: unknown) => void) | null = null
  let automaticSequence: Promise<T> | null = null
  let automaticSequenceToken: symbol | null = null
  let disposed = false

  const clearRetryTimer = (reason?: Error) => {
    if (retryTimer != null) {
      cancelSchedule(retryTimer)
      retryTimer = null
    }
    if (retryWaitReject) {
      const reject = retryWaitReject
      retryWaitReject = null
      reject(reason ?? new JobDetailRefreshCancelledError())
    }
  }

  const runAttempt = (mode: JobDetailRefreshMode): Promise<T> => {
    const controller = new AbortController()
    let timeout: ReturnType<typeof globalThis.setTimeout> | null = null
    const promise = (async () => {
      return await new Promise<T>((resolve, reject) => {
        let settled = false
        const finish = (callback: () => void) => {
          if (settled) return
          settled = true
          if (timeout != null) cancelSchedule(timeout)
          callback()
        }
        controller.signal.addEventListener('abort', () => {
          if (controller.signal.reason instanceof JobDetailRefreshTimeoutError) return
          finish(() => reject(new JobDetailRefreshCancelledError()))
        }, { once: true })
        timeout = schedule(() => {
          const timeoutError = new JobDetailRefreshTimeoutError(timeoutMs)
          finish(() => reject(timeoutError))
          abortController(controller, timeoutError)
        }, timeoutMs)
        Promise.resolve()
          .then(() => options.load(controller.signal))
          .then(
            (value) => finish(() => resolve(value)),
            (error: unknown) => finish(() => {
              if (controller.signal.aborted) {
                if (controller.signal.reason instanceof JobDetailRefreshTimeoutError) {
                  reject(controller.signal.reason)
                } else {
                  reject(new JobDetailRefreshCancelledError())
                }
                return
              }
              reject(error)
            }),
          )
      })
    })()
    active = { mode, promise, controller }
    promise.finally(() => {
      if (active?.promise === promise) active = null
    }).catch(() => undefined)
    return promise
  }

  const runAutomatic = (): Promise<T> => {
    if (disposed) return Promise.reject(new JobDetailRefreshCancelledError())
    if (active?.mode === 'manual') return active.promise
    if (automaticSequence) return automaticSequence
    const sequenceToken = Symbol('job-detail-automatic-sequence')
    automaticSequenceToken = sequenceToken
    const sequence = (async () => {
      try {
        return await runAttempt('automatic')
      } catch (error: unknown) {
        if (disposed || isJobDetailRefreshCancelled(error)) throw error
        await new Promise<void>((resolve, reject) => {
          retryWaitReject = reject
          retryTimer = schedule(() => {
            retryTimer = null
            retryWaitReject = null
            resolve()
          }, retryDelayMs)
        })
        if (disposed) throw new JobDetailRefreshCancelledError()
        return await runAttempt('automatic')
      } finally {
        if (automaticSequenceToken === sequenceToken) {
          automaticSequence = null
          automaticSequenceToken = null
        }
      }
    })()
    automaticSequence = sequence
    sequence.catch(() => undefined)
    return sequence
  }

  const runManual = (): Promise<T> => {
    if (disposed) return Promise.reject(new JobDetailRefreshCancelledError())
    clearRetryTimer(new JobDetailRefreshCancelledError())
    if (active) abortController(active.controller, new JobDetailRefreshCancelledError())
    return runAttempt('manual')
  }

  return {
    automatic: runAutomatic,
    manual: runManual,
    dispose() {
      if (disposed) return
      disposed = true
      clearRetryTimer(new JobDetailRefreshCancelledError())
      if (active) abortController(active.controller, new JobDetailRefreshCancelledError())
      active = null
    },
  }
}

export const __jobDetailRefreshTestUtils = {
  DEFAULT_TIMEOUT_MS,
  DEFAULT_RETRY_DELAY_MS,
}
