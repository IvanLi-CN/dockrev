import { describe, expect, test } from 'bun:test'

import { ApiError, type Service } from '../src/api'
import {
  partitionAggregateUpdateServices,
  readUpdateGuardBlockedReason,
  resolveAggregateUpdateActionState,
} from '../src/aggregateUpdateGuard'

function makeService(overrides?: Partial<Service>): Service {
  return {
    id: 'svc-1',
    name: 'svc-1',
    image: {
      ref: 'ghcr.io/acme/demo:latest',
      tag: 'latest',
      digest: 'sha256:current',
      resolvedTag: 'v1.0.0',
      resolvedTags: ['v1.0.0'],
    },
    candidate: {
      tag: 'latest',
      resolvedTag: 'v1.1.0',
      digest: 'sha256:candidate',
      archMatch: 'match',
      arch: ['linux/amd64'],
    },
    ignore: null,
    versionInference: { status: 'ready', reason: null, checkedAt: null },
    settings: {
      autoRollback: true,
      backupTargets: {
        bindPaths: {},
        volumeNames: {},
      },
    },
    archived: false,
    ...overrides,
  }
}

describe('aggregateUpdateGuard', () => {
  test('disables aggregate apply when visible services include an apply guard blocker', () => {
    const guarded = makeService({
      id: 'svc-guarded',
      name: 'edge',
      updateGuard: {
        blocked: true,
        code: 'traefik_online_service_requires_manual_zero_downtime',
        reason: 'Traefik 在线服务需走手工零停机流程（blue/green）',
      },
    })
    const partition = partitionAggregateUpdateServices([guarded])

    expect(partition.guardedApplyBlocked).toHaveLength(1)
    expect(partition.actionable).toHaveLength(0)

    const action = resolveAggregateUpdateActionState(partition)
    expect(action.enabled).toBe(false)
    expect(action.title).toContain('手工零停机流程')
    expect(action.hint).toContain('手工零停机流程')
  })

  test('extracts guarded reason from update_guard_blocked api errors', () => {
    const err = new ApiError({
      status: 409,
      code: 'update_guard_blocked',
      message: 'Traefik 在线服务需走手工零停机流程（blue/green）',
    })

    expect(readUpdateGuardBlockedReason(err)).toContain('手工零停机流程')
    expect(
      readUpdateGuardBlockedReason(
        new ApiError({ status: 409, code: 'conflict', message: 'conflict' }),
      ),
    ).toBeNull()
  })
})
