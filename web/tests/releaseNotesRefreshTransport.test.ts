import { afterEach, describe, expect, test } from 'bun:test'

import { getServiceReleaseNotes, locateServiceReleaseNotes } from '../src/api'

const originalFetch = globalThis.fetch

afterEach(() => {
  globalThis.fetch = originalFetch
})

function installResponseCapture() {
  const requested: string[] = []
  globalThis.fetch = async (input) => {
    requested.push(String(input))
    return Response.json({
      status: 'ready',
      source: 'octoRill',
      cursor: null,
      limit: 20,
      nextCursor: null,
      previousCursor: null,
      hasMore: false,
      defaultView: 'smart',
      items: [],
    })
  }
  return requested
}

describe('release-notes refresh intent', () => {
  test('adds refresh intent only to an initial list window', async () => {
    const requested = installResponseCapture()

    await getServiceReleaseNotes('svc-api', { limit: 20, refresh: 'if_stale' })
    await getServiceReleaseNotes('svc-api', { cursor: 'octo:page-2', limit: 20, refresh: 'if_stale' })

    expect(requested[0]).toContain('refresh=if_stale')
    expect(requested[1]).not.toContain('refresh=if_stale')
  })

  test('adds refresh intent to the initial locate request', async () => {
    const requested = installResponseCapture()

    await locateServiceReleaseNotes('svc-api', { version: 'v1.2.3', limit: 20, refresh: 'if_stale' })

    expect(requested[0]).toContain('refresh=if_stale')
    expect(requested[0]).toContain('version=v1.2.3')
  })
})
