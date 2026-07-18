import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import {
  getServiceReleaseNotes,
  locateServiceReleaseNotes,
  type ReleaseNotesView,
  type ServiceReleaseNoteItem,
  type ServiceReleaseNotesResponse,
} from './api'
import { buildReleaseNotesFailureResponse, mergeReleaseNoteItems } from './releaseNotes'

type PageDirection = 'older' | 'newer'

type CachedReleaseNotesSnapshot = {
  response: ServiceReleaseNotesResponse
  items: ServiceReleaseNoteItem[]
  olderCursor: string | null
  newerCursor: string | null
}

const releaseNotesSnapshotCache = new Map<string, CachedReleaseNotesSnapshot>()

function snapshotCacheKey(serviceId: string, source: ServiceReleaseNotesResponse['source']): string {
  return `${serviceId}::${source}`
}

function cacheReleaseNotesSnapshot(input: {
  serviceId: string
  response: ServiceReleaseNotesResponse
  items: ServiceReleaseNoteItem[]
  olderCursor: string | null
  newerCursor: string | null
}) {
  releaseNotesSnapshotCache.set(snapshotCacheKey(input.serviceId, input.response.source), {
    response: {
      ...input.response,
      stale: null,
      nextCursor: input.olderCursor,
      previousCursor: input.newerCursor,
      hasMore: input.olderCursor != null,
    },
    items: input.items,
    olderCursor: input.olderCursor,
    newerCursor: input.newerCursor,
  })
}

function buildStaleSnapshotResponse(
  serviceId: string,
  failure: ServiceReleaseNotesResponse,
): CachedReleaseNotesSnapshot | null {
  const cached = releaseNotesSnapshotCache.get(snapshotCacheKey(serviceId, failure.source))
  if (!cached) return null
  const message = failure.message?.trim() || '当前仅显示该数据源最近一次成功结果。'
  return {
    response: {
      ...cached.response,
      status: 'ready',
      source: failure.source,
      repo: failure.repo ?? cached.response.repo,
      limit: failure.limit,
      defaultView: failure.defaultView,
      externalLinks: failure.externalLinks ?? cached.response.externalLinks,
      message: failure.message ?? cached.response.message,
      stale: {
        reason: 'requestFailed',
        message,
      },
      anchor: failure.anchor ?? cached.response.anchor,
    },
    items: cached.items,
    olderCursor: cached.olderCursor,
    newerCursor: cached.newerCursor,
  }
}

function resetReleaseNotesSnapshotCache() {
  releaseNotesSnapshotCache.clear()
}

export const __releaseNotesSessionTestUtils = {
  cacheReleaseNotesSnapshot,
  buildStaleSnapshotResponse,
  resetReleaseNotesSnapshotCache,
}

type UseServiceReleaseNotesSessionInput = {
  enabled: boolean
  serviceId: string | null
  targetVersion?: string | null
  locateTargetVersion: boolean
  limit: number
}

type UseServiceReleaseNotesSessionResult = {
  loadState: 'idle' | 'loading' | 'ready'
  response: ServiceReleaseNotesResponse | null
  items: ServiceReleaseNoteItem[]
  viewMode: ReleaseNotesView
  setViewMode: (view: ReleaseNotesView) => void
  loadingOlder: boolean
  loadingNewer: boolean
  olderFailure: ServiceReleaseNotesResponse | null
  newerFailure: ServiceReleaseNotesResponse | null
  hasOlder: boolean
  hasNewer: boolean
  loadOlder: () => Promise<void>
  loadNewer: () => Promise<void>
}

