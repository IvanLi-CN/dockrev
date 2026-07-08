import { useEffect, useMemo, useRef, useState } from 'react'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import {
  ApiError,
  getServiceResourceUsageHistory,
  newServiceResourceUsageEventsSource,
  type ServiceResourceSample,
  type ServiceResourceUsageWindow,
} from '../api'

type MetricTabKey = 'cpu' | 'memory' | 'network' | 'disk' | 'pids'

type StreamState = 'idle' | 'connecting' | 'live' | 'reconnecting'

type ChartSeries = {
  id: string
  label: string
  colorClass: string
  points: Array<{ x: number; y: number | null }>
}

type RatePair = { rx: number | null; tx: number | null }

const WINDOW_OPTIONS: Array<{ key: ServiceResourceUsageWindow; label: string; seconds: number }> = [
  { key: '15m', label: '15m', seconds: 15 * 60 },
  { key: '1h', label: '1h', seconds: 60 * 60 },
  { key: '6h', label: '6h', seconds: 6 * 60 * 60 },
]

const WINDOW_SECONDS = WINDOW_OPTIONS.reduce<Record<ServiceResourceUsageWindow, number>>(
  (acc, item) => {
    acc[item.key] = item.seconds
    return acc
  },
  { '15m': 15 * 60, '1h': 60 * 60, '6h': 6 * 60 * 60 },
)

const TAB_OPTIONS: Array<{ key: MetricTabKey; label: string }> = [
  { key: 'cpu', label: 'CPU' },
  { key: 'memory', label: '内存' },
  { key: 'network', label: '网络' },
  { key: 'disk', label: '磁盘 I/O' },
  { key: 'pids', label: 'PIDs' },
]

const WINDOW_META_LABELS: Record<ServiceResourceUsageWindow, string> = {
  '15m': '最近 15 分钟',
  '1h': '最近 1 小时',
  '6h': '最近 6 小时',
}

const METRIC_PANEL_COPY: Record<
  MetricTabKey,
  { title: string; description: string; currentLabel: string }
> = {
  cpu: {
    title: 'CPU 占用趋势',
    description: '关注短时尖峰与持续占用，快速判断是否存在抖动或异常突增。',
    currentLabel: '当前 CPU',
  },
  memory: {
    title: '内存使用趋势',
    description: '聚焦容器已用内存与上限关系，适合观察增长是否接近资源边界。',
    currentLabel: '当前内存',
  },
  network: {
    title: '网络吞吐趋势',
    description: '同时观察 RX / TX 速率，判断实时流量波峰是否持续或异常偏置。',
    currentLabel: '当前网络',
  },
  disk: {
    title: '磁盘 I/O 趋势',
    description: '对比 Read / Write 速率，识别高频读写或突发型块设备压力。',
    currentLabel: '当前磁盘 I/O',
  },
  pids: {
    title: '进程数量趋势',
    description: '观察容器进程数是否稳定，排查泄漏、重启抖动或异常派生进程。',
    currentLabel: '当前 PIDs',
  },
}

const SSE_BACKOFF_MS = [1000, 2000, 5000]

export type ServiceResourceSnapshot = {
  fetchedAt?: string | null
  windowKey: ServiceResourceUsageWindow
  samples: ServiceResourceSample[]
  monitorDisabled?: boolean
}

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
  if (!(error instanceof ApiError)) return false
  return error.status === 409 && readReason(error.details) === 'resource_monitor_disabled'
}

function parseSampleTs(sample: ServiceResourceSample): number | null {
  const ts = Date.parse(sample.sampledAt)
  return Number.isFinite(ts) ? ts : null
}

function compareSamplesByTime(a: ServiceResourceSample, b: ServiceResourceSample): number {
  const ta = parseSampleTs(a) ?? 0
  const tb = parseSampleTs(b) ?? 0
  return ta - tb
}

function trimSortedSamples(samples: ServiceResourceSample[], windowSeconds: number): ServiceResourceSample[] {
  if (!samples.length) return []
  const latestTs = parseSampleTs(samples[samples.length - 1]) ?? Date.now()
  const cutoff = latestTs - windowSeconds * 1000
  return samples.filter((sample) => {
    const ts = parseSampleTs(sample)
    return ts === null || ts >= cutoff
  })
}

