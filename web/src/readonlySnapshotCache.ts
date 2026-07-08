const DB_NAME = 'dockrev-readonly-snapshots'
const DB_VERSION = 1
const STORE_NAME = 'snapshots'
const SNAPSHOT_SCHEMA_VERSION = 1
export const READONLY_SNAPSHOT_EXPIRE_MS = 7 * 24 * 60 * 60 * 1000

export type ReadonlySnapshotRecord<T> = {
  key: string
  schemaVersion: number
  sourceVersion: string | null
  fetchedAt: string
  staleAt: string
  expireAt: string
  payload: T
}

export type ReadonlySnapshotState<T> =
  | {
      status: 'fresh' | 'stale'
      record: ReadonlySnapshotRecord<T>
    }
  | {
      status: 'expired'
      record: ReadonlySnapshotRecord<T>
    }
  | {
      status: 'missing' | 'unsupported'
      record: null
    }

function canUseIndexedDb(): boolean {
  return typeof window !== 'undefined' && typeof window.indexedDB !== 'undefined'
}

function openSnapshotsDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = window.indexedDB.open(DB_NAME, DB_VERSION)
    request.onerror = () => reject(request.error ?? new Error('failed to open IndexedDB'))
    request.onupgradeneeded = () => {
      const db = request.result
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME, { keyPath: 'key' })
      }
    }
    request.onsuccess = () => resolve(request.result)
  })
}

function readStoreRecord<T>(key: string): Promise<ReadonlySnapshotRecord<T> | null> {
  return new Promise((resolve, reject) => {
    void openSnapshotsDb()
      .then((db) => {
        const tx = db.transaction(STORE_NAME, 'readonly')
        const store = tx.objectStore(STORE_NAME)
        const request = store.get(key)
        request.onerror = () => reject(request.error ?? new Error(`failed to read snapshot: ${key}`))
        request.onsuccess = () => {
          resolve((request.result as ReadonlySnapshotRecord<T> | undefined) ?? null)
          db.close()
        }
      })
      .catch(reject)
  })
}

function writeStoreRecord<T>(record: ReadonlySnapshotRecord<T>): Promise<void> {
  return new Promise((resolve, reject) => {
    void openSnapshotsDb()
      .then((db) => {
        const tx = db.transaction(STORE_NAME, 'readwrite')
        const store = tx.objectStore(STORE_NAME)
        store.put(record)
        tx.oncomplete = () => {
          db.close()
          resolve()
        }
        tx.onerror = () => reject(tx.error ?? new Error(`failed to write snapshot: ${record.key}`))
        tx.onabort = () => reject(tx.error ?? new Error(`failed to write snapshot: ${record.key}`))
      })
      .catch(reject)
  })
}

function deleteStoreRecord(key: string): Promise<void> {
  return new Promise((resolve, reject) => {
    void openSnapshotsDb()
      .then((db) => {
        const tx = db.transaction(STORE_NAME, 'readwrite')
        tx.objectStore(STORE_NAME).delete(key)
        tx.oncomplete = () => {
          db.close()
          resolve()
        }
        tx.onerror = () => reject(tx.error ?? new Error(`failed to delete snapshot: ${key}`))
        tx.onabort = () => reject(tx.error ?? new Error(`failed to delete snapshot: ${key}`))
      })
      .catch(reject)
  })
}

function isValidRecord<T>(value: unknown): value is ReadonlySnapshotRecord<T> {
  if (typeof value !== 'object' || value === null) return false
  const record = value as Record<string, unknown>
  return (
    typeof record.key === 'string' &&
    typeof record.schemaVersion === 'number' &&
    (typeof record.sourceVersion === 'string' || record.sourceVersion === null) &&
    typeof record.fetchedAt === 'string' &&
    typeof record.staleAt === 'string' &&
    typeof record.expireAt === 'string' &&
    'payload' in record
  )
}

export function buildReadonlySnapshotKey(scope: string, id: string): string {
  return `readonly:${scope}:${id}`
}

export async function readReadonlySnapshot<T>(key: string, now = Date.now()): Promise<ReadonlySnapshotState<T>> {
  if (!canUseIndexedDb()) {
    return { status: 'unsupported', record: null }
  }
  try {
    const record = await readStoreRecord<T>(key)
    if (!record || !isValidRecord<T>(record) || record.schemaVersion !== SNAPSHOT_SCHEMA_VERSION) {
      return { status: 'missing', record: null }
    }
    const staleAtMs = Date.parse(record.staleAt)
    const expireAtMs = Date.parse(record.expireAt)
    if (!Number.isFinite(staleAtMs) || !Number.isFinite(expireAtMs)) {
      await deleteStoreRecord(key).catch(() => {})
      return { status: 'missing', record: null }
    }
    if (expireAtMs <= now) {
      return { status: 'expired', record }
    }
    if (staleAtMs <= now) {
      return { status: 'stale', record }
    }
    return { status: 'fresh', record }
  } catch {
    return { status: 'missing', record: null }
  }
}

export async function writeReadonlySnapshot<T>(
  key: string,
  payload: T,
  opts: {
    staleAfterMs: number
    expireAfterMs?: number
    fetchedAt?: number
    sourceVersion?: string | null
  },
): Promise<void> {
  if (!canUseIndexedDb()) return
  const fetchedAt = opts.fetchedAt ?? Date.now()
  const expireAfterMs = opts.expireAfterMs ?? READONLY_SNAPSHOT_EXPIRE_MS
  const record: ReadonlySnapshotRecord<T> = {
    key,
    schemaVersion: SNAPSHOT_SCHEMA_VERSION,
    sourceVersion: opts.sourceVersion ?? null,
    fetchedAt: new Date(fetchedAt).toISOString(),
    staleAt: new Date(fetchedAt + Math.max(0, opts.staleAfterMs)).toISOString(),
    expireAt: new Date(fetchedAt + Math.max(0, expireAfterMs)).toISOString(),
    payload,
  }
  try {
    await writeStoreRecord(record)
  } catch {
    // IndexedDB persistence is best-effort.
  }
}

export async function deleteReadonlySnapshot(key: string): Promise<void> {
  if (!canUseIndexedDb()) return
  try {
    await deleteStoreRecord(key)
  } catch {
    // best-effort cleanup only
  }
}

export function formatSnapshotTime(ts: string | null | undefined): string {
  const value = (ts ?? '').trim()
  if (!value) return '-'
  const date = new Date(value)
  if (Number.isNaN(date.valueOf())) return value
  return date.toLocaleString()
}
