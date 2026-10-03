import { describe, expect, test } from 'bun:test'

import type { CleanupResourceItem, CleanupScanResponse } from '../src/api'
import { buildCleanupConfirmGroups } from '../src/pages/cleanupPageModel'

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
