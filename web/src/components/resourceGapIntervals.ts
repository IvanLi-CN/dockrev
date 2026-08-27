import type { LifecycleAvailabilityInterval, ServiceLifecycleProjection, ServiceResourceSample } from '../api/serviceResourceTypes'

export type ResourceGapKind = 'service-stopped' | 'unavailable'

export type ResourceGapInterval = {
  start: number
  end: number
  kind: ResourceGapKind
  missingSamples: number
}

const GAP_RATIO_THRESHOLD = 1.5
const CONTINUOUS_MISSING_SAMPLE_COUNT = 2

function median(values: number[]): number | null {
  if (!values.length) return null
  const sorted = [...values].sort((a, b) => a - b)
  const middle = Math.floor(sorted.length / 2)
  return sorted.length % 2 === 0 ? (sorted[middle - 1]! + sorted[middle]!) / 2 : sorted[middle]!
}

function sampleTimes(samples: Array<Pick<ServiceResourceSample, 'sampledAt'>>): number[] {
  return [...new Set(samples.map((sample) => Date.parse(sample.sampledAt)).filter(Number.isFinite))].sort((a, b) => a - b)
}

function downtimeIntervals(lifecycle: ServiceLifecycleProjection | null | undefined): Array<{ start: number; end: number }> {
  return (lifecycle?.availabilityIntervals ?? [])
    .filter((interval: LifecycleAvailabilityInterval) => interval.complete)
    .map((interval) => {
      const started = Date.parse(interval.startedAt)
      const stopped = Date.parse(interval.stoppedAt)
      if (!Number.isFinite(started) || !Number.isFinite(stopped) || started === stopped) return null
      return { start: Math.min(started, stopped), end: Math.max(started, stopped) }
    })
    .filter((interval): interval is { start: number; end: number } => interval !== null)
}

export function deriveResourceGapIntervals(
  samples: Array<Pick<ServiceResourceSample, 'sampledAt'>>,
  lifecycle: ServiceLifecycleProjection | null | undefined,
): ResourceGapInterval[] {
  const times = sampleTimes(samples)
  if (times.length < 2) return []
  const cadence = median(times.slice(1).map((time, index) => time - times[index]!))
  if (cadence == null || cadence <= 0) return []
  const downtime = downtimeIntervals(lifecycle)
  const gaps: ResourceGapInterval[] = []
  for (let index = 1; index < times.length; index += 1) {
    const start = times[index - 1]!
    const end = times[index]!
    const delta = end - start
    if (delta <= cadence * GAP_RATIO_THRESHOLD) continue
    const missingSamples = Math.max(1, Math.round(delta / cadence) - 1)
    const kind = downtime.some((interval) => start < interval.end && end > interval.start)
      ? 'service-stopped'
      : 'unavailable'
    gaps.push({ start, end, kind, missingSamples })
  }
  return gaps
}

export function isContinuousResourceGap(gap: ResourceGapInterval): boolean {
  return gap.missingSamples >= CONTINUOUS_MISSING_SAMPLE_COUNT
}