function trimSamplesToWindow(samples: ServiceResourceSample[], windowSeconds: number): ServiceResourceSample[] {
  if (!samples.length) return []
  const sorted = [...samples].sort(compareSamplesByTime)
  return trimSortedSamples(sorted, windowSeconds)
}

function appendSampleToSorted(samples: ServiceResourceSample[], sample: ServiceResourceSample): ServiceResourceSample[] {
  if (!samples.length) return [sample]

  const next = [...samples]
  const last = next[next.length - 1]
  if (last && last.sampledAt === sample.sampledAt) {
    next[next.length - 1] = sample
    return next
  }

  const sampleTs = parseSampleTs(sample)
  const lastTs = last ? parseSampleTs(last) : null
  if (sampleTs != null && lastTs != null && sampleTs >= lastTs) {
    next.push(sample)
    return next
  }

  const existingIndex = next.findIndex((item) => item.sampledAt === sample.sampledAt)
  if (existingIndex >= 0) {
    next[existingIndex] = sample
    return next
  }

  next.push(sample)
  next.sort(compareSamplesByTime)
  return next
}

function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes)) return '--'
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  let value = bytes
  let idx = 0
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024
    idx += 1
  }
  return `${value.toFixed(idx === 0 ? 0 : 1)} ${units[idx]}`
}

function formatRate(value: number | null): string {
  if (value == null || !Number.isFinite(value)) return '--'
  return `${formatBytes(value)}/s`
}

function formatCount(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return '--'
  return `${Math.round(value)}`
}

function formatPercent(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return '--'
  return `${value.toFixed(1)}%`
}

function formatMemorySummary(sample: ServiceResourceSample | null): string {
  if (!sample) return '--'
  if (sample.memUsedBytes == null) return '--'
  if (sample.memLimitBytes == null || sample.memLimitBytes <= 0) return formatBytes(sample.memUsedBytes)
  return `${formatBytes(sample.memUsedBytes)} / ${formatBytes(sample.memLimitBytes)}`
}

function formatSampleTime(sample: ServiceResourceSample | null): string {
  const ts = sample ? parseSampleTs(sample) : null
  return ts == null ? '暂无样本' : formatTime(ts)
}

