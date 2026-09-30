import { describe, expect, test } from 'bun:test'

import { handleVersionUpdateRoutes } from '../src/stories/mocks/dockrevMockApi/handlers/versionUpdates'
import type { MockRouteContext } from '../src/stories/mocks/dockrevMockApi/context'

function previewResponse(releaseTag: string, submit = false): Response {
  const service = {
    stack: { id: 'stack-prod' },
    svc: {
      id: 'svc-prod-api',
      image: {
        ref: 'ghcr.io/acme/api:latest',
        tag: 'latest',
        digest: `sha256:${'1'.repeat(64)}`,
        resolvedTag: '5.2.2',
      },
    },
  }
  const body = submit
    ? { releaseTag, classification: 'forced', targetDigest: 'unused', backupMode: 'inherit' }
    : { releaseTag }
  const context = {
    scenario: 'service-selected-version-updates',
    method: 'POST',
    urlPath: submit
      ? '/api/services/svc-prod-api/version-update'
      : '/api/services/svc-prod-api/version-update/preview',
    init: { body: JSON.stringify(body) },
    nowIso: () => '2026-09-30T00:00:00.000Z',
    findService: () => service,
    json: (data: unknown, init?: ResponseInit) =>
      new Response(JSON.stringify(data), {
        headers: { 'content-type': 'application/json' },
        ...init,
      }),
    parseJsonBody: (value: unknown) => JSON.parse(String(value)),
    getString: (value: unknown) => (typeof value === 'string' ? value : null),
    getBoolean: (value: unknown) => (typeof value === 'boolean' ? value : null),
    isRecord: (value: unknown): value is Record<string, unknown> =>
      typeof value === 'object' && value !== null,
  } as unknown as MockRouteContext

  return handleVersionUpdateRoutes(context)!
}

describe('selected version update mock contract', () => {
  test('classifies observed and unknown newer releases', async () => {
    const normal = await previewResponse('v5.2.3').json()
    const forced = await previewResponse('v5.2.4').json()

    expect(normal.classification).toBe('normal')
    expect(forced.classification).toBe('forced')
  })

  test.each(['5.2.1', '5.2.2', '5.2.3-foo'])(
    'rejects older, equal, and incomparable release tags during preview: %s',
    async (releaseTag) => {
      const response = previewResponse(releaseTag)
      const body = await response.json()

      expect(response.status).toBe(400)
      expect(body.error).toMatchObject({ code: 'invalid_argument' })
    },
  )

  test('rejects invalid release tags again at submission', async () => {
    const response = previewResponse('5.2.3-foo', true)
    const body = await response.json()

    expect(response.status).toBe(400)
    expect(body.error).toMatchObject({ code: 'invalid_argument' })
  })
})
