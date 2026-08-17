import { describe, expect, test } from 'bun:test'

import { isCurrentHomepageRefresh } from '../src/pages/homepageRefreshState'

function mergeHomepageCards<T extends { serviceId: string; source: 'live' | 'snapshot' }>(
  previous: T[],
  incoming: T[],
): T[] {
  const previousByServiceId = new Map(
    previous.map((card) => [card.serviceId, card] as const),
  )
  return incoming.map((incomingCard) => {
    const existing = previousByServiceId.get(incomingCard.serviceId)
    if (!existing) return incomingCard
    return {
      ...existing,
      ...incomingCard,
      source: 'live',
    }
  })
}

function mergeHomepageCardList<T extends { serviceId: string; source: 'live' | 'snapshot' }>(
  previous: T[],
  incoming: T[],
): T[] {
  if (incoming.length === 0) return []
  if (previous.length === 0) return incoming
  return mergeHomepageCards(previous, incoming)
}

function balancedColumnCount(groupCount: number, requestedColumnCount: number): number {
  const safeColumnCount = Math.max(1, Math.min(4, Math.floor(requestedColumnCount)))
  const columns = Array.from({ length: safeColumnCount }, () => 0)
  for (let index = 0; index < groupCount; index += 1) {
    const targetIndex = columns.reduce(
      (bestIndex, weight, currentIndex) =>
        weight < columns[bestIndex] ? currentIndex : bestIndex,
      0,
    )
    columns[targetIndex] += 1
  }
  const populatedColumns = columns.filter((weight) => weight > 0)
  return populatedColumns.length > 0 ? populatedColumns.length : 1
}

function visibleCards<T>(
  liveLoaded: boolean,
  liveCards: T[],
  cachedCards: T[],
): T[] {
  if (liveLoaded) return liveCards
  if (liveCards.length > 0) return liveCards
  return cachedCards
}

describe('homepage refresh state', () => {
  test('does not apply a delayed older response after a newer refresh completes', async () => {
    let latestRequestId = 0
    let applied: string | null = null
    let resolveOlder!: (value: string) => void
    let resolveNewer!: (value: string) => void
    const older = new Promise<string>((resolve) => {
      resolveOlder = resolve
    })
    const newer = new Promise<string>((resolve) => {
      resolveNewer = resolve
    })

    const refresh = async (response: Promise<string>) => {
      const requestId = ++latestRequestId
      const value = await response
      if (isCurrentHomepageRefresh(requestId, latestRequestId)) {
        applied = value
      }
    }

    const olderRefresh = refresh(older)
    const newerRefresh = refresh(newer)
    resolveNewer('newer')
    await newerRefresh
    resolveOlder('older')
    await olderRefresh

    expect(applied).toBe('newer')
  })

  test('keeps cached cards before first live payload settles', () => {
    const cachedCards = [{ serviceId: 'svc-cached', source: 'snapshot' as const }]
    expect(visibleCards(false, [], cachedCards)).toEqual(cachedCards)
  })

  test('drops removed cards once the live payload settles empty', () => {
    const cachedCards = [{ serviceId: 'svc-cached', source: 'snapshot' as const }]
    const liveCards = mergeHomepageCardList(cachedCards, [])
    expect(liveCards).toEqual([])
    expect(visibleCards(true, liveCards, cachedCards)).toEqual([])
  })

  test('merges matching cards in place by service id', () => {
    const previous = [
      { serviceId: 'svc-api', source: 'snapshot' as const, title: 'Cached API' },
      { serviceId: 'svc-prom', source: 'snapshot' as const, title: 'Cached Prom' },
    ]
    const incoming = [
      { serviceId: 'svc-api', source: 'live' as const, title: 'Live API' },
      { serviceId: 'svc-loki', source: 'live' as const, title: 'Live Loki' },
    ]

    const merged = mergeHomepageCardList(previous, incoming)

    expect(merged).toEqual([
      { serviceId: 'svc-api', source: 'live', title: 'Live API' },
      { serviceId: 'svc-loki', source: 'live', title: 'Live Loki' },
    ])
  })

  test('keeps at least one homepage column when no groups render', () => {
    expect(balancedColumnCount(0, 4)).toBe(1)
    expect(balancedColumnCount(0, 1)).toBe(1)
  })
})
