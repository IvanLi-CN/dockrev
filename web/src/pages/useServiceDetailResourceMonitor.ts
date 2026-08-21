import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  ApiError,
  getServiceResourceUsageHistory,
  newServiceResourceUsageEventsSource,
  type ServiceResourcePeak,
  type ServiceResourceSample,
  type ServiceResourceSnapshot,
  type ServiceResourceUsageWindow,
} from '../api'
import type { AsyncDataTrigger } from '../asyncData'

export type ServiceResourceStreamState = 'idle' | 'connecting' | 'live' | 'reconnecting'

export const RESOURCE_WINDOW_OPTIONS: Array<{
  key: ServiceResourceUsageWindow
  label: string
  seconds: number
}> = [
  { key: '3m', label: '3m', seconds: 3 * 60 },
  { key: '1h', label: '1h', seconds: 60 * 60 },
  { key: '24h', label: '24h', seconds: 24 * 60 * 60 },
  { key: '7d', label: '7d', seconds: 7 * 24 * 60 * 60 },
  { key: '30d', label: '30d', seconds: 30 * 24 * 60 * 60 },
]

export const RESOURCE_WINDOW_SECONDS: Record<ServiceResourceUsageWindow, number> =
  RESOURCE_WINDOW_OPTIONS.reduce<Record<ServiceResourceUsageWindow, number>>(
    (acc, item) => {
      acc[item.key] = item.seconds
      return acc
    },
    { '3m': 3 * 60, '1h': 60 * 60, '24h': 24 * 60 * 60, '7d': 7 * 24 * 60 * 60, '30d': 30 * 24 * 60 * 60 },
  )

export const RESOURCE_WINDOW_META_LABELS: Record<ServiceResourceUsageWindow, string> = {
  '3m': '最近 3 分钟',
  '1h': '最近 1 小时',
  '24h': '最近 24 小时',
  '7d': '最近 7 天',
  '30d': '最近 30 天',
}

const SUMMARY_WINDOW: ServiceResourceUsageWindow = '1h'
const SSE_BACKOFF_MS = [1000, 2000, 5000]

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  return String(error)
}

function readReason(details: unknown): string | null {
  if (!details || typeof details !== 'object') return null
  const reason = (details as Record<string, unknown>).reason
  return typeof reason === 'string' ? reason : null
}

function isMonitorDisabledError(error: unknown): boolean {
  return error instanceof ApiError && error.status === 409 && readReason(error.details) === 'resource_monitor_disabled'
}

function parseSampleTs(sample: ServiceResourceSample): number | null {
  const ts = Date.parse(sample.sampledAt)
  return Number.isFinite(ts) ? ts : null
}

function compareSamplesByTime(a: ServiceResourceSample, b: ServiceResourceSample): number {
  return (parseSampleTs(a) ?? 0) - (parseSampleTs(b) ?? 0)
}

export function trimSamplesToWindow(
  samples: ServiceResourceSample[],
  windowSeconds: number,
  now = Date.now(),
): ServiceResourceSample[] {
  if (!samples.length) return []
  const cutoff = now - Math.max(0, windowSeconds) * 1000
  return samples
    .filter((sample) => {
      const ts = parseSampleTs(sample)
      return ts !== null && ts >= cutoff && ts <= now
    })
    .sort(compareSamplesByTime)
}

function appendSampleToSorted(samples: ServiceResourceSample[], sample: ServiceResourceSample): ServiceResourceSample[] {
  if (!samples.length) return [sample]
  const next = [...samples]
  const existingIndex = next.findIndex((item) => item.sampledAt === sample.sampledAt)
  if (existingIndex >= 0) {
    next[existingIndex] = sample
    return next.sort(compareSamplesByTime)
  }
  next.push(sample)
  return next.sort(compareSamplesByTime)
}

function mergeSamples(
  samples: ServiceResourceSample[],
  liveSamples: ServiceResourceSample[],
  windowSeconds: number,
): ServiceResourceSample[] {
  let merged = trimSamplesToWindow(samples, windowSeconds)
  for (const sample of liveSamples) merged = appendSampleToSorted(merged, sample)
  return trimSamplesToWindow(merged, windowSeconds)
}

export type ServiceDetailResourceMonitorPanelState = {
  windowKey: ServiceResourceUsageWindow
  samples: ServiceResourceSample[]
  peaks: ServiceResourcePeak[]
  historyLoading: boolean
  historyLoaded: boolean
  historyTrigger: AsyncDataTrigger
  historyError: string | null
  monitorDisabled: boolean
  streamState: ServiceResourceStreamState
  streamError: string | null
  isPageVisible: boolean
  readonly: boolean
  isOnline: boolean
  onWindowChange: (windowKey: ServiceResourceUsageWindow) => void
  onRetry: () => void
}

