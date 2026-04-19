import { describe, expect, test } from 'bun:test'

import type { StackDetail, StackListItem } from '../src/api'
import { resolveAggregateUpdateActionState } from '../src/aggregateUpdateGuard'
import { buildAllAggregateScope, buildStackAggregateScope } from '../src/pages/aggregateUpdateScope'

const digest = (fill: string) => `sha256:${fill.repeat(64).slice(0, 64)}`

;(globalThis as typeof globalThis & {
  window?: { __DOCKREV_CONFIG__?: { dockrevImageRepo?: string } }
}).window = {
  __DOCKREV_CONFIG__: {
    dockrevImageRepo: 'ghcr.io/ivanli-cn/dockrev',
  },
}

function makeService(overrides: Partial<StackDetail['services'][number]>): StackDetail['services'][number] {
  return {
    id: 'svc-default',
    name: 'default',
    image: {
      ref: 'ghcr.io/acme/default:latest',
      tag: 'latest',
      digest: digest('a'),
      resolvedTag: 'v1.0.0',
      resolvedTags: ['v1.0.0'],
    },
    candidate: {
      tag: 'latest',
      resolvedTag: 'v1.1.0',
      digest: digest('b'),
      archMatch: 'match',
      arch: ['linux/amd64'],
    },
    ignore: null,
    versionInference: { status: 'ready', reason: null, checkedAt: null },
    settings: {
      autoRollback: true,
      backupTargets: { bindPaths: {}, volumeNames: {} },
    },
    archived: false,
    ...overrides,
  }
}

describe('aggregateUpdateScope', () => {
  test('limits stack submission to the currently visible actionable set when search narrows rows', () => {
    const detail: StackDetail = {
      id: 'stack-prod',
      name: 'prod',
      compose: { type: 'path', composeFiles: ['/srv/prod/compose.yml'], envFile: null },
      services: [
        makeService({ id: 'svc-api', name: 'api', homepage: { name: 'Primary API', href: null, icon: null, description: 'Primary API' } }),
        makeService({ id: 'svc-web', name: 'web', homepage: { name: 'Console', href: null, icon: null, description: 'Admin UI' } }),
        makeService({
          id: 'svc-worker',
          name: 'worker',
          candidate: {
            tag: 'latest',
            resolvedTag: 'v1.1.0',
            digest: digest('c'),
            archMatch: 'match',
            arch: ['linux/amd64'],
          },
          ignore: { matched: true, ruleId: 'ignore-1', reason: 'blocked' },
        }),
      ],
    }

    const scope = buildStackAggregateScope(detail, 'all', 'Primary API')

    expect(scope.visibleServiceCount).toBe(1)
    expect(scope.actionableCount).toBe(1)
    expect(scope.previewItems).toHaveLength(1)
    expect(scope.actionableServices.map((service) => service.id)).toEqual(['svc-api'])
    expect(scope.counts.updatable).toBe(1)
    expect(scope.counts.blocked).toBe(0)
  })

  test('limits aggregate-all submission to the currently visible actionable set across stacks', () => {
    const stackA: StackDetail = {
      id: 'stack-a',
      name: 'prod',
      compose: { type: 'path', composeFiles: ['/srv/prod/compose.yml'], envFile: null },
      services: [
        makeService({ id: 'svc-api', name: 'api', homepage: { name: 'Primary API', href: null, icon: null, description: 'Primary API' } }),
        makeService({ id: 'svc-web', name: 'web' }),
      ],
    }
    const stackB: StackDetail = {
      id: 'stack-b',
      name: 'infra',
      compose: { type: 'path', composeFiles: ['/srv/infra/compose.yml'], envFile: null },
      services: [
        makeService({
          id: 'svc-loki',
          name: 'loki',
          candidate: {
            tag: 'latest',
            resolvedTag: 'v1.1.0',
            digest: digest('d'),
            archMatch: 'unknown',
            arch: ['linux/amd64', 'linux/arm64'],
          },
        }),
        makeService({
          id: 'svc-prom',
          name: 'prom',
          candidate: {
            tag: 'latest',
            resolvedTag: 'v1.1.0',
            digest: digest('e'),
            archMatch: 'mismatch',
            arch: ['linux/arm64'],
          },
        }),
      ],
    }
    const stacks: StackListItem[] = [
      { id: 'stack-a', name: 'prod', status: 'healthy', services: 2, updates: 2, lastCheckAt: null },
      { id: 'stack-b', name: 'infra', status: 'healthy', services: 2, updates: 1, lastCheckAt: null },
    ]

    const scope = buildAllAggregateScope({
      stacks,
      details: { 'stack-a': stackA, 'stack-b': stackB },
      filter: 'all',
      candidateSearch: 'Primary API',
    })

    expect(scope.visibleServiceCount).toBe(1)
    expect(scope.actionableCount).toBe(1)
    expect(scope.previewItems).toHaveLength(1)
    expect(scope.counts.updatable).toBe(1)
    expect(scope.counts.hint).toBe(0)
    expect(scope.counts.archMismatch).toBe(0)
  })

  test('keeps visible guarded services as apply blockers for aggregate scope', () => {
    const detail: StackDetail = {
      id: 'stack-prod',
      name: 'prod',
      compose: { type: 'path', composeFiles: ['/srv/prod/compose.yml'], envFile: null },
      services: [
        makeService({
          id: 'svc-edge',
          name: 'edge',
          updateGuard: {
            blocked: true,
            code: 'traefik_online_service_requires_manual_zero_downtime',
            reason: 'Traefik 在线服务需走手工零停机流程（blue/green）',
          },
        }),
      ],
    }

    const scope = buildStackAggregateScope(detail, 'all', '')

    expect(scope.guardedApplyBlocked).toHaveLength(1)
    expect(scope.counts.blocked).toBe(1)

    const action = resolveAggregateUpdateActionState({
      counts: scope.counts,
      guardedDockrevPreview: [],
      guardedApplyBlocked: scope.guardedApplyBlocked,
    })
    expect(action.enabled).toBe(false)
    expect(action.hint).toContain('手工零停机流程')
  })
})
