import { useEffect, useRef } from 'react'

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
  private readonly documentTarget: DocumentLike | null
  private disposed = false
  private lastRunStartedAt = Number.NEGATIVE_INFINITY
  private queued = false
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
    this.queued = false
    this.detach()
  }

  requestRefresh() {
    this.schedule()
  }

  private readonly onPageShow = (event: Event) => {
    const persisted =
      'persisted' in event && typeof (event as PageTransitionEvent).persisted === 'boolean'
        ? (event as PageTransitionEvent).persisted
        : false
    if (!persisted) return
    this.schedule()
  }

  private readonly onVisibilityChange = () => {
    if (this.documentTarget?.visibilityState !== 'visible') return
    this.schedule()
  }

  private readonly onWindowFocus = () => {
    this.schedule()
  }

  private schedule() {
    if (this.disposed) return
    const now = this.now()

    if (this.running) {
      if (now - this.lastRunStartedAt <= this.burstWindowMs) return
      this.queued = true
      return
    }

    if (now - this.lastRunStartedAt <= this.burstWindowMs) return
    void this.run()
  }

  private async run() {
    if (this.disposed || this.running) return

    this.running = true
    this.lastRunStartedAt = this.now()

    try {
      await this.refresh()
    } catch (error: unknown) {
      this.onError?.(error)
    } finally {
      this.running = false
      const shouldRerun = !this.disposed && this.queued
      this.queued = false
      if (shouldRerun) void this.run()
    }
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
    controller.attach()
    return () => controller.dispose()
  }, [options?.burstWindowMs, options?.enabled])
}
