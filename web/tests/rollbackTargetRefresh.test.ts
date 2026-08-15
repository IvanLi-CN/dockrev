import { describe, expect, test } from 'bun:test'

import {
  decideRollbackTargetDigestRetry,
  ROLLBACK_TARGET_DIGEST_RETRY_LIMIT,
  ROLLBACK_TARGET_DIGEST_RETRY_MS,
  retryRollbackTargetDigestMismatch,
} from '../src/pages/rollbackTargetRefresh'

describe('rollback target digest retry policy', () => {
  test('allows five same-generation retries at 250ms', () => {
    let retryCount = 0
    for (let attempt = 0; attempt < ROLLBACK_TARGET_DIGEST_RETRY_LIMIT; attempt += 1) {
      const decision = decideRollbackTargetDigestRetry(retryCount, 7, 7)
      expect(decision).toEqual({
        kind: 'retry',
        retryCount: retryCount + 1,
        delayMs: ROLLBACK_TARGET_DIGEST_RETRY_MS,
      })
      retryCount = decision.retryCount
    }
    expect(decideRollbackTargetDigestRetry(retryCount, 7, 7)).toEqual({ kind: 'exhausted' })
  })

  test('rejects a response from an older request generation before retrying', () => {
    expect(decideRollbackTargetDigestRetry(0, 7, 8)).toEqual({ kind: 'outdated' })
    expect(decideRollbackTargetDigestRetry(ROLLBACK_TARGET_DIGEST_RETRY_LIMIT, 7, 8)).toEqual({ kind: 'outdated' })
  })

  test('settles after stale responses and records the 250ms retry interval', async () => {
    const delays: number[] = []
    const responses = ['stale-2', 'valid']
    const result = await retryRollbackTargetDigestMismatch({
      initialTarget: 'stale-1',
      requestId: 7,
      currentRequestId: () => 7,
      validate: (target) => target === 'valid' ? 'matched' : 'digest_mismatch',
      fetchTarget: async () => responses.shift() ?? 'valid',
      sleep: async (delayMs) => { delays.push(delayMs) },
    })
    expect(result).toEqual({ kind: 'matched', target: 'valid', retryCount: 2 })
    expect(delays).toEqual([250, 250])
  })

  test('exhausts stale responses after five retries', async () => {
    let fetchCount = 0
    const result = await retryRollbackTargetDigestMismatch({
      initialTarget: 'stale',
      requestId: 7,
      currentRequestId: () => 7,
      validate: () => 'digest_mismatch',
      fetchTarget: async () => { fetchCount += 1; return 'stale' },
      sleep: async () => {},
    })
    expect(result).toEqual({ kind: 'exhausted', retryCount: ROLLBACK_TARGET_DIGEST_RETRY_LIMIT })
    expect(fetchCount).toBe(ROLLBACK_TARGET_DIGEST_RETRY_LIMIT)
  })

  test('returns a fetch failure without applying a stale response', async () => {
    const failure = new Error('rollback target unavailable')
    const result = await retryRollbackTargetDigestMismatch({
      initialTarget: 'stale',
      requestId: 7,
      currentRequestId: () => 7,
      validate: () => 'digest_mismatch',
      fetchTarget: async () => { throw failure },
      sleep: async () => {},
    })
    expect(result).toEqual({ kind: 'failed', error: failure, retryCount: 1 })
  })

  test('discards a fetch failure when the request generation changes in flight', async () => {
    let currentRequestId = 7
    let rejectFetch: ((error: unknown) => void) | undefined
    const resultPromise = retryRollbackTargetDigestMismatch({
      initialTarget: 'stale',
      requestId: 7,
      currentRequestId: () => currentRequestId,
      validate: () => 'digest_mismatch',
      fetchTarget: () => new Promise<string>((_, reject) => { rejectFetch = reject }),
      sleep: async () => undefined,
    })

    await new Promise<void>((resolve) => queueMicrotask(resolve))
    currentRequestId = 8
    rejectFetch?.(new Error('request failed'))

    expect(await resultPromise).toEqual({ kind: 'outdated', retryCount: 1 })
  })

  test('stops before fetching when the request generation changes during the delay', async () => {
    let currentRequestId = 7
    let fetchCount = 0
    const result = await retryRollbackTargetDigestMismatch({
      initialTarget: 'stale',
      requestId: 7,
      currentRequestId: () => currentRequestId,
      validate: () => 'digest_mismatch',
      fetchTarget: async () => { fetchCount += 1; return 'valid' },
      sleep: async () => { currentRequestId = 8 },
    })
    expect(result).toEqual({ kind: 'outdated', retryCount: 1 })
    expect(fetchCount).toBe(0)
  })
})
