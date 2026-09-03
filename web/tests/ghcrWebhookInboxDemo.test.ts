import { expect, test } from 'bun:test'

import { handleGhcrRoutes } from '../src/stories/mocks/dockrevMockApi/handlers/ghcr'
import type { MockRouteContext } from '../src/stories/mocks/dockrevMockApi/context'

function mockContext(path: string): MockRouteContext {
  const url = new URL(path, 'http://mock.local')
  return {
    method: 'GET',
    url,
    urlPath: url.pathname,
    urlPathWithQuery: `${url.pathname}${url.search}`,
    urlString: url.toString(),
    json: (data, init) => new Response(JSON.stringify(data), init),
  } as MockRouteContext
}

test('GHCR inbox demo returns paginated delivery data with filters', async () => {
  const allResponse = await handleGhcrRoutes(
    mockContext('/api/github-packages/webhook/deliveries?page=1&perPage=2'),
  )
  expect(allResponse).not.toBeNull()
  const all = await allResponse!.json()
  expect(all).toMatchObject({
    page: 1,
    perPage: 2,
    total: 3,
    filteredTotal: 3,
    summary: { processed: 1, ignored: 1, rejected: 1 },
  })
  expect(all.deliveries.map((delivery: { deliveryId: string }) => delivery.deliveryId)).toEqual([
    'delivery-demo-003',
    'delivery-demo-002',
  ])

  const filteredResponse = await handleGhcrRoutes(
    mockContext('/api/github-packages/webhook/deliveries?decision=ignored&q=octo-rill'),
  )
  expect(filteredResponse).not.toBeNull()
  const filtered = await filteredResponse!.json()
  expect(filtered).toMatchObject({ filteredTotal: 1 })
  expect(filtered.deliveries[0]).toMatchObject({
    deliveryId: 'delivery-demo-002',
    decision: 'ignored',
  })
})