function computeRatePairs(
  samples: ServiceResourceSample[],
  pickRx: (sample: ServiceResourceSample) => number | null | undefined,
  pickTx: (sample: ServiceResourceSample) => number | null | undefined,
): RatePair[] {
  if (!samples.length) return []
  const out: RatePair[] = []

  for (let i = 0; i < samples.length; i += 1) {
    if (i === 0) {
      out.push({ rx: null, tx: null })
      continue
    }

    const prev = samples[i - 1]
    const next = samples[i]
    const prevTs = parseSampleTs(prev)
    const nextTs = parseSampleTs(next)
    if (prevTs == null || nextTs == null || nextTs <= prevTs) {
      out.push({ rx: null, tx: null })
      continue
    }

    const dt = (nextTs - prevTs) / 1000
    const prevRx = pickRx(prev)
    const nextRx = pickRx(next)
    const prevTx = pickTx(prev)
    const nextTx = pickTx(next)

    const rx =
      prevRx != null && nextRx != null && nextRx >= prevRx && Number.isFinite(nextRx - prevRx)
        ? (nextRx - prevRx) / dt
        : null
    const tx =
      prevTx != null && nextTx != null && nextTx >= prevTx && Number.isFinite(nextTx - prevTx)
        ? (nextTx - prevTx) / dt
        : null

    out.push({ rx, tx })
  }

  return out
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

function buildPath(
  points: Array<{ x: number; y: number | null }>,
  domain: { xMin: number; xMax: number; yMin: number; yMax: number },
  box: { left: number; top: number; width: number; height: number },
): string {
  if (!points.length) return ''

  const xSpan = Math.max(1, domain.xMax - domain.xMin)
  const ySpan = Math.max(1e-6, domain.yMax - domain.yMin)

  const toX = (x: number) => box.left + ((x - domain.xMin) / xSpan) * box.width
  const toY = (y: number) => box.top + box.height - ((y - domain.yMin) / ySpan) * box.height

  let path = ''
  let drawing = false

  for (const point of points) {
    if (point.y == null || !Number.isFinite(point.y)) {
      drawing = false
      continue
    }

    const x = toX(point.x)
    const y = toY(point.y)
    if (!drawing) {
      path += `M ${x.toFixed(2)} ${y.toFixed(2)}`
      drawing = true
      continue
    }
    path += ` L ${x.toFixed(2)} ${y.toFixed(2)}`
  }

  return path
}

function scalePoint(
  point: { x: number; y: number },
  domain: { xMin: number; xMax: number; yMin: number; yMax: number },
  box: { left: number; top: number; width: number; height: number },
): { x: number; y: number } {
  const xSpan = Math.max(1, domain.xMax - domain.xMin)
  const ySpan = Math.max(1e-6, domain.yMax - domain.yMin)
  return {
    x: box.left + ((point.x - domain.xMin) / xSpan) * box.width,
    y: box.top + box.height - ((point.y - domain.yMin) / ySpan) * box.height,
  }
}

function buildAreaPaths(
  points: Array<{ x: number; y: number | null }>,
  domain: { xMin: number; xMax: number; yMin: number; yMax: number },
  box: { left: number; top: number; width: number; height: number },
): string[] {
  const segments: Array<Array<{ x: number; y: number }>> = []
  let currentSegment: Array<{ x: number; y: number }> = []

  for (const point of points) {
    if (point.y == null || !Number.isFinite(point.y)) {
      if (currentSegment.length) {
        segments.push(currentSegment)
        currentSegment = []
      }
      continue
    }
    currentSegment.push(scalePoint({ x: point.x, y: point.y }, domain, box))
  }

  if (currentSegment.length) segments.push(currentSegment)

  const baseY = box.top + box.height
  return segments.map((segment) => {
    const [first] = segment
    const last = segment[segment.length - 1]
    let path = `M ${first.x.toFixed(2)} ${baseY.toFixed(2)} L ${first.x.toFixed(2)} ${first.y.toFixed(2)}`
    for (let index = 1; index < segment.length; index += 1) {
      const point = segment[index]
      path += ` L ${point.x.toFixed(2)} ${point.y.toFixed(2)}`
    }
    path += ` L ${last.x.toFixed(2)} ${baseY.toFixed(2)} Z`
    return path
  })
}

function currentPointValue(points: Array<{ x: number; y: number | null }>): number | null {
  const value = points[points.length - 1]?.y
  return value != null && Number.isFinite(value) ? value : null
}

function currentPointMarker(
  points: Array<{ x: number; y: number | null }>,
  domain: { xMin: number; xMax: number; yMin: number; yMax: number },
  box: { left: number; top: number; width: number; height: number },
): { x: number; y: number } | null {
  const point = points[points.length - 1]
  if (!point || point.y == null || !Number.isFinite(point.y)) return null
  return scalePoint({ x: point.x, y: point.y }, domain, box)
}

function ResourceLineChart(props: {
  series: ChartSeries[]
  yFormatter: (value: number) => string
  emptyText: string
}) {
  const { series, yFormatter, emptyText } = props

  const allPoints = series
    .flatMap((item) => item.points)
    .filter((point) => point.y != null && Number.isFinite(point.y)) as Array<{
    x: number
    y: number
  }>

  if (!allPoints.length) {
    return <div className="svcResourceChartEmpty">{emptyText}</div>
  }

  const xMin = Math.min(...allPoints.map((point) => point.x))
  const rawXMax = Math.max(...allPoints.map((point) => point.x))
  const xMax = rawXMax > xMin ? rawXMax : xMin + 1000

  const yMin = 0
  const yMaxRaw = Math.max(...allPoints.map((point) => point.y))
  const yMax = yMaxRaw > 0 ? yMaxRaw * 1.08 : 1

  const gridTicks = [0, 0.25, 0.5, 0.75, 1]
  const width = 900
  const height = 280
  const box = {
    left: 50,
    right: 16,
    top: 16,
    bottom: 34,
    width: 900 - 50 - 16,
    height: 280 - 16 - 34,
  }

  const domain = { xMin, xMax, yMin, yMax }
  const singleSeries = series.length === 1

  return (
    <div className="svcResourceChart">
      <svg viewBox={`0 0 ${width} ${height}`} className="svcResourceChartSvg" role="img" aria-label="服务资源趋势图">
        <rect
          className="svcResourcePlotBackdrop"
          height={box.height}
          rx={20}
          width={box.width}
          x={box.left}
          y={box.top}
        />

        {gridTicks.map((tick) => {
          const y = box.top + box.height - tick * box.height
          const value = yMin + tick * (yMax - yMin)
          return (
            <g key={`grid-${tick}`}>
              <line x1={box.left} y1={y} x2={box.left + box.width} y2={y} className="svcResourceGridLine" />
              <text x={box.left - 8} y={y + 4} className="svcResourceAxisLabel" textAnchor="end">
                {yFormatter(value)}
              </text>
            </g>
          )
        })}

        {series.map((item) => {
          const path = buildPath(item.points, domain, {
            left: box.left,
            top: box.top,
            width: box.width,
            height: box.height,
          })
          const areaPaths = singleSeries
            ? buildAreaPaths(item.points, domain, {
                left: box.left,
                top: box.top,
                width: box.width,
                height: box.height,
              })
            : []
          const point = currentPointMarker(item.points, domain, {
            left: box.left,
            top: box.top,
            width: box.width,
            height: box.height,
          })
          if (!path) return null
          return (
            <g key={item.id} className={item.colorClass}>
              {areaPaths.map((areaPath, index) => (
                <path key={`${item.id}-area-${index}`} d={areaPath} className={`svcResourceArea ${item.colorClass}`} />
              ))}
              <path d={path} className={`svcResourceLine ${item.colorClass}`} />
              {point ? <circle className={`svcResourcePoint ${item.colorClass}`} cx={point.x} cy={point.y} r={4} /> : null}
            </g>
          )
        })}

        <text x={box.left} y={height - 10} className="svcResourceAxisLabel" textAnchor="start">
          {formatTime(xMin)}
        </text>
        <text x={box.left + box.width} y={height - 10} className="svcResourceAxisLabel" textAnchor="end">
          {formatTime(xMax)}
        </text>
      </svg>

      <div className="svcResourceLegend">
        {series.map((item) => {
          const latestValue = currentPointValue(item.points)
          return (
            <div key={item.id} className="svcResourceLegendItem">
              <span className={`svcResourceLegendDot ${item.colorClass}`} />
              <span className="svcResourceLegendLabel">{item.label}</span>
              <span className="svcResourceLegendValue">{latestValue == null ? '--' : yFormatter(latestValue)}</span>
            </div>
          )
        })}
      </div>
    </div>
  )
}

export function ServiceResourcePanel(props: {
  serviceId: string
  readonly?: boolean
  initialSnapshot?: ServiceResourceSnapshot | null
}) {
  const { serviceId, readonly = false, initialSnapshot = null } = props

  const [windowKey, setWindowKey] = useState<ServiceResourceUsageWindow>(
    initialSnapshot?.windowKey ?? '1h',
  )
  const [metricTab, setMetricTab] = useState<MetricTabKey>('cpu')
  const [samples, setSamples] = useState<ServiceResourceSample[]>(initialSnapshot?.samples ?? [])
  const [historyLoading, setHistoryLoading] = useState(false)
  const [historyError, setHistoryError] = useState<string | null>(null)
  const [monitorDisabled, setMonitorDisabled] = useState(
    initialSnapshot?.monitorDisabled === true,
  )
  const [streamState, setStreamState] = useState<StreamState>('idle')
  const [streamError, setStreamError] = useState<string | null>(null)
  const [isPageVisible, setIsPageVisible] = useState(() =>
    typeof document === 'undefined' ? true : document.visibilityState === 'visible',
  )

  const windowSecondsRef = useRef(WINDOW_SECONDS[windowKey])

  useEffect(() => {
    windowSecondsRef.current = WINDOW_SECONDS[windowKey]
  }, [windowKey])

  useEffect(() => {
    if (!readonly || !initialSnapshot) return
    setWindowKey(initialSnapshot.windowKey)
    setSamples(initialSnapshot.samples)
    setMonitorDisabled(initialSnapshot.monitorDisabled === true)
    setHistoryError(null)
    setHistoryLoading(false)
    setStreamError(null)
    setStreamState('idle')
  }, [initialSnapshot, readonly])

  useEffect(() => {
    if (typeof document === 'undefined') return undefined
    const onVisibilityChange = () => {
      setIsPageVisible(document.visibilityState === 'visible')
    }
    document.addEventListener('visibilitychange', onVisibilityChange)
    return () => document.removeEventListener('visibilitychange', onVisibilityChange)
  }, [])

  useEffect(() => {
    if (readonly) {
      setHistoryLoading(false)
      return undefined
    }
    let cancelled = false

    const load = async () => {
      setHistoryLoading(true)
      setHistoryError(null)

      try {
        const response = await getServiceResourceUsageHistory(serviceId, windowKey)
        if (cancelled) return
        setMonitorDisabled(false)
        setSamples(trimSamplesToWindow(response.samples, WINDOW_SECONDS[windowKey]))
      } catch (error: unknown) {
        if (cancelled) return
        if (isMonitorDisabledError(error)) {
          setMonitorDisabled(true)
          setSamples([])
          setHistoryError(null)
          setStreamError(null)
          return
        }
        setHistoryError(errorMessage(error))
      } finally {
        if (!cancelled) setHistoryLoading(false)
      }
    }

    void load()

    return () => {
      cancelled = true
    }
  }, [readonly, serviceId, windowKey])

  useEffect(() => {
    if (readonly || !isPageVisible || monitorDisabled) {
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

    const appendSample = (sample: ServiceResourceSample) => {
      setSamples((prev) => {
        const next = appendSampleToSorted(prev, sample)
        return trimSortedSamples(next, windowSecondsRef.current)
      })
    }

    const closeSource = () => {
      if (!eventSource) return
      eventSource.close()
      eventSource = null
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

    const onSampleEvent = (event: Event) => {
      const message = event as MessageEvent
      if (typeof message.data !== 'string' || !message.data) return
      try {
        const parsed = JSON.parse(message.data) as unknown
        if (!parsed || typeof parsed !== 'object') return
        const sample = (parsed as { sample?: ServiceResourceSample }).sample
        if (!sample || typeof sample !== 'object') return
        appendSample(sample)
        setStreamError(null)
      } catch {
        // Ignore malformed payloads to keep stream alive.
      }
    }

    const onErrorEvent = (event: Event) => {
      const message = event as MessageEvent
      if (typeof message.data !== 'string' || !message.data) return
      try {
        const parsed = JSON.parse(message.data) as unknown
        if (!parsed || typeof parsed !== 'object') return
        const error = (parsed as { error?: unknown }).error
        if (typeof error !== 'string' || !error) return
        if (error === 'resource_monitor_disabled') {
          setMonitorDisabled(true)
          setStreamError('资源监控已关闭，请在系统设置中启用后重试。')
          closeSource()
          setStreamState('idle')
          return
        }
        setStreamError(error)
      } catch {
        // Ignore malformed payloads to keep stream alive.
      }
    }

    const connect = () => {
      if (closed) return
      setStreamState(reconnectStep === 0 ? 'connecting' : 'reconnecting')

      const es = newServiceResourceUsageEventsSource(serviceId)
      eventSource = es
      es.onopen = () => {
        if (closed) return
        reconnectStep = 0
        setStreamState('live')
        setStreamError(null)
      }
      es.onerror = () => {
        if (closed) return
        closeSource()
        scheduleReconnect()
      }

      es.addEventListener('resource_usage_snapshot', onSampleEvent)
      es.addEventListener('resource_usage_tick', onSampleEvent)
      es.addEventListener('resource_usage_error', onErrorEvent)
    }

    connect()

    return () => {
      closed = true
      clearReconnectTimer()
      closeSource()
      setStreamState('idle')
    }
  }, [isPageVisible, monitorDisabled, readonly, serviceId])

  const networkRates = useMemo(
    () => computeRatePairs(samples, (sample) => sample.netRxBytes, (sample) => sample.netTxBytes),
    [samples],
  )
  const diskRates = useMemo(
    () => computeRatePairs(samples, (sample) => sample.blockReadBytes, (sample) => sample.blockWriteBytes),
    [samples],
  )

  const latestSample = samples.length ? samples[samples.length - 1] : null
  const latestNetworkRate = networkRates.length ? networkRates[networkRates.length - 1] : { rx: null, tx: null }
  const latestDiskRate = diskRates.length ? diskRates[diskRates.length - 1] : { rx: null, tx: null }

  const chartSeries = useMemo<ChartSeries[]>(() => {
    const basePoints = samples.map((sample) => ({
      x: parseSampleTs(sample) ?? Date.now(),
      sample,
    }))

    if (metricTab === 'cpu') {
      return [
        {
          id: 'cpu',
          label: 'CPU %',
          colorClass: 'svcResourceLineBlue',
          points: basePoints.map((point) => ({ x: point.x, y: point.sample.cpuPercent })),
        },
      ]
    }

    if (metricTab === 'memory') {
      return [
        {
          id: 'mem',
          label: '内存',
          colorClass: 'svcResourceLineBlue',
          points: basePoints.map((point) => ({ x: point.x, y: point.sample.memUsedBytes ?? null })),
        },
      ]
    }

    if (metricTab === 'network') {
      return [
        {
          id: 'net-rx',
          label: 'RX',
          colorClass: 'svcResourceLineBlue',
          points: basePoints.map((point, index) => ({ x: point.x, y: networkRates[index]?.rx ?? null })),
        },
        {
          id: 'net-tx',
          label: 'TX',
          colorClass: 'svcResourceLineOrange',
          points: basePoints.map((point, index) => ({ x: point.x, y: networkRates[index]?.tx ?? null })),
        },
      ]
    }

    if (metricTab === 'disk') {
      return [
        {
          id: 'disk-read',
          label: 'Read',
          colorClass: 'svcResourceLineBlue',
          points: basePoints.map((point, index) => ({ x: point.x, y: diskRates[index]?.rx ?? null })),
        },
        {
          id: 'disk-write',
          label: 'Write',
          colorClass: 'svcResourceLineOrange',
          points: basePoints.map((point, index) => ({ x: point.x, y: diskRates[index]?.tx ?? null })),
        },
      ]
    }

    return [
      {
        id: 'pids',
        label: 'PIDs',
        colorClass: 'svcResourceLineBlue',
        points: basePoints.map((point) => ({ x: point.x, y: point.sample.pids ?? null })),
      },
    ]
  }, [diskRates, metricTab, networkRates, samples])

  const yFormatter = useMemo(() => {
    if (metricTab === 'cpu') return (value: number) => `${value.toFixed(0)}%`
    if (metricTab === 'memory') return (value: number) => formatBytes(value)
    if (metricTab === 'network' || metricTab === 'disk') return (value: number) => formatRate(value)
    return (value: number) => `${Math.round(value)}`
  }, [metricTab])

  const streamStatusLabel = readonly
    ? '离线缓存（只读）'
    : streamState === 'live'
      ? '实时连接中（1s）'
      : streamState === 'connecting'
        ? '正在建立实时连接…'
        : streamState === 'reconnecting'
          ? '连接中断，正在重连…'
          : isPageVisible
            ? '未连接'
            : '页面不可见，实时连接已暂停'

  const streamBadge = useMemo(() => {
    if (readonly) return { label: '本地缓存', className: 'svcResourceStatusIdle' }
    if (monitorDisabled) return { label: '监控关闭', className: 'svcResourceStatusWarn' }
    if (streamError) return { label: '实时异常', className: 'svcResourceStatusBad' }
    if (streamState === 'live') return { label: '实时在线', className: 'svcResourceStatusLive' }
    if (streamState === 'connecting' || streamState === 'reconnecting') {
      return { label: streamState === 'connecting' ? '建立连接' : '正在重连', className: 'svcResourceStatusSync' }
    }
    if (!isPageVisible) return { label: '已暂停', className: 'svcResourceStatusIdle' }
    return { label: '未连接', className: 'svcResourceStatusIdle' }
  }, [isPageVisible, monitorDisabled, readonly, streamError, streamState])

  const activeMetric = TAB_OPTIONS.find((item) => item.key === metricTab) ?? TAB_OPTIONS[0]
  const activeMetricCopy = METRIC_PANEL_COPY[metricTab]
  const chartCurrentValue =
    metricTab === 'cpu'
      ? formatPercent(latestSample?.cpuPercent ?? null)
      : metricTab === 'memory'
        ? formatMemorySummary(latestSample)
        : metricTab === 'network'
          ? `↓ ${formatRate(latestNetworkRate.rx)} · ↑ ${formatRate(latestNetworkRate.tx)}`
          : metricTab === 'disk'
            ? `R ${formatRate(latestDiskRate.rx)} · W ${formatRate(latestDiskRate.tx)}`
            : formatCount(latestSample?.pids)

  const chartContext = historyLoading
    ? `${WINDOW_META_LABELS[windowKey]} · 正在加载历史样本`
    : samples.length > 0
      ? `${WINDOW_META_LABELS[windowKey]} · ${samples.length} 个${readonly ? '已缓存' : ''}样本`
      : `${WINDOW_META_LABELS[windowKey]} · 暂无${readonly ? '缓存' : '历史'}样本`

  const statCards = [
    {
      key: 'cpu',
      label: 'CPU',
      value: formatPercent(latestSample?.cpuPercent ?? null),
      meta: '实时占用率',
      primary: true,
    },
    {
      key: 'memory',
      label: '内存',
      value: formatMemorySummary(latestSample),
      meta: '已用 / 限额',
      primary: true,
    },
    {
      key: 'network',
      label: '网络速率',
      value: `↓ ${formatRate(latestNetworkRate.rx)} · ↑ ${formatRate(latestNetworkRate.tx)}`,
      meta: 'RX / TX',
      primary: false,
    },
    {
      key: 'disk',
      label: '磁盘 I/O',
      value: `R ${formatRate(latestDiskRate.rx)} · W ${formatRate(latestDiskRate.tx)}`,
      meta: 'Read / Write',
      primary: false,
    },
    {
      key: 'pids',
      label: 'PIDs',
      value: formatCount(latestSample?.pids),
      meta: '容器进程数',
      primary: false,
    },
  ]

  return (
    <div className="card svcResourceCard">
      <div className="svcResourceHero">
        <div className="svcResourceEyebrow">Service Observability</div>
        <div className="svcResourceHeroTop">
          <div className="svcResourceTitleBlock">
            <div className="title svcResourceTitle">资源监控</div>
            <div className="muted svcResourceSubtitle">
              {readonly
                ? '当前展示最近一次缓存到本地的监控样本；恢复联网后才会继续拉取历史并恢复实时推送。'
                : '历史趋势 + SSE 实时推送（1 秒），优先帮助你抓住尖峰、漂移和容器压力。'}
            </div>
          </div>

          <div className={`svcResourceStatusBadge ${streamBadge.className}`} role="status" aria-live="polite">
            <span className="svcResourceStatusDot" aria-hidden="true" />
            <span className="svcResourceStatusText">{streamBadge.label}</span>
          </div>
        </div>

        <div className="svcResourceFacts" aria-label="监控面板概览">
          <div className="svcResourceFact">{WINDOW_META_LABELS[windowKey]}</div>
          <div className="svcResourceFact">
            {historyLoading ? '加载样本中' : `${samples.length} 个${readonly ? '已缓存' : ''}样本`}
          </div>
          <div className="svcResourceFact">最近更新 {formatSampleTime(latestSample)}</div>
        </div>

        {streamError && !monitorDisabled && !readonly ? <div className="svcResourceSubtleAlert">实时状态：{streamError}</div> : null}
      </div>

      {monitorDisabled ? (
        <div className="svcResourceNotice">资源监控已关闭，请在“系统设置 → 资源监控”中启用。</div>
      ) : (
        <>
          <div className="svcResourceStatGrid">
            {statCards.map((card) => (
              <div
                key={card.key}
                className={card.primary ? 'svcResourceStatCard svcResourceStatCardPrimary' : 'svcResourceStatCard'}
              >
                <div className="svcResourceStatLabel">{card.label}</div>
                <div className="svcResourceStatValue">{card.value}</div>
                <div className="svcResourceStatMeta">{card.meta}</div>
              </div>
            ))}
          </div>

          <Tabs className="svcResourceChartWrap" onValueChange={(value) => setMetricTab(value as MetricTabKey)} value={metricTab}>
            <div className="svcResourceToolbar">
              <div className="svcResourceToolbarGroup">
                <div className="svcResourceToolbarLabel">监控指标</div>
                <TabsList className="svcResourceTabs" aria-label="监控指标切换">
                  {TAB_OPTIONS.map((tab) => (
                    <TabsTrigger
                      key={tab.key}
                      className={tab.key === metricTab ? 'svcResourceTab active' : 'svcResourceTab'}
                      value={tab.key}
                    >
                      {tab.label}
                    </TabsTrigger>
                  ))}
                </TabsList>
              </div>

              <div className="svcResourceToolbarGroup svcResourceToolbarGroupWindow">
                <div className="svcResourceToolbarLabel">时间范围</div>
                {readonly ? (
                  <div className="svcResourceWindowSwitch" aria-label="时间窗口切换">
                    <div className="svcResourceWindowBtn active" aria-disabled="true">
                      {WINDOW_META_LABELS[windowKey]}
                    </div>
                  </div>
                ) : (
                  <ToggleGroup
                    className="svcResourceWindowSwitch"
                    type="single"
                    value={windowKey}
                    onValueChange={(value) => {
                      if (!value) return
                      setWindowKey(value as ServiceResourceUsageWindow)
                    }}
                    aria-label="时间窗口切换"
                  >
                    {WINDOW_OPTIONS.map((option) => (
                      <ToggleGroupItem
                        key={option.key}
                        className={option.key === windowKey ? 'svcResourceWindowBtn active' : 'svcResourceWindowBtn'}
                        value={option.key}
                      >
                        {option.label}
                      </ToggleGroupItem>
                    ))}
                  </ToggleGroup>
                )}
              </div>
            </div>

            <div className="svcResourceChartStage">
              <div className="svcResourceChartStageHead">
                <div className="svcResourceChartTitleBlock">
                  <div className="svcResourceChartEyebrow">{activeMetric.label}</div>
                  <div className="svcResourceChartTitle">{activeMetricCopy.title}</div>
                  <div className="svcResourceChartDescription">{activeMetricCopy.description}</div>
                </div>

                <div className="svcResourceChartCurrentCard">
                  <div className="svcResourceChartCurrentLabel">{activeMetricCopy.currentLabel}</div>
                  <div className="svcResourceChartCurrentValue">{chartCurrentValue}</div>
                  <div className="svcResourceChartCurrentMeta">{chartContext}</div>
                </div>
              </div>

              {historyLoading ? (
                <div className="svcResourceChartEmpty">正在加载历史采样…</div>
              ) : historyError ? (
                <div className="svcResourceChartEmpty">历史数据加载失败：{historyError}</div>
              ) : (
                <ResourceLineChart
                  series={chartSeries}
                  yFormatter={yFormatter}
                  emptyText="当前窗口暂无可展示的监控数据"
                />
              )}
            </div>
          </Tabs>

          <div className="svcResourceStreamStatus">{streamStatusLabel}</div>
        </>
      )}
    </div>
  )
}