export function useServiceReleaseNotesSession(
  input: UseServiceReleaseNotesSessionInput,
): UseServiceReleaseNotesSessionResult {
  const activeSessionRef = useRef<string | null>(null)
  const inFlightPagesRef = useRef<Map<string, Promise<ServiceReleaseNotesResponse | null>>>(new Map())
  const responseRef = useRef<ServiceReleaseNotesResponse | null>(null)
  const olderCursorRef = useRef<string | null>(null)
  const newerCursorRef = useRef<string | null>(null)

  const [loadState, setLoadState] = useState<'idle' | 'loading' | 'ready'>('idle')
  const [response, setResponse] = useState<ServiceReleaseNotesResponse | null>(null)
  const [items, setItems] = useState<ServiceReleaseNoteItem[]>([])
  const [viewMode, setViewMode] = useState<ReleaseNotesView>('smart')
  const [olderCursor, setOlderCursor] = useState<string | null>(null)
  const [newerCursor, setNewerCursor] = useState<string | null>(null)
  const [loadingOlder, setLoadingOlder] = useState(false)
  const [loadingNewer, setLoadingNewer] = useState(false)
  const [olderFailure, setOlderFailure] = useState<ServiceReleaseNotesResponse | null>(null)
  const [newerFailure, setNewerFailure] = useState<ServiceReleaseNotesResponse | null>(null)

  useEffect(() => {
    responseRef.current = response
  }, [response])

  useEffect(() => {
    olderCursorRef.current = olderCursor
  }, [olderCursor])

  useEffect(() => {
    newerCursorRef.current = newerCursor
  }, [newerCursor])

  const sessionKey = useMemo(() => {
    if (!input.enabled || !input.serviceId) return null
    return `${input.serviceId}::${input.targetVersion?.trim() ?? ''}::${input.locateTargetVersion ? 'locate' : 'list'}`
  }, [input.enabled, input.locateTargetVersion, input.serviceId, input.targetVersion])

  const resetState = useCallback(() => {
    inFlightPagesRef.current.clear()
    setLoadState('idle')
    setResponse(null)
    setItems([])
    setViewMode('smart')
    setOlderCursor(null)
    setNewerCursor(null)
    setLoadingOlder(false)
    setLoadingNewer(false)
    setOlderFailure(null)
    setNewerFailure(null)
  }, [])

  const fetchDirectionPage = useCallback(
    async (expectedSession: string, direction: PageDirection, cursor: string): Promise<ServiceReleaseNotesResponse | null> => {
      if (!input.serviceId) return null
      const requestKey = `${expectedSession}:${direction}:${cursor}`
      const existing = inFlightPagesRef.current.get(requestKey)
      if (existing) return await existing

      const request = (async () => {
        let nextResponse: ServiceReleaseNotesResponse
        try {
          nextResponse = await getServiceReleaseNotes(input.serviceId!, {
            cursor,
            direction,
            limit: input.limit,
          })
        } catch (error) {
          nextResponse = buildReleaseNotesFailureResponse(error, cursor, input.limit)
        }
        if (activeSessionRef.current !== expectedSession) return null
        return nextResponse
      })()

      inFlightPagesRef.current.set(requestKey, request)
      try {
        return await request
      } finally {
        if (inFlightPagesRef.current.get(requestKey) === request) {
          inFlightPagesRef.current.delete(requestKey)
        }
      }
    },
    [input.limit, input.serviceId],
  )

  useEffect(() => {
    if (!input.enabled || !input.serviceId || !sessionKey) {
      activeSessionRef.current = null
      resetState()
      return
    }

    resetState()
    activeSessionRef.current = sessionKey
    setLoadState('loading')

    let cancelled = false

    void (async () => {
      let nextResponse: ServiceReleaseNotesResponse
      try {
        nextResponse =
          input.locateTargetVersion && input.targetVersion?.trim()
            ? await locateServiceReleaseNotes(input.serviceId!, {
                version: input.targetVersion.trim(),
                limit: input.limit,
              })
            : await getServiceReleaseNotes(input.serviceId!, { limit: input.limit })
      } catch (error) {
        nextResponse = buildReleaseNotesFailureResponse(error, null, input.limit)
      }

      if (cancelled || activeSessionRef.current !== sessionKey) return

      if (nextResponse.status !== 'ready') {
        const staleSnapshot = buildStaleSnapshotResponse(input.serviceId!, nextResponse)
        setResponse(staleSnapshot?.response ?? nextResponse)
        setLoadState('ready')
        if (staleSnapshot) {
          setItems(staleSnapshot.items)
          setOlderCursor(staleSnapshot.olderCursor)
          setNewerCursor(staleSnapshot.newerCursor)
          setViewMode(staleSnapshot.response.source === 'gitHub' ? 'original' : staleSnapshot.response.defaultView)
          return
        }
        setItems([])
        setOlderCursor(null)
        setNewerCursor(null)
        return
      }

      setResponse(nextResponse)
      setLoadState('ready')
      setItems(nextResponse.items)
      const nextOlderCursor = nextResponse.nextCursor?.trim() || null
      const nextNewerCursor = nextResponse.previousCursor?.trim() || null
      setOlderCursor(nextOlderCursor)
      setNewerCursor(nextNewerCursor)
      setViewMode(nextResponse.source === 'gitHub' ? 'original' : nextResponse.defaultView)
      cacheReleaseNotesSnapshot({
        serviceId: input.serviceId!,
        response: nextResponse,
        items: nextResponse.items,
        olderCursor: nextOlderCursor,
        newerCursor: nextNewerCursor,
      })
    })()

    return () => {
      cancelled = true
    }
  }, [
    input.enabled,
    input.limit,
    input.locateTargetVersion,
    input.serviceId,
    input.targetVersion,
    resetState,
    sessionKey,
  ])

  const loadOlder = useCallback(async () => {
    if (!sessionKey || !input.serviceId || loadingOlder || !olderCursor) return
    setLoadingOlder(true)
    try {
      const nextResponse = await fetchDirectionPage(sessionKey, 'older', olderCursor)
      if (!nextResponse || activeSessionRef.current !== sessionKey) return
      if (nextResponse.status !== 'ready') {
        setOlderFailure(nextResponse)
        setResponse((prev) =>
          prev
            ? {
                ...prev,
                stale: {
                  reason: 'requestFailed',
                  message: nextResponse.message?.trim() || '当前仅显示该数据源最近一次成功结果。',
                },
                message: nextResponse.message ?? prev.message,
                anchor: nextResponse.anchor ?? prev.anchor,
              }
            : prev,
        )
        return
      }
      setItems((prev) => {
        const merged = mergeReleaseNoteItems(prev, nextResponse.items, 'older')
        const nextOlderCursor = nextResponse.nextCursor?.trim() || null
        const cachedResponse = responseRef.current ?? nextResponse
        cacheReleaseNotesSnapshot({
          serviceId: input.serviceId!,
          response: {
            ...cachedResponse,
            stale: null,
            message: cachedResponse.stale ? null : cachedResponse.message ?? null,
          },
          items: merged,
          olderCursor: nextOlderCursor,
          newerCursor: newerCursorRef.current,
        })
        return merged
      })
      setOlderCursor(nextResponse.nextCursor?.trim() || null)
      setOlderFailure(null)
      setResponse((prev) =>
        prev
          ? {
              ...prev,
              stale: null,
              message: prev.stale ? null : prev.message,
            }
          : prev,
      )
    } finally {
      if (activeSessionRef.current === sessionKey) {
        setLoadingOlder(false)
      }
    }
  }, [fetchDirectionPage, input.serviceId, loadingOlder, olderCursor, sessionKey])

  const loadNewer = useCallback(async () => {
    if (!sessionKey || !input.serviceId || loadingNewer || !newerCursor) return
    setLoadingNewer(true)
    try {
      const nextResponse = await fetchDirectionPage(sessionKey, 'newer', newerCursor)
      if (!nextResponse || activeSessionRef.current !== sessionKey) return
      if (nextResponse.status !== 'ready') {
        setNewerFailure(nextResponse)
        setResponse((prev) =>
          prev
            ? {
                ...prev,
                stale: {
                  reason: 'requestFailed',
                  message: nextResponse.message?.trim() || '当前仅显示该数据源最近一次成功结果。',
                },
                message: nextResponse.message ?? prev.message,
                anchor: nextResponse.anchor ?? prev.anchor,
              }
            : prev,
        )
        return
      }
      setItems((prev) => {
        const merged = mergeReleaseNoteItems(prev, nextResponse.items, 'newer')
        const nextNewerCursor = nextResponse.previousCursor?.trim() || null
        const cachedResponse = responseRef.current ?? nextResponse
        cacheReleaseNotesSnapshot({
          serviceId: input.serviceId!,
          response: {
            ...cachedResponse,
            stale: null,
            message: cachedResponse.stale ? null : cachedResponse.message ?? null,
          },
          items: merged,
          olderCursor: olderCursorRef.current,
          newerCursor: nextNewerCursor,
        })
        return merged
      })
      setNewerCursor(nextResponse.previousCursor?.trim() || null)
      setNewerFailure(null)
      setResponse((prev) =>
        prev
          ? {
              ...prev,
              stale: null,
              message: prev.stale ? null : prev.message,
            }
          : prev,
      )
    } finally {
      if (activeSessionRef.current === sessionKey) {
        setLoadingNewer(false)
      }
    }
  }, [fetchDirectionPage, input.serviceId, loadingNewer, newerCursor, sessionKey])

  const effectiveResponse = useMemo<ServiceReleaseNotesResponse | null>(() => {
    if (!response) return null
    return {
      ...response,
      nextCursor: olderCursor,
      previousCursor: newerCursor,
      hasMore: olderCursor != null,
    }
  }, [newerCursor, olderCursor, response])

  return {
    loadState,
    response: effectiveResponse,
    items,
    viewMode,
    setViewMode,
    loadingOlder,
    loadingNewer,
    olderFailure,
    newerFailure,
    hasOlder: olderCursor != null,
    hasNewer: newerCursor != null,
    loadOlder,
    loadNewer,
  }
}
