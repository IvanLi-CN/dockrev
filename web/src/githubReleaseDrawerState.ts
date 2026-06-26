import type {
  ServiceGitHubReleaseLocateResponse,
  ServiceGitHubReleaseLocateStatus,
  ServiceGitHubReleasesResponse,
  ServiceGitHubReleasesStatus,
} from './api'

export const RELEASE_DRAWER_LOCATE_LIMIT = 50

function locateStatusFromListStatus(status: ServiceGitHubReleasesStatus): ServiceGitHubReleaseLocateStatus {
  switch (status) {
    case 'permissionDenied':
      return 'permissionDenied'
    case 'rateLimited':
      return 'rateLimited'
    case 'unsupportedRepo':
      return 'unsupportedRepo'
    case 'upstreamError':
    case 'ready':
    default:
      return 'upstreamError'
  }
}

export function applyLocatePreloadFailure(
  locateResponse: ServiceGitHubReleaseLocateResponse,
  preloadResponse: ServiceGitHubReleasesResponse | null,
): ServiceGitHubReleaseLocateResponse {
  if (!preloadResponse || preloadResponse.status === 'ready') {
    return locateResponse
  }

  const matchedVersion = locateResponse.matchedTag ?? locateResponse.version
  const detail = preloadResponse.message?.trim()
  return {
    ...locateResponse,
    status: locateStatusFromListStatus(preloadResponse.status),
    message: detail
      ? `已定位到 ${matchedVersion}，但加载对应发布页失败：${detail}`
      : `已定位到 ${matchedVersion}，但加载对应发布页失败，请稍后重试。`,
  }
}

export function shouldContinueReleaseLocateSearch(
  response: ServiceGitHubReleasesResponse | null,
  searchedCount: number,
  limit = RELEASE_DRAWER_LOCATE_LIMIT,
): boolean {
  if (!response || response.status !== 'ready' || !response.hasMore) {
    return false
  }
  return searchedCount < limit
}

export function buildReleaseLocateNotFoundResponse(
  listResponse: ServiceGitHubReleasesResponse | null,
  version: string,
  searchedCount: number,
  limit = RELEASE_DRAWER_LOCATE_LIMIT,
): ServiceGitHubReleaseLocateResponse {
  const reachedLimit = searchedCount >= limit && (listResponse?.hasMore ?? false)
  return {
    status: reachedLimit ? 'outsideWindow' : 'notFound',
    authMode: listResponse?.authMode ?? 'anonymous',
    repo: listResponse?.repo ?? null,
    version,
    searchedCount,
    matchedTag: reachedLimit ? version : null,
    page: null,
    indexWithinPage: null,
    absoluteIndex: null,
    message: reachedLimit
      ? `已定位到 ${version}，但它不在前 ${searchedCount} 条发布记录内。`
      : `在前 ${searchedCount} 条发布记录中未找到 ${version}。`,
  }
}
