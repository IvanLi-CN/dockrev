import { afterEach, describe, expect, test } from 'bun:test'
import { getStacksOverview } from '../src/api'

const originalFetch = globalThis.fetch

afterEach(() => {
  globalThis.fetch = originalFetch
})

describe('candidate summary read model', () => {
  test('loads candidate rows through one overview request without stack-detail fan-out', async () => {
    const requested: string[] = []
    globalThis.fetch = async (input) => {
      requested.push(String(input))
      return Response.json({
        stacks: [{ id: 'stack-a', name: 'alpha', status: 'healthy', services: 1, updates: 1, lastCheckAt: '2026-08-20T00:00:00.000Z' }],
        details: [{ id: 'stack-a', name: 'alpha', services: [] }],
      })
    }

    const overview = await getStacksOverview()

    expect(overview.stacks).toHaveLength(1)
    expect(overview.details).toHaveLength(1)
    expect(requested).toEqual(['/api/stacks/overview'])
  })
})
