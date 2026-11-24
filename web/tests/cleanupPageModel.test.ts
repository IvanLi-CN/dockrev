import { describe, expect, test } from 'bun:test'

import type { CleanupResourceItem, CleanupScanResponse } from '../src/api'
import {
  buildCleanupConfirmGroups,
  cleanupFailureBackoffMs,
  cleanupRefreshLeadMs,
  isSnapshotFresh,
  nextCleanupRefreshDelayMs,
  projectResponseForTarget,
  shouldStopCleanupRefresh,
  snapshotAgeNow,
  sumKnownReclaimableBytes,
} from '../src/pages/cleanupPageModel'

function resource(
  resourceId: string,
  kind: CleanupResourceItem['kind'],
  bytes: number | null,
): CleanupResourceItem {
  return {
    resourceId,
    kind,
    label: resourceId,
    reason: 'test candidate',
    minPreset: 'balanced',
    estimatedReclaimableBytes: bytes,
    estimateUnknown: bytes == null,
  }
}

function response(): CleanupScanResponse {
  return {
    status: 'ready',
    reason: 'confirm',
    preset: 'aggressive',
    scope: 'all',
    estimatedReclaimableBytes: 190,
    hasUnknownSize: true,
    stackGroups: [
      {
        stackId: 'stack-prod',
        stackName: 'prod',
        estimatedReclaimableBytes: 180,
        hasUnknownSize: true,
        stackOrphans: [resource('network', 'network', 0)],
        services: [
          {
            serviceId: 'svc-api',
            serviceName: 'api',
            estimatedReclaimableBytes: 150,
            hasUnknownSize: true,
            resources: [
              resource('image', 'image', 100),
              resource('volume', 'volume', null),
              resource('container', 'container', 50),
            ],
          },
        ],
      },
    ],
    unownedGroup: {
      title: '未归属资源',
      estimatedReclaimableBytes: 10,
      hasUnknownSize: false,
      resources: [resource('cache', 'builder_cache', 10)],
    },
  }
}

describe('buildCleanupConfirmGroups', () => {
  test('returns four resource groups with known subtotals and unknown counts', () => {
    const groups = buildCleanupConfirmGroups(response())

    expect(groups.map(({ key }) => key)).toEqual(['image', 'volume', 'container', 'other'])
    expect(groups.map(({ resources, bytes, unknownCount }) => [resources.length, bytes, unknownCount])).toEqual([
      [1, 100, 0],
      [1, 0, 1],
      [1, 50, 0],
      [2, 10, 0],
    ])
    expect(groups[0]?.resources[0]?.ownerLabel).toBe('prod / api')
    expect(groups[1]?.resources[0]?.ownerLabel).toBe('prod / api')
    expect(groups[3]?.resources.map(({ resource: item }) => item.kind)).toEqual(['network', 'builder_cache'])
  })

  test('does not mutate the response used by cleanup confirmation', () => {
    const input = response()
    const before = JSON.stringify(input)

    buildCleanupConfirmGroups(input)

    expect(JSON.stringify(input)).toBe(before)
    expect(input.scope).toBe('all')
  })
})

describe('cleanup snapshot scheduling', () => {
  test('ages a snapshot from the server-reported age plus local elapsed time', () => {
    expect(snapshotAgeNow({ snapshotAgeMs: 240_000 }, 1_000, 61_000)).toBe(300_000)
    expect(snapshotAgeNow({ snapshotAgeMs: 10_000 }, 1_000, 500)).toBe(10_000)
    expect(snapshotAgeNow({ snapshotAgeMs: null }, 1_000, 61_000)).toBeNull()
    expect(snapshotAgeNow({}, 1_000, 61_000)).toBeNull()
  })

  test('treats the freshness boundary as exclusive', () => {
    expect(isSnapshotFresh(299_999)).toBe(true)
    expect(isSnapshotFresh(300_000)).toBe(false)
    expect(isSnapshotFresh(null)).toBe(false)
  })

  test('derives the refresh lead from the last scan duration within 60-240 seconds', () => {
    expect(cleanupRefreshLeadMs(null)).toBe(60_000)
    expect(cleanupRefreshLeadMs(1_000)).toBe(60_000)
    expect(cleanupRefreshLeadMs(100_000)).toBe(115_000)
    expect(cleanupRefreshLeadMs(500_000)).toBe(240_000)
    expect(nextCleanupRefreshDelayMs(100_000, 100_000)).toBe(15_000)
    expect(nextCleanupRefreshDelayMs(200_000, 100_000)).toBe(0)
    expect(nextCleanupRefreshDelayMs(null, 100_000)).toBeNull()
  })

  test('backs off failures and stops after thirty minutes of inactivity', () => {
    expect([0, 1, 2, 3, 4, 5, 99].map(cleanupFailureBackoffMs)).toEqual([
      60_000,
      120_000,
      240_000,
      480_000,
      960_000,
      1_800_000,
      1_800_000,
    ])
    expect(shouldStopCleanupRefresh(0, 30 * 60_000 - 1)).toBe(false)
    expect(shouldStopCleanupRefresh(0, 30 * 60_000)).toBe(true)
  })

  test('projects a page snapshot onto a stack or service target', () => {
    const input = response()
    const stack = projectResponseForTarget(input, { scope: 'stack', stackId: 'stack-prod' })
    expect(stack.stackGroups).toHaveLength(1)
    expect(stack.stackGroups[0]?.stackId).toBe('stack-prod')
    expect(stack.estimatedReclaimableBytes).toBe(150)
    expect(stack.unownedGroup).toBeNull()
    expect(stack.confirmationFingerprint).toBeNull()

    const service = projectResponseForTarget(input, {
      scope: 'service',
      stackId: 'stack-prod',
      serviceId: 'svc-api',
    })
    expect(service.stackGroups[0]?.services).toHaveLength(1)
    expect(service.stackGroups[0]?.stackOrphans).toEqual([])
    expect(service.estimatedReclaimableBytes).toBe(150)
  })

  test('sums only known candidate bytes', () => {
    expect(sumKnownReclaimableBytes(response())).toBe(160)
  })
})
