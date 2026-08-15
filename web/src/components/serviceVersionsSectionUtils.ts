import type { ServiceReleaseNoteItem } from '../api'

export function observeVersionSectionInlineWidth(
  element: HTMLElement,
  onWidth: (width: number) => void,
): () => void {
  if (typeof window === 'undefined') return () => {}

  const measure = () => onWidth(element.getBoundingClientRect().width)
  measure()

  if (typeof window.ResizeObserver !== 'undefined') {
    let observer: ResizeObserver | null = null
    try {
      observer = new window.ResizeObserver((entries) => {
        const entry = entries[0]
        if (entry) onWidth(entry.contentRect.width)
      })
      observer.observe(element)
      return () => observer?.disconnect()
    } catch {
      observer?.disconnect()
    }
  }

  let frameId: number | null = null
  const scheduleMeasure = () => {
    if (frameId !== null) return
    frameId = window.requestAnimationFrame(() => {
      frameId = null
      measure()
    })
  }
  const shell = element.closest<HTMLElement>('.appShell')
  const observer =
    shell && typeof window.MutationObserver !== 'undefined'
      ? new window.MutationObserver(scheduleMeasure)
      : null

  if (observer && shell) {
    observer.observe(shell, { attributes: true, attributeFilter: ['class', 'style'] })
  }
  window.addEventListener('resize', scheduleMeasure)
  return () => {
    observer?.disconnect()
    window.removeEventListener('resize', scheduleMeasure)
    if (frameId !== null) window.cancelAnimationFrame(frameId)
  }
}

export function formatReleaseDate(value: string | null | undefined): {
  dateLine: string
  timeLine: string | null
} {
  const trimmed = (value ?? '').trim()
  if (!trimmed) return { dateLine: '时间未知', timeLine: null }
  const parsed = new Date(trimmed)
  if (Number.isNaN(parsed.valueOf())) return { dateLine: trimmed, timeLine: null }
  return {
    dateLine: new Intl.DateTimeFormat(undefined, {
      dateStyle: 'medium',
    }).format(parsed),
    timeLine: new Intl.DateTimeFormat(undefined, {
      timeStyle: 'short',
    }).format(parsed),
  }
}

export function preferredReleaseTimestamp(item: ServiceReleaseNoteItem): string | null {
  return item.publishedAt?.trim() || item.createdAt?.trim() || null
}

export function formatVersionDirectoryTimeLabel(
  value: string | null | undefined,
  nowMs = Date.now(),
): string {
  const trimmed = (value ?? '').trim()
  if (!trimmed) return '时间未知'
  const parsed = new Date(trimmed)
  const parsedMs = parsed.valueOf()
  if (Number.isNaN(parsedMs)) return trimmed

  const deltaMs = nowMs - parsedMs
  if (deltaMs >= 0) {
    if (deltaMs < 60_000) return '刚刚'
    if (deltaMs < 60 * 60_000) return `${Math.floor(deltaMs / 60_000)} 分钟前`
    if (deltaMs < 24 * 60 * 60_000) return `${Math.floor(deltaMs / (60 * 60_000))} 小时前`
    if (deltaMs <= 7 * 24 * 60 * 60_000) return `${Math.floor(deltaMs / (24 * 60 * 60_000))} 天前`
  }

  const year = parsed.getFullYear()
  const month = String(parsed.getMonth() + 1).padStart(2, '0')
  const day = String(parsed.getDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

export function normalizeVersion(value: string | null | undefined): string {
  return (value ?? '').trim().toLowerCase()
}

export function safeHttpUrl(value: string | null | undefined): string {
  const trimmed = (value ?? '').trim()
  if (!trimmed) return ''
  try {
    const url = new URL(trimmed)
    return url.protocol === 'http:' || url.protocol === 'https:' ? url.toString() : ''
  } catch {
    return ''
  }
}

export function releaseMatchesVersion(
  item: Pick<ServiceReleaseNoteItem, 'tagName'>,
  version: string | null | undefined,
): boolean {
  const normalizedVersion = normalizeVersion(version)
  if (!normalizedVersion) return false
  const normalizedTag = normalizeVersion(item.tagName)
  if (normalizedTag === normalizedVersion) return true
  if (normalizedTag === `v${normalizedVersion}`) return true
  if (normalizedVersion.startsWith('v') && normalizedTag === normalizedVersion.slice(1)) return true
  return false
}

export function mergeReleaseNoteItems(
  existing: ServiceReleaseNoteItem[],
  incoming: ServiceReleaseNoteItem[],
): ServiceReleaseNoteItem[] {
  if (existing.length === 0) return [...incoming]
  if (incoming.length === 0) return existing
  const seen = new Set(existing.map((item) => item.id))
  const merged = [...existing]
  for (const item of incoming) {
    if (seen.has(item.id)) continue
    seen.add(item.id)
    merged.push(item)
  }
  return merged
}
