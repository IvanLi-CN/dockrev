import { describe, expect, test } from 'bun:test'

import type { ServiceGitHubReleaseLocateResponse, ServiceGitHubReleasesResponse } from '../src/api'
import {
  applyLocatePreloadFailure,
  buildReleaseLocateFailureFromListResponse,
  buildReleaseLocateNotFoundResponse,
  RELEASE_DRAWER_LOCATE_LIMIT,
  shouldContinueReleaseLocateSearch,
} from '../src/githubReleaseDrawerState'
import { shouldResetReleaseDrawerOnRouteChange } from '../src/releaseDrawer'

describe('release drawer route transitions', () => {
  test('closes an open drawer when hash-routed navigation switches pages', () => {
    expect(
      shouldResetReleaseDrawerOnRouteChange({
        drawerOpen: true,
        hashRouting: true,
        previousPathname: '/services',
        nextPathname: '/settings',
      }),
    ).toBe(true)
  })

  test('keeps the drawer state for normal routing page changes', () => {
    expect(
      shouldResetReleaseDrawerOnRouteChange({
        drawerOpen: true,
        hashRouting: false,
        previousPathname: '/services',
        nextPathname: '/settings',
      }),
    ).toBe(false)
  })

  test('keeps the drawer when the hash route pathname does not change', () => {
    expect(
      shouldResetReleaseDrawerOnRouteChange({
        drawerOpen: true,
        hashRouting: true,
        previousPathname: '/services',
        nextPathname: '/services',
      }),
    ).toBe(false)
  })
})

describe('release drawer locate preload failures', () => {
  test('downgrades a found locate result when loading the target page fails', () => {
    const locateResponse: ServiceGitHubReleaseLocateResponse = {
      status: 'found',
      authMode: 'anonymous',
      repo: null,
      version: '1.39.5',
      searchedCount: 20,
      matchedTag: '1.39.5',
      page: 2,
      indexWithinPage: 4,
      absoluteIndex: 24,
      message: null,
    }
    const preloadFailure: ServiceGitHubReleasesResponse = {
      status: 'rateLimited',
      authMode: 'anonymous',
      repo: null,
      page: 2,
      perPage: 20,
      hasMore: true,
      items: [],
      message: 'GitHub 匿名访问已触发 rate limit。请稍后再试。',
    }

    expect(applyLocatePreloadFailure(locateResponse, preloadFailure)).toEqual({
      ...locateResponse,
      status: 'rateLimited',
      message: '已定位到 1.39.5，但加载对应发布页失败：GitHub 匿名访问已触发 rate limit。请稍后再试。',
    })
  })
})

describe('release drawer locate bounds', () => {
  test('continues paging only while still inside the search window', () => {
    const listResponse: ServiceGitHubReleasesResponse = {
      status: 'ready',
      authMode: 'anonymous',
      repo: null,
      page: 2,
      perPage: 20,
      hasMore: true,
      items: [],
      message: null,
    }

    expect(shouldContinueReleaseLocateSearch(listResponse, 40, RELEASE_DRAWER_LOCATE_LIMIT)).toBe(true)
    expect(shouldContinueReleaseLocateSearch(listResponse, 50, RELEASE_DRAWER_LOCATE_LIMIT)).toBe(false)
  })

  test('returns outsideWindow once the bounded search budget is exhausted', () => {
    const listResponse: ServiceGitHubReleasesResponse = {
      status: 'ready',
      authMode: 'anonymous',
      repo: null,
      page: 3,
      perPage: 20,
      hasMore: true,
      items: [],
      message: null,
    }

    expect(buildReleaseLocateNotFoundResponse(listResponse, '1.39.5', 50, RELEASE_DRAWER_LOCATE_LIMIT)).toEqual({
      status: 'outsideWindow',
      authMode: 'anonymous',
      repo: null,
      version: '1.39.5',
      searchedCount: 50,
      matchedTag: '1.39.5',
      page: null,
      indexWithinPage: null,
      absoluteIndex: null,
      message: '已定位到 1.39.5，但它不在前 50 条发布记录内。',
    })
  })

  test('returns notFound when the repo exhausts before the bounded search limit', () => {
    const listResponse: ServiceGitHubReleasesResponse = {
      status: 'ready',
      authMode: 'anonymous',
      repo: null,
      page: 2,
      perPage: 20,
      hasMore: false,
      items: [],
      message: null,
    }

    expect(buildReleaseLocateNotFoundResponse(listResponse, '9.9.9', 32, RELEASE_DRAWER_LOCATE_LIMIT)).toEqual({
      status: 'notFound',
      authMode: 'anonymous',
      repo: null,
      version: '9.9.9',
      searchedCount: 32,
      matchedTag: null,
      page: null,
      indexWithinPage: null,
      absoluteIndex: null,
      message: '在前 32 条发布记录中未找到 9.9.9。',
    })
  })

  test('preserves first-page permission failure for locate banner state', () => {
    const listResponse: ServiceGitHubReleasesResponse = {
      status: 'permissionDenied',
      authMode: 'anonymous',
      repo: null,
      page: 1,
      perPage: 20,
      hasMore: false,
      items: [],
      message: '需要 GitHub PAT 才能读取这个私有仓库。',
    }

    expect(buildReleaseLocateFailureFromListResponse(listResponse, '1.39.5')).toEqual({
      status: 'permissionDenied',
      authMode: 'anonymous',
      repo: null,
      version: '1.39.5',
      searchedCount: 0,
      matchedTag: null,
      page: null,
      indexWithinPage: null,
      absoluteIndex: null,
      message: '需要 GitHub PAT 才能读取这个私有仓库。',
    })
  })
})
