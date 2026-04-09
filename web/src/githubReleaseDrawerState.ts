import type {
  ServiceGitHubReleaseLocateResponse,
  ServiceGitHubReleaseLocateStatus,
  ServiceGitHubReleasesResponse,
  ServiceGitHubReleasesStatus,
} from './api'

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
