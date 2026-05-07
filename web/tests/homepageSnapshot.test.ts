import { describe, expect, test } from 'bun:test'

import {
  HOMEPAGE_NAV_SNAPSHOT_KEY,
  HOMEPAGE_RESOURCE_SUMMARY_KEY,
  markResourceOverviewStale,
  readHomepageNavSnapshot,
  readHomepageResourceSummarySnapshot,
  resourceSummarySnapshotIsStale,
  writeHomepageNavSnapshot,
  writeHomepageResourceSummarySnapshot,
  type HomepageNavCardSnapshotItem,
} from '../src/pages/homepageSnapshot'
import type { ServiceResourceOverviewResponse } from '../src/api'

class MemoryStorage {
  private values = new Map<string, string>()

  getItem(key: string) {
    return this.values.get(key) ?? null
  }

  setItem(key: string, value: string) {
    this.values.set(key, value)
  }

  removeItem(key: string) {
    this.values.delete(key)
  }
}

const card: HomepageNavCardSnapshotItem = {
  id: 'svc-api',
  stackId: 'stack-prod',
  stackName: 'prod',
  serviceId: 'svc-api',
  serviceName: 'api',
  imageRef: 'ghcr.io/acme/api:5.2.1',
  groupName: 'Brain',
  title: 'Acme API',
  description: 'API gateway',
  href: 'https://api.example.com',
  icon: 'si-github',
  status: 'updatable',
  isDockrev: false,
}

const overview: ServiceResourceOverviewResponse = {
  enabled: true,
  window: '1h',
  generatedAt: '2026-05-07T00:00:00.000Z',
  staleAfterSeconds: 60,
  services: [
    {
      serviceId: 'svc-api',
      sampledAt: '2026-05-07T00:00:00.000Z',
      cpuPercent: 12,
      memUsedBytes: 128,
      memLimitBytes: 256,
      netRxRateBps: 4,
      netTxRateBps: 8,
      stale: false,
      sampleCount: 2,
    },
  ],
}

describe('homepage snapshot cache', () => {
  test('round-trips normalized navigation cards without full stack detail', () => {
    const storage = new MemoryStorage()

    writeHomepageNavSnapshot([card], storage, '2026-05-07T00:00:00.000Z')

    const raw = JSON.parse(storage.getItem(HOMEPAGE_NAV_SNAPSHOT_KEY) ?? '{}')
    expect(raw.cards[0].service).toBeUndefined()
    expect(raw.cards[0].compose).toBeUndefined()

    const snapshot = readHomepageNavSnapshot(storage)
    expect(snapshot?.cards).toEqual([card])
  })

  test('drops invalid navigation snapshots instead of rendering corrupt entries', () => {
    const storage = new MemoryStorage()
    storage.setItem(
      HOMEPAGE_NAV_SNAPSHOT_KEY,
      JSON.stringify({
        version: 1,
        generatedAt: '2026-05-07T00:00:00.000Z',
        cards: [{ ...card, status: 'ignored' }],
      }),
    )

    expect(readHomepageNavSnapshot(storage)).toBeNull()
  })

  test('round-trips resource summaries and detects stale cached samples', () => {
    const storage = new MemoryStorage()

    writeHomepageResourceSummarySnapshot(
      overview,
      storage,
      '2026-05-07T00:00:00.000Z',
    )

    const snapshot = readHomepageResourceSummarySnapshot(storage)
    expect(snapshot?.overview.services[0].cpuPercent).toBe(12)
    expect(
      resourceSummarySnapshotIsStale(
        snapshot!,
        Date.parse('2026-05-07T00:02:01.000Z'),
      ),
    ).toBe(true)
  })

  test('marks cached resource overview services stale while preserving values', () => {
    const stale = markResourceOverviewStale(overview)

    expect(stale.services[0].stale).toBe(true)
    expect(stale.services[0].cpuPercent).toBe(12)
    expect(stale.services[0].netTxRateBps).toBe(8)
    expect(stale).not.toBe(overview)
  })

  test('drops invalid resource summary snapshots', () => {
    const storage = new MemoryStorage()
    storage.setItem(
      HOMEPAGE_RESOURCE_SUMMARY_KEY,
      JSON.stringify({
        version: 2,
        generatedAt: '2026-05-07T00:00:00.000Z',
        overview,
      }),
    )

    expect(readHomepageResourceSummarySnapshot(storage)).toBeNull()
  })
})
