import { useEffect, useMemo, useRef, useState } from 'react'
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

  return (
    <div className="svcResourceChart">
      <svg viewBox={`0 0 ${width} ${height}`} className="svcResourceChartSvg" role="img" aria-label="服务资源趋势图">
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
          if (!path) return null
          return <path key={item.id} d={path} className={`svcResourceLine ${item.colorClass}`} />
        })}

        <text x={box.left} y={height - 10} className="svcResourceAxisLabel" textAnchor="start">
          {formatTime(xMin)}
        </text>
        <text x={box.left + box.width} y={height - 10} className="svcResourceAxisLabel" textAnchor="end">
          {formatTime(xMax)}
        </text>
      </svg>

      <div className="svcResourceLegend">
        {series.map((item) => (
          <div key={item.id} className="svcResourceLegendItem">
            <span className={`svcResourceLegendDot ${item.colorClass}`} />
            <span>{item.label}</span>
          </div>
        ))}
      </div>
    </div>
  )
}

export function ServiceResourcePanel(props: { serviceId: string }) {
  const { serviceId } = props

  const [windowKey, setWindowKey] = useState<ServiceResourceUsageWindow>('1h')
  const [metricTab, setMetricTab] = useState<MetricTabKey>('cpu')
  const [samples, setSamples] = useState<ServiceResourceSample[]>([])
  const [historyLoading, setHistoryLoading] = useState(false)
  const [historyError, setHistoryError] = useState<string | null>(null)
  const [monitorDisabled, setMonitorDisabled] = useState(false)
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
    if (typeof document === 'undefined') return undefined
    const onVisibilityChange = () => {
      setIsPageVisible(document.visibilityState === 'visible')
    }
    document.addEventListener('visibilitychange', onVisibilityChange)
    return () => document.removeEventListener('visibilitychange', onVisibilityChange)
  }, [])

  useEffect(() => {
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
  }, [serviceId, windowKey])

  useEffect(() => {
    if (!isPageVisible || monitorDisabled) {
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
  }, [isPageVisible, monitorDisabled, serviceId])

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

  const streamStatusLabel =
    streamState === 'live'
      ? '实时连接中（1s）'
      : streamState === 'connecting'
        ? '正在建立实时连接…'
        : streamState === 'reconnecting'
          ? '连接中断，正在重连…'
          : isPageVisible
            ? '未连接'
            : '页面不可见，实时连接已暂停'

  return (
    <div className="card svcResourceCard">
      <div className="title">资源监控</div>
      <div className="muted">历史趋势 + SSE 实时推送（1 秒）</div>

      {monitorDisabled ? (
        <div className="svcResourceNotice">资源监控已关闭，请在“系统设置 → 资源监控”中启用。</div>
      ) : (
        <>
          <div className="svcResourceStatGrid">
            <div className="svcResourceStatCard">
              <div className="svcResourceStatLabel">CPU</div>
              <div className="svcResourceStatValue">{formatPercent(latestSample?.cpuPercent ?? null)}</div>
            </div>
            <div className="svcResourceStatCard">
              <div className="svcResourceStatLabel">内存</div>
              <div className="svcResourceStatValue">{formatMemorySummary(latestSample)}</div>
            </div>
            <div className="svcResourceStatCard">
              <div className="svcResourceStatLabel">网络速率</div>
              <div className="svcResourceStatValue">{`↓ ${formatRate(latestNetworkRate.rx)} · ↑ ${formatRate(latestNetworkRate.tx)}`}</div>
            </div>
            <div className="svcResourceStatCard">
              <div className="svcResourceStatLabel">磁盘 I/O 速率</div>
              <div className="svcResourceStatValue">{`R ${formatRate(latestDiskRate.rx)} · W ${formatRate(latestDiskRate.tx)}`}</div>
            </div>
            <div className="svcResourceStatCard">
              <div className="svcResourceStatLabel">PIDs</div>
              <div className="svcResourceStatValue">{formatCount(latestSample?.pids)}</div>
            </div>
          </div>

          <div className="svcResourceChartWrap">
            <div className="svcResourceTabs" role="tablist" aria-label="监控指标切换">
              {TAB_OPTIONS.map((tab) => (
                <button
                  key={tab.key}
                  type="button"
                  className={tab.key === metricTab ? 'svcResourceTab active' : 'svcResourceTab'}
                  onClick={() => setMetricTab(tab.key)}
                  role="tab"
                  aria-selected={tab.key === metricTab}
                >
                  {tab.label}
                </button>
              ))}
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

          <div className="svcResourceFooter">
            <div className="svcResourceWindowSwitch" role="group" aria-label="时间窗口切换">
              {WINDOW_OPTIONS.map((option) => (
                <button
                  key={option.key}
                  type="button"
                  className={option.key === windowKey ? 'svcResourceWindowBtn active' : 'svcResourceWindowBtn'}
                  onClick={() => setWindowKey(option.key)}
                >
                  {option.label}
                </button>
              ))}
            </div>

            <div className="svcResourceStreamStatus">{streamError ? `实时状态：${streamError}` : streamStatusLabel}</div>
          </div>
        </>
      )}
    </div>
  )
}
