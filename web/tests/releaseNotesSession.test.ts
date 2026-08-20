import { afterEach, describe, expect, test } from 'bun:test'

import type { ServiceReleaseNoteItem, ServiceReleaseNotesResponse } from '../src/api'
import { __releaseNotesSessionTestUtils } from '../src/useServiceReleaseNotesSession'

function makeItem(tagName: string): ServiceReleaseNoteItem {
  return {
    id: `release:${tagName}`,
    tagName,
    name: tagName,
    originalBody: `Original ${tagName}`,
    translatedBody: `Translated ${tagName}`,
    smartBody: `Smart ${tagName}`,
    htmlUrl: `https://github.com/acme/api/releases/tag/${tagName}`,
    draft: false,
    prerelease: false,
    publishedAt: '2026-07-18T10:00:00.000Z',
    createdAt: '2026-07-18T09:00:00.000Z',
  }
}

function makeReadyResponse(
  source: ServiceReleaseNotesResponse['source'],
  defaultView: ServiceReleaseNotesResponse['defaultView'],
): ServiceReleaseNotesResponse {
  return {
    status: 'ready',
    source,
    repo: { fullName: 'acme/api', htmlUrl: 'https://github.com/acme/api' },
    cursor: null,
    limit: 20,
    nextCursor: 'next-1',
    previousCursor: null,
    hasMore: true,
    defaultView,
    externalLinks: {
      githubReleasesUrl: 'https://github.com/acme/api/releases',
      octoRillReleasesUrl: 'https://octo.example.com/acme/api/releases',
    },
    items: [makeItem('v1.2.3')],
    message: null,
    stale: null,
    anchor: null,
  }
}

function makeFailureResponse(
  source: ServiceReleaseNotesResponse['source'],
  defaultView: ServiceReleaseNotesResponse['defaultView'],
  message: string,
): ServiceReleaseNotesResponse {
  return {
    status: 'upstreamError',
    source,
    repo: { fullName: 'acme/api', htmlUrl: 'https://github.com/acme/api' },
    cursor: null,
    limit: 20,
    nextCursor: null,
    previousCursor: null,
    hasMore: false,
    defaultView,
    externalLinks: {
      githubReleasesUrl: 'https://github.com/acme/api/releases',
      octoRillReleasesUrl: 'https://octo.example.com/acme/api/releases',
    },
    items: [],
    message,
    stale: null,
    anchor: {
      status: 'unavailable',
      version: 'v1.2.3',
      message,
    },
  }
}

afterEach(() => {
  __releaseNotesSessionTestUtils.resetReleaseNotesSnapshotCache()
})

describe('release notes session stale snapshots', () => {
  test('reuses the latest snapshot only for the same service and provider', () => {
    const ready = makeReadyResponse('octoRill', 'smart')
    __releaseNotesSessionTestUtils.cacheReleaseNotesSnapshot({
      serviceId: 'svc-api',
      response: ready,
      items: ready.items,
      olderCursor: 'next-1',
      newerCursor: null,
    })

    const stale = __releaseNotesSessionTestUtils.buildStaleSnapshotResponse(
      'svc-api',
      makeFailureResponse('octoRill', 'smart', 'OctoRill 暂时不可用。'),
    )

    expect(stale).not.toBeNull()
    expect(stale?.response.status).toBe('ready')
    expect(stale?.response.source).toBe('octoRill')
    expect(stale?.response.stale).toEqual({
      reason: 'requestFailed',
      message: 'OctoRill 暂时不可用。',
    })
    expect(stale?.items.map((item) => item.tagName)).toEqual(['v1.2.3'])
    expect(stale?.olderCursor).toBe('next-1')
  })

  test('does not reuse a cached snapshot across providers', () => {
    const ready = makeReadyResponse('octoRill', 'smart')
    __releaseNotesSessionTestUtils.cacheReleaseNotesSnapshot({
      serviceId: 'svc-api',
      response: ready,
      items: ready.items,
      olderCursor: 'next-1',
      newerCursor: null,
    })

    const stale = __releaseNotesSessionTestUtils.buildStaleSnapshotResponse(
      'svc-api',
      makeFailureResponse('gitHub', 'original', '读取 GitHub Releases 失败，请稍后重试。'),
    )

    expect(stale).toBeNull()
  })

  test('keeps GitHub stale snapshots pinned to the original view', () => {
    const ready = makeReadyResponse('gitHub', 'original')
    __releaseNotesSessionTestUtils.cacheReleaseNotesSnapshot({
      serviceId: 'svc-api',
      response: ready,
      items: ready.items,
      olderCursor: null,
      newerCursor: null,
    })

    const stale = __releaseNotesSessionTestUtils.buildStaleSnapshotResponse(
      'svc-api',
      makeFailureResponse('gitHub', 'original', '读取 GitHub Releases 失败，请稍后重试。'),
    )

    expect(stale?.response.source).toBe('gitHub')
    expect(stale?.response.defaultView).toBe('original')
    expect(stale?.response.items.map((item) => item.tagName)).toEqual(['v1.2.3'])
  })
})

describe('release notes refresh retry policy', () => {
  test('uses the server retry interval for active refreshes and caps backoff at one minute', () => {
    const queued = makeReadyResponse('octoRill', 'smart')
    queued.refresh = { state: 'queued', retryAfterSeconds: 2 }
    expect(__releaseNotesSessionTestUtils.releaseNotesRefreshRetryDelayMs(queued)).toBe(2_000)

    const backoff = makeReadyResponse('octoRill', 'smart')
    backoff.refresh = { state: 'backoff', retryAfterSeconds: 120 }
    expect(__releaseNotesSessionTestUtils.releaseNotesRefreshRetryDelayMs(backoff)).toBe(60_000)
  })

  test('does not schedule retries when refresh is fresh or absent', () => {
    const fresh = makeReadyResponse('octoRill', 'smart')
    fresh.refresh = { state: 'fresh' }
    expect(__releaseNotesSessionTestUtils.releaseNotesRefreshRetryDelayMs(fresh)).toBeNull()
    expect(__releaseNotesSessionTestUtils.releaseNotesRefreshRetryDelayMs(makeReadyResponse('gitHub', 'original'))).toBeNull()
  })
})
