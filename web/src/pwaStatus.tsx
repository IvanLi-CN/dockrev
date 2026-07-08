import { createContext, useContext, useEffect, useMemo, useRef, useState, type PropsWithChildren } from 'react'
import { useRegisterSW } from 'virtual:pwa-register/react'

const UPDATE_CHECK_INTERVAL_MS = 60 * 60 * 1000

export type PwaStatusContextValue = {
  isOnline: boolean
  offlineReady: boolean
  updateAvailable: boolean
  dismissOfflineReady: () => void
  dismissUpdate: () => void
  applyUpdate: () => Promise<void>
  checkForUpdates: () => Promise<void>
}

const PwaStatusContext = createContext<PwaStatusContextValue | null>(null)

function buildPwaStatusValue(
  overrides?: Partial<PwaStatusContextValue>,
): PwaStatusContextValue {
  return {
    isOnline: true,
    offlineReady: false,
    updateAvailable: false,
    dismissOfflineReady: () => {},
    dismissUpdate: () => {},
    applyUpdate: async () => {},
    checkForUpdates: async () => {},
    ...overrides,
  }
}

function readOnlineStatus(): boolean {
  if (typeof navigator === 'undefined') return true
  return navigator.onLine
}

export function PwaStatusProvider(props: PropsWithChildren) {
  const registrationRef = useRef<ServiceWorkerRegistration | null>(null)
  const [isOnline, setIsOnline] = useState(readOnlineStatus)
  const {
    offlineReady: [offlineReady, setOfflineReady],
    needRefresh: [needRefresh, setNeedRefresh],
    updateServiceWorker,
  } = useRegisterSW({
    onRegistered(registration) {
      registrationRef.current = registration ?? null
    },
    onRegisterError(error) {
      console.error('[dockrev] service worker registration failed', error)
    },
  })

  const checkForUpdates = useMemo(
    () => async () => {
      if (typeof document !== 'undefined' && document.visibilityState !== 'visible') return
      const registration = registrationRef.current
      if (!registration) return
      try {
        await registration.update()
      } catch (error) {
        console.warn('[dockrev] service worker update check failed', error)
      }
    },
    [],
  )

  useEffect(() => {
    if (typeof window === 'undefined') return
    const onOnline = () => setIsOnline(true)
    const onOffline = () => setIsOnline(false)
    const onVisible = () => {
      if (document.visibilityState === 'visible') void checkForUpdates()
    }
    const onFocus = () => void checkForUpdates()
    const onPageShow = (event: PageTransitionEvent) => {
      if (event.persisted) void checkForUpdates()
    }

    const timer = window.setInterval(() => {
      if (document.visibilityState !== 'visible') return
      void checkForUpdates()
    }, UPDATE_CHECK_INTERVAL_MS)

    window.addEventListener('online', onOnline)
    window.addEventListener('offline', onOffline)
    window.addEventListener('focus', onFocus)
    window.addEventListener('pageshow', onPageShow)
    document.addEventListener('visibilitychange', onVisible)

    if (document.visibilityState === 'visible') void checkForUpdates()

    return () => {
      window.clearInterval(timer)
      window.removeEventListener('online', onOnline)
      window.removeEventListener('offline', onOffline)
      window.removeEventListener('focus', onFocus)
      window.removeEventListener('pageshow', onPageShow)
      document.removeEventListener('visibilitychange', onVisible)
    }
  }, [checkForUpdates])

  const value = useMemo<PwaStatusContextValue>(
    () => ({
      isOnline,
      offlineReady,
      updateAvailable: needRefresh,
      dismissOfflineReady: () => setOfflineReady(false),
      dismissUpdate: () => setNeedRefresh(false),
      applyUpdate: async () => {
        await updateServiceWorker(true)
      },
      checkForUpdates,
    }),
    [checkForUpdates, isOnline, needRefresh, offlineReady, setNeedRefresh, setOfflineReady, updateServiceWorker],
  )

  return <PwaStatusContext.Provider value={value}>{props.children}</PwaStatusContext.Provider>
}

export function PwaStatusMockProvider(
  props: PropsWithChildren<{ value?: Partial<PwaStatusContextValue> }>,
) {
  const value = useMemo(() => buildPwaStatusValue(props.value), [props.value])
  return <PwaStatusContext.Provider value={value}>{props.children}</PwaStatusContext.Provider>
}

export function usePwaStatus() {
  const value = useContext(PwaStatusContext)
  if (!value) throw new Error('usePwaStatus must be used within PwaStatusProvider')
  return value
}
