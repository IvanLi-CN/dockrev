import type { ServiceReleaseNoteItem, ServiceReleaseNotesSource } from '../../../api'

export function clampReleaseNotesLimit(
  rawValue: string | null | undefined,
  { fallback, max }: { fallback: number; max: number },
): number {
  const parsed = Number(rawValue ?? String(fallback))
  if (!Number.isFinite(parsed)) return fallback
  return Math.min(max, Math.max(1, Math.trunc(parsed)))
}

export function parseReleaseNotesCursor(rawValue: string | null | undefined): number {
  const parsed = Number(rawValue ?? '0')
  if (!Number.isFinite(parsed)) return 0
  return Math.max(0, Math.trunc(parsed))
}

function normalizeMockReleaseVersion(value: string | null | undefined): string {
  return (value ?? '').trim().toLowerCase()
}

export function mockReleaseTagMatchesVersion(tagName: string, version: string | null | undefined): boolean {
  const normalizedVersion = normalizeMockReleaseVersion(version)
  if (!normalizedVersion) return false
  const normalizedTag = normalizeMockReleaseVersion(tagName)
  if (normalizedTag === normalizedVersion) return true
  if (normalizedTag === `v${normalizedVersion}`) return true
  if (normalizedVersion.startsWith('v') && normalizedTag === normalizedVersion.slice(1)) return true
  return false
}

export function buildMockReleaseNotesExternalLinks(
  repoHtmlUrl: string | null | undefined,
  repoFullName: string | null | undefined,
  octoRillApiBaseUrl: string | null | undefined,
) {
  const githubReleasesUrl = repoHtmlUrl ? `${repoHtmlUrl.replace(/\/+$/, '')}/releases` : null
  let octoRillReleasesUrl: string | null = null
  if (repoFullName && octoRillApiBaseUrl?.trim()) {
    const [owner, repo] = repoFullName.split('/')
    if (owner?.trim() && repo?.trim()) {
      try {
        const releaseUrl = new URL(octoRillApiBaseUrl.trim())
        const baseSegments = releaseUrl.pathname.split('/').filter(Boolean)
        releaseUrl.pathname = `/${[...baseSegments, owner.trim(), repo.trim(), 'releases'].join('/')}`
        octoRillReleasesUrl = releaseUrl.toString()
      } catch {
        octoRillReleasesUrl = null
      }
    }
  }
  return githubReleasesUrl
    ? {
        githubReleasesUrl,
        ...(octoRillReleasesUrl ? { octoRillReleasesUrl } : {}),
      }
    : null
}

export function buildMockSmartReleaseSummary(title: string): string {
  const subject = /^release\s+\d/i.test(title) || /^\d+(?:\.\d+)+(?:[-+].*)?$/.test(title) ? '本次更新' : title
  return [
    `润色摘要：${subject}主要优化发布说明阅读体验与维护决策效率。`,
    '',
    '- 将关键变更压缩成可快速扫读的摘要，减少从原文里反复定位重点。',
    '- 适合在维护窗口中判断升级收益、影响范围和是否需要继续查看原文。',
  ].join('\n')
}

type MockGitHubReleaseItem = {
  id: string | number
  tagName: string
  name?: string | null
  body?: string | null
  htmlUrl: string
  draft: boolean
  prerelease: boolean
  publishedAt?: string | null
  createdAt?: string | null
}

export function buildMockReleaseNotesItems(
  items: MockGitHubReleaseItem[],
  source: ServiceReleaseNotesSource,
): ServiceReleaseNoteItem[] {
  const useOctoRill = source === 'octoRill'
  return items.map((item, index) => {
    const title = item.name && item.name !== item.tagName ? item.name : item.tagName
    return {
      id: useOctoRill ? `octorill:${item.id}` : `github:${item.id}`,
      tagName: item.tagName,
      name: item.name,
      originalBody: item.body,
      translatedBody:
        useOctoRill && index % 4 !== 2
          ? `翻译：${item.name ?? item.tagName}\n\n${item.body ?? '暂无原始说明'}`
          : null,
      smartBody: useOctoRill && index % 4 !== 3 ? buildMockSmartReleaseSummary(title) : null,
      htmlUrl: item.htmlUrl,
      draft: item.draft,
      prerelease: item.prerelease,
      publishedAt: item.publishedAt,
      createdAt: item.createdAt,
    }
  })
}
