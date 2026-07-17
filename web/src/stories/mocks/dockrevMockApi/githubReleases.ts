import type {
  GitHubReleaseAuthMode,
  ServiceGitHubReleaseItem,
  ServiceGitHubReleasesResponse,
  ServiceGitHubReleasesStatus,
  ServiceGitHubRepoRef,
} from '../../../api'
import type { DockrevMockApiOptions, DockrevMockGitHubReleasesDataset } from './shared'
import { hashString, nowIso, offsetMockVersion } from './shared'
import type { FindServiceResult } from './context'

function mockGitHubReleaseErrorMessage(
  status: ServiceGitHubReleasesStatus,
  authMode: GitHubReleaseAuthMode,
): string | null {
  if (status === 'ready') return null
  if (status === 'unsupportedRepo') return '该服务尚未配置 GitHub repoUrl，当前只支持 GitHub Releases。'
  if (status === 'permissionDenied') {
    return authMode === 'pat'
      ? '当前 GitHub PAT 无法访问该仓库的 Releases，请检查权限范围或仓库可见性。'
      : '匿名请求无法访问该仓库的 Releases，请前往“设置 -> GitHub Packages”配置 PAT 后重试。'
  }
  if (status === 'rateLimited') {
    return authMode === 'pat'
      ? 'GitHub API 请求已达到速率限制，请稍后再试。'
      : '匿名请求已命中 GitHub API 速率限制，请前往“设置 -> GitHub Packages”配置 PAT 后重试。'
  }
  return '读取 GitHub Releases 失败，请稍后重试。'
}

function buildDefaultMockGitHubReleaseItems(
  serviceId: string,
  findService: (serviceId: string) => FindServiceResult,
  parseMockGitHubRepoRef: (input: string | null | undefined) => ServiceGitHubRepoRef | null,
): ServiceGitHubReleaseItem[] {
  const found = findService(serviceId)
  if (!found) return []
  const runningVersion =
    found.svc.image.resolvedTag?.trim() ||
    found.svc.image.tag?.trim() ||
    '1.0.0'
  const candidateVersion =
    found.svc.candidate?.resolvedTag?.trim() ||
    found.svc.candidate?.tag?.trim() ||
    offsetMockVersion(runningVersion, 1, '1.0.1')
  const versions = Array.from(
    new Set([
      candidateVersion,
      offsetMockVersion(candidateVersion, -1, runningVersion),
      offsetMockVersion(candidateVersion, -2, runningVersion),
      runningVersion,
    ]),
  )
  return versions.map((tagName, index) => ({
    id: 10_000 + index + hashString(`${serviceId}:${tagName}`),
    tagName,
    name: tagName,
    body:
      index === 0
        ? `Release notes for ${tagName}\\n\\n- Improve update visibility\\n- Keep discovery timeline linked to releases`
        : `Release notes for ${tagName}`,
    htmlUrl: `https://github.com/${(parseMockGitHubRepoRef(found.svc.settings.repoUrl)?.fullName ?? 'acme/example')}/releases/tag/${encodeURIComponent(tagName)}`,
    draft: false,
    prerelease: index > 0 && tagName.includes('rc'),
    publishedAt: nowIso(-(index + 1) * 36 * 60 * 1000),
    createdAt: nowIso(-(index + 1) * 40 * 60 * 1000),
  }))
}

export function buildMockGitHubReleasesDataset(
  serviceId: string,
  options: DockrevMockApiOptions,
  findService: (serviceId: string) => FindServiceResult,
  parseMockGitHubRepoRef: (input: string | null | undefined) => ServiceGitHubRepoRef | null,
): DockrevMockGitHubReleasesDataset {
  const explicit = options.githubReleasesByServiceId?.[serviceId]
  if (explicit) {
    return {
      authMode: explicit.authMode ?? 'anonymous',
      repo: explicit.repo ?? null,
      listStatus: explicit.listStatus ?? 'ready',
      listMessage: explicit.listMessage ?? null,
      items: explicit.items?.map((item) => ({ ...item })) ?? [],
      locateByVersion: explicit.locateByVersion,
    }
  }

  const found = findService(serviceId)
  const repo = parseMockGitHubRepoRef(found?.svc.settings.repoUrl)
  if (!repo) {
    return {
      authMode: 'anonymous',
      repo: null,
      listStatus: 'unsupportedRepo',
      listMessage: mockGitHubReleaseErrorMessage('unsupportedRepo', 'anonymous'),
      items: [],
    }
  }

  return {
    authMode: 'anonymous',
    repo,
    listStatus: 'ready',
    listMessage: null,
    items: buildDefaultMockGitHubReleaseItems(serviceId, findService, parseMockGitHubRepoRef),
  }
}

export function buildMockGitHubReleasesResponse(
  serviceId: string,
  page: number,
  perPage: number,
  options: DockrevMockApiOptions,
  findService: (serviceId: string) => FindServiceResult,
  parseMockGitHubRepoRef: (input: string | null | undefined) => ServiceGitHubRepoRef | null,
): ServiceGitHubReleasesResponse {
  const dataset = buildMockGitHubReleasesDataset(serviceId, options, findService, parseMockGitHubRepoRef)
  const status = dataset.listStatus ?? 'ready'
  const authMode = dataset.authMode ?? 'anonymous'
  const items = dataset.items ?? []
  if (status !== 'ready') {
    return {
      status,
      authMode,
      repo: dataset.repo ?? null,
      page,
      perPage,
      hasMore: false,
      items: [],
      message: dataset.listMessage ?? mockGitHubReleaseErrorMessage(status, authMode),
    }
  }

  const offset = (page - 1) * perPage
  const paged = items.slice(offset, offset + perPage)
  return {
    status: 'ready',
    authMode,
    repo: dataset.repo ?? null,
    page,
    perPage,
    hasMore: offset + perPage < items.length,
    items: paged.map((item) => ({ ...item })),
    message: dataset.listMessage ?? null,
  }
}
