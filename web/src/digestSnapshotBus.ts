type DigestKey = string

type Listener = (token: number) => void

// In-memory event bus to coordinate digest snapshot cache invalidation between multiple
// popover instances that refer to the same `{serviceId}:{digest}` snapshot key.
const tokensByKey = new Map<DigestKey, number>()
const listenersByKey = new Map<DigestKey, Set<Listener>>()

export function getDigestSnapshotInvalidationToken(key: DigestKey): number {
  return tokensByKey.get(key) ?? 0
}

export function invalidateDigestSnapshot(key: DigestKey): number {
  const next = (tokensByKey.get(key) ?? 0) + 1
  tokensByKey.set(key, next)

  const listeners = listenersByKey.get(key)
  if (!listeners) return next

  for (const listener of Array.from(listeners)) {
    try {
      listener(next)
    } catch {
      // Best-effort only: one bad listener should not break others.
    }
  }

  return next
}

export function subscribeDigestSnapshotInvalidation(
  key: DigestKey,
  listener: Listener,
): () => void {
  const set = listenersByKey.get(key) ?? new Set<Listener>()
  set.add(listener)
  listenersByKey.set(key, set)

  return () => {
    const existing = listenersByKey.get(key)
    if (!existing) return
    existing.delete(listener)
    if (existing.size === 0) {
      listenersByKey.delete(key)
      tokensByKey.delete(key)
    }
  }
}
