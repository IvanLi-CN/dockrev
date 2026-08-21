import type { ServiceResourceSnapshot } from '../api'

type ServiceDetailMonitorSample = ServiceResourceSnapshot['samples'][number]

export function isMonitorDisabledError(error: unknown): boolean {
  if (!(error instanceof Error) || !('status' in error) || !('details' in error)) return false
  const details = error.details
  return error.status === 409 && typeof details === 'object' && details !== null && (details as Record<string, unknown>).reason === 'resource_monitor_disabled'
}

function parseMonitorSampleTime(sample: ServiceDetailMonitorSample | null): number | null {
  if (!sample) return null
  const timestamp = Date.parse(sample.sampledAt)
  return Number.isFinite(timestamp) ? timestamp : null
}

export function formatMonitorPercent(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return '-'
  return value < 10 ? `${value.toFixed(1)}%` : `${value.toFixed(0)}%`
}

export function formatMonitorBytes(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes)) return '-'
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  let value = bytes
  let index = 0
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024
    index += 1
  }
  const digits = index === 0 || value >= 100 ? 0 : value >= 10 ? 1 : 2
  return `${value.toFixed(digits)} ${units[index]}`
}

export function formatMonitorRate(value: number | null): string {
  if (value == null || !Number.isFinite(value)) return '-'
  if (value < 1) return '0 B/s'
  return `${formatMonitorBytes(value)}/s`
}

export function computeMonitorTerminalRate(
  previousSample: ServiceDetailMonitorSample | null,
  latestSample: ServiceDetailMonitorSample | null,
  pick: (sample: ServiceDetailMonitorSample) => number | null | undefined,
): number | null {
  const previousTimestamp = parseMonitorSampleTime(previousSample)
  const latestTimestamp = parseMonitorSampleTime(latestSample)
  if (previousTimestamp == null || latestTimestamp == null || latestTimestamp <= previousTimestamp || !previousSample || !latestSample) return null
  const previousValue = pick(previousSample)
  const latestValue = pick(latestSample)
  if (previousValue == null || latestValue == null || !Number.isFinite(previousValue) || !Number.isFinite(latestValue) || latestValue < previousValue) return null
  return (latestValue - previousValue) / ((latestTimestamp - previousTimestamp) / 1000)
}
