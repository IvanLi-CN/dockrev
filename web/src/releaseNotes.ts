import { ApiError, type ReleaseNotesView, type ServiceReleaseNoteItem, type ServiceReleaseNotesResponse } from './api'

export function normalizeReleaseVersion(value: string | null | undefined): string {
  return (value ?? '').trim().toLowerCase()
}

export function releaseNotesTagMatchesVersion(
  item: ServiceReleaseNoteItem,
  version: string | null | undefined,
): boolean {
  const normalizedVersion = normalizeReleaseVersion(version)
  if (!normalizedVersion) return false
  const normalizedTag = normalizeReleaseVersion(item.tagName)
  if (normalizedTag === normalizedVersion) return true
  if (normalizedTag === `v${normalizedVersion}`) return true
  if (normalizedVersion.startsWith('v') && normalizedTag === normalizedVersion.slice(1)) return true
  return false
}

export function releaseNotesBodyForView(
  item: ServiceReleaseNoteItem,
  view: ReleaseNotesView,
): { body: string; missing: boolean } {
  const original = (item.originalBody ?? '').trim()
  if (view === 'translated') {
    const translated = (item.translatedBody ?? '').trim()
    return translated ? { body: translated, missing: false } : { body: original, missing: true }
  }
  if (view === 'smart') {
    const smart = (item.smartBody ?? '').trim()
    return smart ? { body: smart, missing: false } : { body: original, missing: true }
  }
  return { body: original, missing: false }
}

export function releaseNotesViewLabel(view: ReleaseNotesView): string {
  if (view === 'original') return '原文'
  if (view === 'translated') return '翻译'
  return '润色'
}

export function releaseNotesSourceLabel(response: ServiceReleaseNotesResponse | null | undefined): string {
  if (!response) return '未知'
  return response.source === 'octoRill' ? 'OctoRill' : 'GitHub Releases'
}

export function releaseNotesShouldOfferSettingsAction(
  response: ServiceReleaseNotesResponse | null | undefined,
): boolean {
  const message = response?.message ?? response?.stale?.message ?? ''
  return message.includes('GitHub PAT') || message.includes('token 权限') || message.includes('OctoRill')
}

export function releaseNotesRefreshAlert(
  response: ServiceReleaseNotesResponse | null | undefined,
): { title: string; message: string } | null {
  if (response?.stale) {
    return { title: '发布记录暂未更新', message: response.stale.message }
  }
  if (response?.source !== 'octoRill' || !response.refresh) return null
  if (response.refresh.state === 'queued' || response.refresh.state === 'running') {
    return { title: '正在同步发布记录', message: '正在获取最新发布记录。' }
  }
  if (response.refresh.state === 'backoff') {
    return { title: '发布记录暂未更新', message: '正在显示最近一次成功结果。' }
  }
  return null
}

export function fallbackReleaseNotesErrorMessage(error: unknown): string {
  if (error instanceof ApiError && error.status === 404) {
    return '该服务不存在或已被删除，无法读取发布记录。'
  }
  if (error instanceof Error) {
    const message = error.message.trim()
    if (message) return message
  }
  return '发布记录拉取失败，请稍后重试。'
}

export function buildReleaseNotesFailureResponse(
  error: unknown,
  cursor: string | null,
  limit: number,
): ServiceReleaseNotesResponse {
  return {
    status: 'upstreamError',
    source: 'gitHub',
    repo: null,
    cursor,
    limit,
    nextCursor: null,
    previousCursor: null,
    hasMore: false,
    defaultView: 'original',
    items: [],
    message: fallbackReleaseNotesErrorMessage(error),
    stale: null,
    anchor: null,
  }
}

export function mergeReleaseNoteItems(
  existing: ServiceReleaseNoteItem[],
  incoming: ServiceReleaseNoteItem[],
  direction: 'older' | 'newer',
): ServiceReleaseNoteItem[] {
  const seen = new Set<string>()
  const ordered = direction === 'newer' ? [...incoming, ...existing] : [...existing, ...incoming]
  return ordered.filter((item) => {
    if (seen.has(item.id)) return false
    seen.add(item.id)
    return true
  })
}

export function findReleaseNoteIndex(
  items: ServiceReleaseNoteItem[],
  version: string | null | undefined,
): number {
  return items.findIndex((item) => releaseNotesTagMatchesVersion(item, version))
}

export function releaseNoteCardShouldReserveAside(input: {
  currentMatch: boolean
  showUpdate: boolean
  showRollback: boolean
  showCandidateStatus: boolean
  showRollbackDigestStatus: boolean
  showHistoricalStatus: boolean
}): boolean {
  return (
    input.currentMatch ||
    input.showUpdate ||
    input.showRollback ||
    input.showCandidateStatus ||
    input.showRollbackDigestStatus ||
    input.showHistoricalStatus
  )
}
