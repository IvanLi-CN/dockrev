import { useCallback, useEffect, useRef } from 'react'

const DEFAULT_RESUME_BURST_WINDOW_MS = 250

type Listener = (event: Event) => void

interface EventTargetLike {
  addEventListener(type: string, listener: Listener): void
  removeEventListener(type: string, listener: Listener): void
}

interface DocumentLike extends EventTargetLike {
  visibilityState?: DocumentVisibilityState
}

type WindowLike = EventTargetLike

export interface PageResumeRefreshControllerOptions {
  onError?: (error: unknown) => void
  refresh: () => Promise<void>
  burstWindowMs?: number
  documentTarget?: DocumentLike | null
  now?: () => number
  windowTarget?: WindowLike | null
}

export class PageResumeRefreshController {
  private readonly burstWindowMs: number
  private currentPromise: Promise<void> | null = null
  private readonly documentTarget: DocumentLike | null
  private disposed = false
  private lastRunStartedAt = Number.NEGATIVE_INFINITY
  private pending = false
  private readonly now: () => number
  private readonly onError?: (error: unknown) => void
  private readonly refresh: () => Promise<void>
  private running = false
  private readonly windowTarget: WindowLike | null

  constructor(options: PageResumeRefreshControllerOptions) {
    this.refresh = options.refresh
    this.onError = options.onError
    this.now = options.now ?? (() => Date.now())
    this.burstWindowMs = options.burstWindowMs ?? DEFAULT_RESUME_BURST_WINDOW_MS
    this.windowTarget = options.windowTarget ?? (typeof window === 'undefined' ? null : window)
    this.documentTarget = options.documentTarget ?? (typeof document === 'undefined' ? null : document)
  }

  attach() {
    if (!this.windowTarget) return
    this.windowTarget.addEventListener('focus', this.onWindowFocus)
    this.windowTarget.addEventListener('pageshow', this.onPageShow)
    this.documentTarget?.addEventListener('visibilitychange', this.onVisibilityChange)
  }

  detach() {
    if (!this.windowTarget) return
    this.windowTarget.removeEventListener('focus', this.onWindowFocus)
    this.windowTarget.removeEventListener('pageshow', this.onPageShow)
    this.documentTarget?.removeEventListener('visibilitychange', this.onVisibilityChange)
  }

  dispose() {
    this.disposed = true
    this.pending = false
    this.detach()
  }

  requestRefresh() {
    return this.enqueueRefresh()
  }

  private readonly onPageShow = (event: Event) => {
    const persisted =
      'persisted' in event && typeof (event as PageTransitionEvent).persisted === 'boolean'
        ? (event as PageTransitionEvent).persisted
        : false
    if (!persisted) return
    this.requestResumeRefresh()
  }

  private readonly onVisibilityChange = () => {
    if (this.documentTarget?.visibilityState !== 'visible') return
    this.requestResumeRefresh()
  }

  private readonly onWindowFocus = () => {
    this.requestResumeRefresh()
  }

  private enqueueRefresh(options?: { respectBurstWindow?: boolean }): Promise<void> {
    if (this.disposed) return Promise.resolve()
    const respectBurstWindow = options?.respectBurstWindow === true
    const now = this.now()

    if (this.running) {
      this.pending = true
      return this.currentPromise ?? Promise.resolve()
    }

    if (respectBurstWindow && now - this.lastRunStartedAt <= this.burstWindowMs) {
      return this.currentPromise ?? Promise.resolve()
    }

    this.pending = true
    if (!this.currentPromise) {
      this.currentPromise = this.drain().finally(() => {
        this.currentPromise = null
      })
    }
    return this.currentPromise
  }

  private requestResumeRefresh() {
    void this.enqueueRefresh({ respectBurstWindow: true }).catch(() => {
      // Errors are already forwarded through onError for passive resume refreshes.
    })
  }

  private async drain() {
    let lastError: unknown = null

    while (!this.disposed && this.pending) {
      this.pending = false
      this.running = true
      this.lastRunStartedAt = this.now()

      try {
        await this.refresh()
        lastError = null
      } catch (error: unknown) {
        lastError = error
        this.onError?.(error)
      } finally {
        this.running = false
      }
    }

    if (lastError != null) throw lastError
  }
}

export function createPageResumeRefreshController(options: PageResumeRefreshControllerOptions) {
  return new PageResumeRefreshController(options)
}

export function usePageResumeRefresh(
  refresh: () => Promise<void>,
  options?: {
    burstWindowMs?: number
    enabled?: boolean
    onError?: (error: unknown) => void
  },
) {
  const controllerRef = useRef<PageResumeRefreshController | null>(null)
  const refreshRef = useRef(refresh)
  const onErrorRef = useRef(options?.onError)

  useEffect(() => {
    refreshRef.current = refresh
  }, [refresh])

  useEffect(() => {
    onErrorRef.current = options?.onError
  }, [options?.onError])

  useEffect(() => {
    if (options?.enabled === false) return

    const controller = createPageResumeRefreshController({
      burstWindowMs: options?.burstWindowMs,
      onError: (error) => onErrorRef.current?.(error),
      refresh: () => refreshRef.current(),
    })
    controllerRef.current = controller
    controller.attach()
    return () => {
      if (controllerRef.current === controller) controllerRef.current = null
      controller.dispose()
    }
  }, [options?.burstWindowMs, options?.enabled])

  return useCallback(() => controllerRef.current?.requestRefresh() ?? refreshRef.current(), [])
}
