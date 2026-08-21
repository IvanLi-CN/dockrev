import { useMemo, useState } from 'react'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import type { ServiceResourceSample } from '../api'
import {
  buildResourceChartPaths,
  scaleResourceChartPoint,
  type ResourceChartInterpolation,
} from './resourceChartPaths'
import { AsyncDataRegion, AsyncDataSkeleton } from './AsyncDataRegion'
import type { AsyncDataPhase } from '../asyncData'
import {
  RESOURCE_WINDOW_META_LABELS,
  RESOURCE_WINDOW_OPTIONS,
  trimSamplesToWindow,
  type ServiceDetailResourceMonitorPanelState,
} from '../pages/useServiceDetailResourceMonitor'

type MetricTabKey = 'cpu' | 'memory' | 'network' | 'disk' | 'pids'

type ChartSeries = {
  id: string
  label: string
  colorClass: string
  interpolation: ResourceChartInterpolation
  points: Array<{ x: number; y: number | null }>
}

type RatePair = { rx: number | null; tx: number | null }

const TAB_OPTIONS: Array<{ key: MetricTabKey; label: string }> = [
  { key: 'cpu', label: 'CPU' },
  { key: 'memory', label: '内存' },
  { key: 'network', label: '网络' },
  { key: 'disk', label: '磁盘 I/O' },
  { key: 'pids', label: 'PIDs' },
]

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

const MAX_AGGREGATED_CHART_POINTS = 480

function parseSampleTs(sample: ServiceResourceSample): number | null {
  const ts = Date.parse(sample.sampledAt)
  return Number.isFinite(ts) ? ts : null
}

export { trimSamplesToWindow }

function chartSamplesForWindow(samples: ServiceResourceSample[], isAggregatedWindow: boolean): ServiceResourceSample[] {
  if (!isAggregatedWindow || samples.length <= MAX_AGGREGATED_CHART_POINTS) return samples

  const stride = Math.ceil((samples.length - 1) / (MAX_AGGREGATED_CHART_POINTS - 1))
  return samples.filter((_, index) => index === samples.length - 1 || index % stride === 0)
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
  return scaleResourceChartPoint({ x: point.x, y: point.y }, domain, box)
}

