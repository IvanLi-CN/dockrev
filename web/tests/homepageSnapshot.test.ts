import { describe, expect, test } from 'bun:test'

import {
  HOMEPAGE_NAV_SNAPSHOT_KEY,
  HOMEPAGE_RESOURCE_SUMMARY_KEY,
  HOMEPAGE_SNAPSHOT_KEY,
  homepageSnapshotFromResponse,
  homepageSnapshotIsResourceStale,
  markHomepageSnapshotResourceStale,
  readHomepageSnapshot,
  writeHomepageSnapshot,
  type HomepageSnapshotCard,
} from '../src/pages/homepageSnapshot'
import type { HomepageNavResponse, ServiceResourceOverviewResponse } from '../src/api'

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

const card: HomepageSnapshotCard = {
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
  service: {
    id: 'svc-api',
    name: 'api',
    image: {
      ref: 'ghcr.io/acme/api:5.2.1',
      tag: '5.2.1',
      digest: 'sha256:api',
      resolvedTag: '5.2.1',
      resolvedTags: ['5.2.1'],
    },
    homepage: {
      group: 'Brain',
      name: 'Acme API',
      icon: 'si-github',
      href: 'https://api.example.com',
      description: 'API gateway',
    },
    candidate: {
      tag: '5.2.3',
      resolvedTag: '5.2.3',
      digest: 'sha256:candidate',
      archMatch: 'match',
      arch: ['linux/amd64'],
    },
    ignore: null,
    versionInference: {
      status: 'ready',
      reason: null,
      checkedAt: null,
    },
    newVersionDiscoveryCount: 1,
    settings: {
      autoRollback: true,
      backupTargets: { bindPaths: {}, volumeNames: {} },
      repoUrl: null,
    },
    archived: false,
  },
}

const homepageResponse: HomepageNavResponse = {
  generatedAt: '2026-05-07T00:00:00.000Z',
  lastCheckAt: '2026-05-07T00:00:00.000Z',
  resourceSummary: overview,
  items: [],
}

describe('homepage snapshot cache', () => {
  test('round-trips homepage snapshot v2', () => {
    const storage = new MemoryStorage()
    const snapshot = homepageSnapshotFromResponse({
      generatedAt: homepageResponse.generatedAt,
      lastCheckAt: homepageResponse.lastCheckAt,
      resourceSummary: overview,
      cards: [card],
    })

    writeHomepageSnapshot(snapshot, storage)

    const raw = JSON.parse(storage.getItem(HOMEPAGE_SNAPSHOT_KEY) ?? '{}')
    expect(raw.version).toBe(2)
    expect(raw.cards[0].service.image.ref).toBe('ghcr.io/acme/api:5.2.1')

    const parsed = readHomepageSnapshot(storage)
    expect(parsed?.cards).toEqual([card])
    expect(parsed?.resourceSummary.services[0].cpuPercent).toBe(12)
  })

  test('marks cached resource summary stale while preserving values', () => {
    const snapshot = homepageSnapshotFromResponse({
      generatedAt: '2026-05-07T00:00:00.000Z',
      lastCheckAt: null,
      resourceSummary: overview,
      cards: [card],
    })

    expect(
      homepageSnapshotIsResourceStale(snapshot, Date.parse('2026-05-07T00:02:01.000Z')),
    ).toBe(true)

    const stale = markHomepageSnapshotResourceStale(snapshot)
    expect(stale.resourceSummary.services[0].stale).toBe(true)
    expect(stale.resourceSummary.services[0].cpuPercent).toBe(12)
  })

  test('migrates legacy v1 nav/resource snapshots into v2 on read', () => {
    const storage = new MemoryStorage()
    storage.setItem(
      HOMEPAGE_NAV_SNAPSHOT_KEY,
      JSON.stringify({
        version: 1,
        generatedAt: '2026-05-07T00:00:00.000Z',
        cards: [
          {
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
          },
        ],
      }),
    )
    storage.setItem(
      HOMEPAGE_RESOURCE_SUMMARY_KEY,
      JSON.stringify({
        version: 1,
        generatedAt: '2026-05-07T00:00:00.000Z',
        overview,
      }),
    )

    const snapshot = readHomepageSnapshot(storage)
    expect(snapshot?.version).toBe(2)
    expect(snapshot?.cards[0]?.title).toBe('Acme API')
    expect(storage.getItem(HOMEPAGE_SNAPSHOT_KEY)).not.toBeNull()
  })
})
