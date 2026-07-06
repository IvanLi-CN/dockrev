import type { JobProgressDownload } from './api'

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function optionalNumber(value: unknown): number | null | undefined {
  if (value === null) return null
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined
}

export function parseJobProgressDownload(value: unknown): JobProgressDownload | null {
  if (!isRecord(value)) return null
  const activeLayers = Array.isArray(value.activeLayers)
    ? value.activeLayers.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
    : []
  return {
    currentBytes: optionalNumber(value.currentBytes),
    totalBytes: optionalNumber(value.totalBytes),
    completedLayers: optionalNumber(value.completedLayers),
    totalLayers: optionalNumber(value.totalLayers),
    activeLayers,
    status: typeof value.status === 'string' || value.status === null ? value.status : undefined,
  }
}

export function formatDownloadBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '-'
  const units = [
    { suffix: 'GB', size: 1024 ** 3 },
    { suffix: 'MB', size: 1024 ** 2 },
    { suffix: 'KB', size: 1024 },
  ]
  for (const unit of units) {
    if (bytes >= unit.size) return `${(bytes / unit.size).toFixed(1)}${unit.suffix}`
  }
  return `${Math.round(bytes)}B`
}

export function formatJobProgressDownload(download?: JobProgressDownload | null): string | null {
  if (!download) return null
  const parts: string[] = []
  if (typeof download.currentBytes === 'number') {
    if (typeof download.totalBytes === 'number' && download.totalBytes > 0) {
      parts.push(`${formatDownloadBytes(download.currentBytes)} / ${formatDownloadBytes(download.totalBytes)}`)
    } else {
      parts.push(`已下载 ${formatDownloadBytes(download.currentBytes)}`)
    }
  }
  if (typeof download.completedLayers === 'number' && typeof download.totalLayers === 'number' && download.totalLayers > 0) {
    parts.push(`layers ${download.completedLayers}/${download.totalLayers}`)
  }
  const active = download.activeLayers?.[0]
  if (active) parts.push(active)
  if (parts.length > 0) return parts.join(' · ')
  return download.status?.trim() || null
}
