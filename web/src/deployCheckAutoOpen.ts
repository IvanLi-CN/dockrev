const DATABASE_NAME = 'dockrev-deploy-check'
const DATABASE_VERSION = 1
const STORE_NAME = 'claims'

function deploymentKey(): string {
  if (typeof window === 'undefined') return 'server'
  const firstPathSegment = window.location.pathname.split('/').filter(Boolean)[0] ?? ''
  return `${window.location.origin}|${firstPathSegment}`
}

function openClaimsDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === 'undefined') {
      reject(new Error('indexedDB unavailable'))
      return
    }
    const request = indexedDB.open(DATABASE_NAME, DATABASE_VERSION)
    request.onupgradeneeded = () => {
      request.result.createObjectStore(STORE_NAME, { keyPath: 'key' })
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('indexedDB open failed'))
  })
}

export async function claimDeployCheckAutoOpen(failureKey: string): Promise<boolean> {
  if (!failureKey || typeof window === 'undefined') return false
  let database: IDBDatabase
  try {
    database = await openClaimsDatabase()
  } catch {
    return false
  }

  return new Promise((resolve) => {
    let settled = false
    const finish = (value: boolean) => {
      if (settled) return
      settled = true
      database.close()
      resolve(value)
    }
    const transaction = database.transaction(STORE_NAME, 'readwrite')
    const request = transaction.objectStore(STORE_NAME).add({
      key: `${deploymentKey()}|${failureKey}`,
      claimedAt: new Date().toISOString(),
    })
    request.onsuccess = () => finish(true)
    request.onerror = () => finish(request.error?.name === 'ConstraintError' ? false : false)
    transaction.onerror = () => finish(false)
    transaction.onabort = () => finish(false)
  })
}