export type ServiceDetailResourceMonitorController = {
  summarySnapshot: ServiceResourceSnapshot | null
  panel: ServiceDetailResourceMonitorPanelState
}

export function useServiceDetailResourceMonitor(props: {
  serviceId: string
  readonly?: boolean
  initialSnapshot?: ServiceResourceSnapshot | null
  isOnline?: boolean
}): ServiceDetailResourceMonitorController {
  const { serviceId, readonly = false, initialSnapshot = null, isOnline = true } = props
  const initialWindow = initialSnapshot?.windowKey ?? '1h'
  const initialPanelSamples = initialSnapshot
    ? trimSamplesToWindow(initialSnapshot.samples, RESOURCE_WINDOW_SECONDS[initialWindow])
    : []
  const initialSummarySamples = initialSnapshot
    ? trimSamplesToWindow(initialSnapshot.samples, RESOURCE_WINDOW_SECONDS[SUMMARY_WINDOW])
    : []
  const initialHasData = initialPanelSamples.length > 0 || initialSnapshot?.monitorDisabled === true
  const initialSummaryHasData = initialSummarySamples.length > 0 || initialSnapshot?.monitorDisabled === true

  const [windowKey, setWindowKey] = useState<ServiceResourceUsageWindow>(initialWindow)
  const [panelSamples, setPanelSamples] = useState<ServiceResourceSample[]>(initialPanelSamples)
  const [summarySamples, setSummarySamples] = useState<ServiceResourceSample[]>(initialSummarySamples)
  const [peaks, setPeaks] = useState<ServiceResourcePeak[]>([])
  const [historyLoading, setHistoryLoading] = useState(!readonly && isOnline)
  const [historyLoaded, setHistoryLoaded] = useState(initialHasData)
  const [summaryLoaded, setSummaryLoaded] = useState(initialSummaryHasData)
  const [historyTrigger, setHistoryTrigger] = useState<AsyncDataTrigger>('background')
  const [historyReloadTick, setHistoryReloadTick] = useState(0)
  const [historyError, setHistoryError] = useState<string | null>(null)
  const [monitorDisabled, setMonitorDisabled] = useState(initialSnapshot?.monitorDisabled === true)
  const [streamState, setStreamState] = useState<ServiceResourceStreamState>('idle')
  const [streamError, setStreamError] = useState<string | null>(null)
  const [isPageVisible, setIsPageVisible] = useState(() =>
    typeof document === 'undefined' ? true : document.visibilityState === 'visible',
  )

  const liveSamplesRef = useRef<ServiceResourceSample[]>([])
  const windowKeyRef = useRef(windowKey)
  const panelHistoryRequestIdRef = useRef(0)
  const summaryHistoryRequestIdRef = useRef(0)

  windowKeyRef.current = windowKey

  useEffect(() => {
    liveSamplesRef.current = []
    panelHistoryRequestIdRef.current += 1
    summaryHistoryRequestIdRef.current += 1
    setWindowKey(initialWindow)
    setPanelSamples([])
    setSummarySamples([])
    setSummaryLoaded(false)
    setPeaks([])
    setMonitorDisabled(false)
    setHistoryLoading(!readonly && isOnline)
    setHistoryLoaded(false)
    setHistoryError(null)
    setStreamError(null)
    setStreamState('idle')
  }, [initialWindow, isOnline, readonly, serviceId])

  useEffect(() => {
    if (readonly && initialSnapshot) {
      const nextPanelSamples = trimSamplesToWindow(initialSnapshot.samples, RESOURCE_WINDOW_SECONDS[initialSnapshot.windowKey])
      const nextSummarySamples = trimSamplesToWindow(initialSnapshot.samples, RESOURCE_WINDOW_SECONDS[SUMMARY_WINDOW])
      liveSamplesRef.current = []
      setWindowKey(initialSnapshot.windowKey)
      setPanelSamples(nextPanelSamples)
      setSummarySamples(nextSummarySamples)
      setSummaryLoaded(nextSummarySamples.length > 0 || initialSnapshot.monitorDisabled === true)
      setPeaks([])
      setMonitorDisabled(initialSnapshot.monitorDisabled === true)
      setHistoryError(null)
      setHistoryLoading(false)
      setHistoryLoaded(nextPanelSamples.length > 0 || initialSnapshot.monitorDisabled === true)
      setStreamError(null)
      setStreamState('idle')
      return
    }
    if (readonly && !initialSnapshot && !isOnline) {
      liveSamplesRef.current = []
      setPanelSamples([])
      setSummarySamples([])
      setSummaryLoaded(false)
      setPeaks([])
      setMonitorDisabled(false)
      setHistoryLoading(false)
      setHistoryLoaded(false)
      setStreamError(null)
      setStreamState('idle')
    }
  }, [initialSnapshot, isOnline, readonly, serviceId])

  useEffect(() => {
    if (typeof document === 'undefined') return undefined
    const onVisibilityChange = () => setIsPageVisible(document.visibilityState === 'visible')
    document.addEventListener('visibilitychange', onVisibilityChange)
    return () => document.removeEventListener('visibilitychange', onVisibilityChange)
  }, [])

  useEffect(() => {
    if (readonly || !isOnline || monitorDisabled) {
      setHistoryLoading(false)
      return undefined
    }

    const requestId = ++panelHistoryRequestIdRef.current
    let cancelled = false
    setHistoryLoading(true)
    setHistoryError(null)

    void (async () => {
      try {
        const response = await getServiceResourceUsageHistory(serviceId, windowKey)
        if (cancelled || requestId !== panelHistoryRequestIdRef.current) return
        const isAggregatedWindow = windowKey === '7d' || windowKey === '30d'
        const nextSamples = isAggregatedWindow
          ? trimSamplesToWindow(response.samples, RESOURCE_WINDOW_SECONDS[windowKey])
          : mergeSamples(response.samples, liveSamplesRef.current, RESOURCE_WINDOW_SECONDS[windowKey])
        setPanelSamples(nextSamples)
        setPeaks(response.peaks ?? [])
        setMonitorDisabled(false)
        setHistoryLoaded(true)
        if (windowKey === SUMMARY_WINDOW) {
          setSummarySamples(mergeSamples(response.samples, liveSamplesRef.current, RESOURCE_WINDOW_SECONDS[SUMMARY_WINDOW]))
          setSummaryLoaded(true)
        }
      } catch (error: unknown) {
        if (cancelled || requestId !== panelHistoryRequestIdRef.current) return
        if (isMonitorDisabledError(error)) {
          setMonitorDisabled(true)
          setPanelSamples([])
          setPeaks([])
          setSummarySamples([])
          setSummaryLoaded(true)
          setHistoryError(null)
          setStreamError(null)
          setHistoryLoaded(false)
          return
        }
        setHistoryError(errorMessage(error))
      } finally {
        if (!cancelled && requestId === panelHistoryRequestIdRef.current) setHistoryLoading(false)
      }
    })()

    return () => {
      cancelled = true
    }
  }, [historyReloadTick, isOnline, monitorDisabled, readonly, serviceId, windowKey])

  useEffect(() => {
    if (readonly || !isOnline || windowKey === SUMMARY_WINDOW || monitorDisabled) return undefined
    let cancelled = false
    const requestId = ++summaryHistoryRequestIdRef.current
    void (async () => {
      try {
        const response = await getServiceResourceUsageHistory(serviceId, SUMMARY_WINDOW)
        if (cancelled || requestId !== summaryHistoryRequestIdRef.current) return
        setSummarySamples(mergeSamples(response.samples, liveSamplesRef.current, RESOURCE_WINDOW_SECONDS[SUMMARY_WINDOW]))
      } catch (error: unknown) {
        if (cancelled || requestId !== summaryHistoryRequestIdRef.current) return
        if (isMonitorDisabledError(error)) {
          liveSamplesRef.current = []
          setPanelSamples([])
          setSummarySamples([])
          setPeaks([])
          setMonitorDisabled(true)
          setSummaryLoaded(true)
          setHistoryLoaded(false)
          setHistoryError(null)
          setStreamError(null)
          return
        }
        setStreamError(errorMessage(error))
      }
    })()
    return () => {
      cancelled = true
    }
  }, [isOnline, monitorDisabled, readonly, serviceId, windowKey])

  useEffect(() => {
    if (readonly || !isOnline || !isPageVisible || monitorDisabled) {
      setStreamState('idle')
      return undefined
    }

    let closed = false
    let eventSource: EventSource | null = null
    let reconnectTimer: number | null = null
    let reconnectStep = 0

    const clearReconnectTimer = () => {
      if (reconnectTimer != null) {
        window.clearTimeout(reconnectTimer)
        reconnectTimer = null
      }
    }

    const closeSource = () => {
      if (!eventSource) return
      eventSource.close()
      eventSource = null
    }

    const appendLiveSample = (sample: ServiceResourceSample) => {
      liveSamplesRef.current = trimSamplesToWindow(
        appendSampleToSorted(liveSamplesRef.current, sample),
        RESOURCE_WINDOW_SECONDS[SUMMARY_WINDOW],
      )
      setSummarySamples((previous) => trimSamplesToWindow(appendSampleToSorted(previous, sample), RESOURCE_WINDOW_SECONDS[SUMMARY_WINDOW]))
      setSummaryLoaded(true)
      if (windowKeyRef.current === '7d' || windowKeyRef.current === '30d') return
      setPanelSamples((previous) =>
        trimSamplesToWindow(
          appendSampleToSorted(previous, sample),
          RESOURCE_WINDOW_SECONDS[windowKeyRef.current],
        ),
      )
    }

    const onSampleEvent = (event: Event) => {
      const data = (event as MessageEvent).data
      if (typeof data !== 'string' || !data) return
      try {
        const parsed = JSON.parse(data) as { sample?: ServiceResourceSample }
        if (!parsed.sample || typeof parsed.sample !== 'object') return
        appendLiveSample(parsed.sample)
        setStreamError(null)
      } catch {
        // Ignore malformed payloads and keep the connection alive.
      }
    }

    const onErrorEvent = (event: Event) => {
      const data = (event as MessageEvent).data
      if (typeof data !== 'string' || !data) return
      try {
        const parsed = JSON.parse(data) as { error?: unknown }
        if (parsed.error === 'resource_monitor_disabled') {
          liveSamplesRef.current = []
          setPanelSamples([])
          setSummarySamples([])
          setPeaks([])
          setMonitorDisabled(true)
          setSummaryLoaded(true)
          setHistoryLoaded(false)
          setHistoryError(null)
          setStreamError('资源监控已关闭，请在系统设置中启用后重试。')
          closeSource()
          setStreamState('idle')
          return
        }
        if (typeof parsed.error === 'string' && parsed.error) setStreamError(parsed.error)
      } catch {
        // Ignore malformed payloads and keep the connection alive.
      }
    }

    const scheduleReconnect = () => {
      if (closed || reconnectTimer != null) return
      const delay = SSE_BACKOFF_MS[Math.min(reconnectStep, SSE_BACKOFF_MS.length - 1)]
      reconnectStep += 1
      setStreamState('reconnecting')
      reconnectTimer = window.setTimeout(() => {
        reconnectTimer = null
        connect()
      }, delay)
    }

    const connect = () => {
      if (closed) return
      setStreamState(reconnectStep === 0 ? 'connecting' : 'reconnecting')
      const source = newServiceResourceUsageEventsSource(serviceId)
      eventSource = source
      source.onopen = () => {
        if (closed) return
        reconnectStep = 0
        setStreamState('live')
        setStreamError(null)
      }
      source.onerror = () => {
        if (closed) return
        closeSource()
        scheduleReconnect()
      }
      source.addEventListener('resource_usage_snapshot', onSampleEvent)
      source.addEventListener('resource_usage_tick', onSampleEvent)
      source.addEventListener('resource_usage_error', onErrorEvent)
    }

    connect()
    return () => {
      closed = true
      clearReconnectTimer()
      closeSource()
      setStreamState('idle')
    }
  }, [isOnline, isPageVisible, monitorDisabled, readonly, serviceId])

  const onWindowChange = useCallback((nextWindowKey: ServiceResourceUsageWindow) => {
    setHistoryTrigger('user-action')
    setPeaks([])
    setHistoryLoaded(false)
    setPanelSamples((current) =>
      nextWindowKey === '7d' || nextWindowKey === '30d'
        ? []
        : trimSamplesToWindow(current, RESOURCE_WINDOW_SECONDS[nextWindowKey]),
    )
    setWindowKey(nextWindowKey)
  }, [])

  const onRetry = useCallback(() => {
    setHistoryTrigger('user-action')
    setMonitorDisabled(false)
    setSummaryLoaded(false)
    setHistoryLoaded(false)
    setHistoryReloadTick((current) => current + 1)
  }, [])

  const summarySnapshot = useMemo<ServiceResourceSnapshot | null>(() => {
    if (!summaryLoaded && !monitorDisabled) return null
    return {
      fetchedAt: summarySamples.at(-1)?.sampledAt ?? null,
      windowKey: SUMMARY_WINDOW,
      samples: summarySamples,
      monitorDisabled,
    }
  }, [monitorDisabled, summaryLoaded, summarySamples])

  const panel = useMemo<ServiceDetailResourceMonitorPanelState>(
    () => ({
      windowKey,
      samples: panelSamples,
      peaks,
      historyLoading,
      historyLoaded,
      historyTrigger,
      historyError,
      monitorDisabled,
      streamState,
      streamError,
      isPageVisible,
      readonly,
      isOnline,
      onWindowChange,
      onRetry,
    }),
    [
      historyError,
      historyLoaded,
      historyLoading,
      historyTrigger,
      isOnline,
      isPageVisible,
      monitorDisabled,
      onRetry,
      onWindowChange,
      panelSamples,
      peaks,
      readonly,
      streamError,
      streamState,
      windowKey,
    ],
  )

  return { summarySnapshot, panel }
}
