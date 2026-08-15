export const ROLLBACK_TARGET_DIGEST_RETRY_LIMIT = 5
export const ROLLBACK_TARGET_DIGEST_RETRY_MS = 250

export type RollbackTargetRetryDecision =
  | { kind: 'retry'; retryCount: number; delayMs: number }
  | { kind: 'exhausted' }
  | { kind: 'outdated' }

export type RollbackTargetDigestRetryValidation = 'matched' | 'digest_mismatch' | 'outdated'

export type RollbackTargetDigestRetryOutcome<T> =
  | { kind: 'matched'; target: T; retryCount: number }
  | { kind: 'exhausted'; retryCount: number }
  | { kind: 'outdated'; retryCount: number }
  | { kind: 'failed'; error: unknown; retryCount: number }

export function decideRollbackTargetDigestRetry(
  retryCount: number,
  requestId: number,
  currentRequestId: number,
): RollbackTargetRetryDecision {
  if (requestId !== currentRequestId) return { kind: 'outdated' }
  if (retryCount >= ROLLBACK_TARGET_DIGEST_RETRY_LIMIT) return { kind: 'exhausted' }
  return {
    kind: 'retry',
    retryCount: retryCount + 1,
    delayMs: ROLLBACK_TARGET_DIGEST_RETRY_MS,
  }
}

export async function retryRollbackTargetDigestMismatch<T>(options: {
  initialTarget: T
  requestId: number
  currentRequestId: () => number
  validate: (target: T) => RollbackTargetDigestRetryValidation
  fetchTarget: () => Promise<T>
  sleep: (delayMs: number) => Promise<void>
}): Promise<RollbackTargetDigestRetryOutcome<T>> {
  let target = options.initialTarget
  let retryCount = 0

  for (;;) {
    if (options.requestId !== options.currentRequestId()) return { kind: 'outdated', retryCount }
    const validation = options.validate(target)
    if (validation === 'matched') return { kind: 'matched', target, retryCount }
    if (validation === 'outdated') return { kind: 'outdated', retryCount }

    const decision = decideRollbackTargetDigestRetry(retryCount, options.requestId, options.currentRequestId())
    if (decision.kind === 'outdated') return { kind: 'outdated', retryCount }
    if (decision.kind === 'exhausted') return { kind: 'exhausted', retryCount }

    retryCount = decision.retryCount
    await options.sleep(decision.delayMs)
    if (options.requestId !== options.currentRequestId()) return { kind: 'outdated', retryCount }
    try {
      target = await options.fetchTarget()
    } catch (error: unknown) {
      if (options.requestId !== options.currentRequestId()) return { kind: 'outdated', retryCount }
      return { kind: 'failed', error, retryCount }
    }
  }
}
