import { describe, expect, test } from 'bun:test'

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
})