function ResourceLineChart(props: {
  series: ChartSeries[]
  yFormatter: (value: number) => string
  emptyText: string
  latestPeakLabel?: string | null
}) {
  const { series, yFormatter, emptyText, latestPeakLabel } = props
  const renderedPointCount = Math.max(0, ...series.map((item) => item.points.length))

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
    <div className="svcResourceChart" data-point-count={renderedPointCount}>
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
          const { linePath, areaPaths } = buildResourceChartPaths({
            points: item.points,
            domain,
            box: {
              left: box.left,
              top: box.top,
              width: box.width,
              height: box.height,
            },
            interpolation: item.interpolation,
            includeArea: singleSeries,
          })
          const point = currentPointMarker(item.points, domain, {
            left: box.left,
            top: box.top,
            width: box.width,
            height: box.height,
          })
          if (!linePath) return null
          return (
            <g key={item.id} className={item.colorClass}>
              {areaPaths.map((areaPath, index) => (
                <path key={`${item.id}-area-${index}`} d={areaPath} className={`svcResourceArea ${item.colorClass}`} />
              ))}
              <path d={linePath} className={`svcResourceLine ${item.colorClass}`} />
              {point ? (
                <circle className={`svcResourcePoint ${item.colorClass}`} cx={point.x} cy={point.y} r={4}>
                  {latestPeakLabel ? <title>{latestPeakLabel}</title> : null}
                </circle>
              ) : null}
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

export function ServiceResourcePanel(props: { monitor: ServiceDetailResourceMonitorPanelState }) {
  const {
    windowKey,
    samples,
    peaks,
    historyLoading,
    historyLoaded,
    historyTrigger,
    historyError,
    monitorDisabled,
    streamState,
    streamError,
    isPageVisible,
    readonly: effectiveReadonly,
    isOnline,
    onWindowChange,
    onRetry,
  } = props.monitor
  const [metricTab, setMetricTab] = useState<MetricTabKey>('cpu')
  const isAggregatedWindow = windowKey === '7d' || windowKey === '30d'

  const chartSamples = useMemo(
    () => chartSamplesForWindow(samples, isAggregatedWindow),
    [isAggregatedWindow, samples],
  )

  const networkRates = useMemo(
    () =>
      chartSamples.some((sample) => sample.netRxRateBps != null || sample.netTxRateBps != null)
        ? chartSamples.map((sample) => ({ rx: sample.netRxRateBps ?? null, tx: sample.netTxRateBps ?? null }))
        : computeRatePairs(chartSamples, (sample) => sample.netRxBytes, (sample) => sample.netTxBytes),
    [chartSamples],
  )
  const diskRates = useMemo(
    () =>
      chartSamples.some((sample) => sample.blockReadRateBps != null || sample.blockWriteRateBps != null)
        ? chartSamples.map((sample) => ({ rx: sample.blockReadRateBps ?? null, tx: sample.blockWriteRateBps ?? null }))
        : computeRatePairs(chartSamples, (sample) => sample.blockReadBytes, (sample) => sample.blockWriteBytes),
    [chartSamples],
  )

  const latestSample = samples.length ? samples[samples.length - 1] : null
  const latestPeak = peaks.length ? peaks[peaks.length - 1] : null
  const latestNetworkRate = networkRates.length ? networkRates[networkRates.length - 1] : { rx: null, tx: null }
  const latestDiskRate = diskRates.length ? diskRates[diskRates.length - 1] : { rx: null, tx: null }

  const chartSeries = useMemo<ChartSeries[]>(() => {
    const basePoints = chartSamples.map((sample) => ({
      x: parseSampleTs(sample) ?? 0,
      sample,
    }))

    if (metricTab === 'cpu') {
      return [
        {
          id: 'cpu',
          label: 'CPU %',
          colorClass: 'svcResourceLineBlue',
          interpolation: 'step-after-rounded',
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
          interpolation: 'step-after-rounded',
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
          interpolation: 'step-after-rounded',
          points: basePoints.map((point, index) => ({ x: point.x, y: networkRates[index]?.rx ?? null })),
        },
        {
          id: 'net-tx',
          label: 'TX',
          colorClass: 'svcResourceLineOrange',
          interpolation: 'step-after-rounded',
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
          interpolation: 'step-after-rounded',
          points: basePoints.map((point, index) => ({ x: point.x, y: diskRates[index]?.rx ?? null })),
        },
        {
          id: 'disk-write',
          label: 'Write',
          colorClass: 'svcResourceLineOrange',
          interpolation: 'step-after-rounded',
          points: basePoints.map((point, index) => ({ x: point.x, y: diskRates[index]?.tx ?? null })),
        },
      ]
    }

    return [
      {
        id: 'pids',
        label: 'PIDs',
        colorClass: 'svcResourceLineBlue',
        interpolation: 'step-after',
        points: basePoints.map((point) => ({ x: point.x, y: point.sample.pids ?? null })),
      },
    ]
  }, [chartSamples, diskRates, metricTab, networkRates])

  const yFormatter = useMemo(() => {
    if (metricTab === 'cpu') return (value: number) => `${value.toFixed(0)}%`
    if (metricTab === 'memory') return (value: number) => formatBytes(value)
    if (metricTab === 'network' || metricTab === 'disk') return (value: number) => formatRate(value)
    return (value: number) => `${Math.round(value)}`
  }, [metricTab])

  const streamStatusLabel = effectiveReadonly
    ? '离线缓存（只读）'
    : isAggregatedWindow
      ? '聚合历史（只读）'
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
    if (effectiveReadonly) return { label: '本地缓存', className: 'svcResourceStatusIdle' }
    if (isAggregatedWindow) return { label: '聚合历史', className: 'svcResourceStatusIdle' }
    if (monitorDisabled) return { label: '监控关闭', className: 'svcResourceStatusWarn' }
    if (streamError) return { label: '实时异常', className: 'svcResourceStatusBad' }
    if (streamState === 'live') return { label: '实时在线', className: 'svcResourceStatusLive' }
    if (streamState === 'connecting' || streamState === 'reconnecting') {
      return { label: streamState === 'connecting' ? '建立连接' : '正在重连', className: 'svcResourceStatusSync' }
    }
    if (!isPageVisible) return { label: '已暂停', className: 'svcResourceStatusIdle' }
    return { label: '未连接', className: 'svcResourceStatusIdle' }
  }, [isAggregatedWindow, isPageVisible, monitorDisabled, effectiveReadonly, streamError, streamState])

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

  const latestPeakLabel = latestPeak
    ? metricTab === 'cpu'
      ? `此桶峰值 CPU ${formatPercent(latestPeak.cpuPercent)}`
      : metricTab === 'memory'
        ? `此桶峰值内存 ${formatBytes(latestPeak.memUsedBytes ?? null)}`
        : metricTab === 'network'
          ? `此桶峰值 RX ${formatRate(latestPeak.netRxRateBps ?? null)}，TX ${formatRate(latestPeak.netTxRateBps ?? null)}`
          : metricTab === 'disk'
            ? `此桶峰值读 ${formatRate(latestPeak.blockReadRateBps ?? null)}，写 ${formatRate(latestPeak.blockWriteRateBps ?? null)}`
            : `此桶峰值 PIDs ${formatCount(latestPeak.pids)}`
    : null

  const sampleUnit = effectiveReadonly ? '已缓存' : isAggregatedWindow ? '聚合桶' : '样本（含实时点）'
  const historyPhase: AsyncDataPhase = !isOnline && samples.length === 0 && !monitorDisabled
    ? 'offline'
    : historyError
      ? 'error'
      : historyLoading
        ? historyLoaded ? 'refreshing' : 'initial-loading'
        : samples.length === 0 ? 'ready-empty' : 'ready-data'
  const chartContext = historyLoading
    ? `${RESOURCE_WINDOW_META_LABELS[windowKey]} · 正在加载历史样本`
    : samples.length > 0
      ? `${RESOURCE_WINDOW_META_LABELS[windowKey]} · ${samples.length} 个${sampleUnit}`
      : `${RESOURCE_WINDOW_META_LABELS[windowKey]} · 暂无${effectiveReadonly ? '缓存' : '历史或实时'}样本`

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
    <div
      className="card svcResourceCard"
      data-resource-window={windowKey}
      data-resource-current-sampled-at={latestSample?.sampledAt ?? ''}
    >
      <div className="svcResourceHero">
        <div className="svcResourceEyebrow">Service Observability</div>
        <div className="svcResourceHeroTop">
          <div className="svcResourceTitleBlock">
            <div className="title svcResourceTitle">资源监控</div>
            <div className="muted svcResourceSubtitle">
              {effectiveReadonly
                ? '当前展示最近一次缓存到本地的监控样本；恢复联网后才会继续拉取历史并恢复实时推送。'
                : isAggregatedWindow
                  ? '长时间窗口按时间桶展示历史均值；最近桶保留峰值提示。'
                  : '历史样本按设置频率对每个 compose project 采集；页面打开后会叠加 1 秒 SSE 实时点，优先帮助你抓住尖峰、漂移和容器压力。'}
            </div>
          </div>

          <div className={`svcResourceStatusBadge ${streamBadge.className}`} role="status" aria-live="polite">
            <span className="svcResourceStatusDot" aria-hidden="true" />
            <span className="svcResourceStatusText">{streamBadge.label}</span>
          </div>
        </div>

        <div className="svcResourceFacts" aria-label="监控面板概览">
          <div className="svcResourceFact">{RESOURCE_WINDOW_META_LABELS[windowKey]}</div>
          <div className="svcResourceFact">
            {historyLoading ? '加载样本中' : `${samples.length} 个${sampleUnit}`}
          </div>
          <div className="svcResourceFact">最近更新 {formatSampleTime(latestSample)}</div>
        </div>

        {streamError && !monitorDisabled && !effectiveReadonly ? <div className="svcResourceSubtleAlert">实时状态：{streamError}</div> : null}
      </div>

      {monitorDisabled ? (
        <div className="svcResourceNotice">
          <span>资源监控已关闭，请在“系统设置 → 资源监控”中启用。</span>
          {!effectiveReadonly && isOnline ? (
            <button type="button" className="svcResourceNoticeAction" onClick={onRetry}>
              重试
            </button>
          ) : null}
        </div>
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
                {effectiveReadonly ? (
                  <div className="svcResourceWindowSwitch" aria-label="时间窗口切换">
                    <div className="svcResourceWindowBtn active" aria-disabled="true">
                      {RESOURCE_WINDOW_META_LABELS[windowKey]}
                    </div>
                  </div>
                ) : (
                  <ToggleGroup
                    className="svcResourceWindowSwitch"
                    type="single"
                    value={windowKey}
                    onValueChange={(value) => {
                      if (!value) return
                      onWindowChange(value as Parameters<typeof onWindowChange>[0])
                    }}
                    aria-label="时间窗口切换"
                  >
                    {RESOURCE_WINDOW_OPTIONS.map((option) => (
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

            <AsyncDataRegion
              className="svcResourceChartStage"
              error={historyError}
              hasData={historyLoaded}
              label="正在刷新监控历史"
              onRetry={onRetry}
              phase={historyPhase}
              skeleton={<AsyncDataSkeleton className="svcResourceChartLoadingSkeleton" lines={5} />}
              trigger={historyTrigger}
            >
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

              <ResourceLineChart
                series={chartSeries}
                yFormatter={yFormatter}
                emptyText="当前窗口暂无可展示的监控数据"
                latestPeakLabel={latestPeakLabel}
              />
            </AsyncDataRegion>
          </Tabs>

          <div className="svcResourceStreamStatus">{streamStatusLabel}</div>
        </>
      )}
    </div>
  )
}
