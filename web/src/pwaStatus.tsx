import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type PropsWithChildren } from 'react'
import { useRegisterSW } from 'virtual:pwa-register/react'
import {
  createPwaUpdateActivator,
  createPwaUpdateLifecycleController,
  phaseAfterSuccessfulUpdateCheck,
  type PwaUpdatePhase,
} from './pwaUpdateLifecycle'

const UPDATE_CHECK_INTERVAL_MS = 60 * 60 * 1000

export type PwaStatusContextValue = {
  isOnline: boolean
  offlineReady: boolean
  updatePhase: PwaUpdatePhase
  updatePromptVisible: boolean
  updateAvailable: boolean
  dismissOfflineReady: () => void
  dismissUpdate: () => void
  applyUpdate: () => Promise<void>
  applyUpdateOnNavigation: () => Promise<void>
  checkForUpdates: () => Promise<void>
}

const PwaStatusContext = createContext<PwaStatusContextValue | null>(null)

function isPwaEnvEnabled(): boolean {
  const flag = (import.meta.env.VITE_DOCKREV_PWA ?? '').trim().toLowerCase()
  return flag !== 'off' && flag !== 'false' && flag !== '0'
}

export function isPwaRuntimeEnabled(): boolean {
  return isPwaEnvEnabled()
}

function buildPwaStatusValue(
  overrides?: Partial<PwaStatusContextValue>,
): PwaStatusContextValue {
  return {
    isOnline: true,
    offlineReady: false,
    updatePhase: 'idle',
    updatePromptVisible: false,
    updateAvailable: false,
    dismissOfflineReady: () => {},
    dismissUpdate: () => {},
    applyUpdate: async () => {},
    applyUpdateOnNavigation: async () => {},
    checkForUpdates: async () => {},
    ...overrides,
  }
}

function readOnlineStatus(): boolean {
  if (typeof navigator === 'undefined') return true
  return navigator.onLine
}

function LivePwaStatusProvider(props: PropsWithChildren) {
  const registrationRef = useRef<ServiceWorkerRegistration | null>(null)
  const updateLifecycleRef = useRef<ReturnType<typeof createPwaUpdateLifecycleController> | null>(null)
  const updatePhaseRef = useRef<PwaUpdatePhase>('idle')
  const updateServiceWorkerRef = useRef<(reloadPage?: boolean) => Promise<void>>(async () => {})
  const updateActivatorRef = useRef<ReturnType<typeof createPwaUpdateActivator> | null>(null)
  const [isOnline, setIsOnline] = useState(readOnlineStatus)
  const [updatePhase, setUpdatePhase] = useState<PwaUpdatePhase>('idle')
  const [updatePromptVisible, setUpdatePromptVisible] = useState(false)
  const transitionUpdatePhase = useCallback((phase: PwaUpdatePhase) => {
    if (updatePhaseRef.current === phase) return
    updatePhaseRef.current = phase
    setUpdatePhase(phase)
    if (phase !== 'idle') setUpdatePromptVisible(true)
  }, [])
  const {
    offlineReady: [offlineReady, setOfflineReady],
    updateServiceWorker,
  } = useRegisterSW({
    onRegistered(registration) {
      registrationRef.current = registration ?? null
      if (!registration) return
      if (!updateLifecycleRef.current) {
        updateLifecycleRef.current = createPwaUpdateLifecycleController({
          hasControllingWorker: () => Boolean(navigator.serviceWorker?.controller),
          onPhaseChange: transitionUpdatePhase,
        })
      }
      updateLifecycleRef.current.attach(registration)
    },
    onNeedRefresh() {
      if (registrationRef.current?.waiting) transitionUpdatePhase('ready')
    },
    onRegisterError(error) {
      console.error('[dockrev] service worker registration failed', error)
    },
  })

  useEffect(() => {
    updateServiceWorkerRef.current = updateServiceWorker
  }, [updateServiceWorker])

  useEffect(() => {
    updateActivatorRef.current = createPwaUpdateActivator({
      activate: () => updateServiceWorkerRef.current(true),
      hasWaitingWorker: () => Boolean(registrationRef.current?.waiting),
      isReady: () => updatePhaseRef.current === 'ready',
    })
    return () => {
      updateActivatorRef.current = null
    }
  }, [])

  useEffect(() => {
    return () => updateLifecycleRef.current?.dispose()
  }, [])

  const checkForUpdates = useMemo(
    () => async () => {
      if (typeof document !== 'undefined' && document.visibilityState !== 'visible') return
      const registration = registrationRef.current
      if (!registration) return
      try {
        await registration.update()
        const nextPhase = phaseAfterSuccessfulUpdateCheck(
          updatePhaseRef.current,
          Boolean(registration.waiting),
        )
        if (nextPhase === 'idle' && updatePhaseRef.current !== 'idle') {
          updatePhaseRef.current = 'idle'
          setUpdatePhase('idle')
          setUpdatePromptVisible(false)
        } else if (nextPhase === 'ready' && updatePhaseRef.current !== 'ready') {
          transitionUpdatePhase('ready')
        }
      } catch (error) {
        console.warn('[dockrev] service worker update check failed', error)
        if (updatePhaseRef.current !== 'ready') {
          updatePhaseRef.current = 'failed'
          setUpdatePhase('failed')
          setUpdatePromptVisible(true)
        }
      }
    },
    [transitionUpdatePhase],
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

    const initialCheckTimer =
      document.visibilityState === 'visible' ? window.setTimeout(onFocus, 0) : null

    return () => {
      window.clearInterval(timer)
      if (initialCheckTimer !== null) window.clearTimeout(initialCheckTimer)
      window.removeEventListener('online', onOnline)
      window.removeEventListener('offline', onOffline)
      window.removeEventListener('focus', onFocus)
      window.removeEventListener('pageshow', onPageShow)
      document.removeEventListener('visibilitychange', onVisible)
    }
  }, [checkForUpdates])

  const applyUpdate = useMemo(
    () => async () => {
      if (updatePhaseRef.current !== 'ready') return
      try {
        await updateActivatorRef.current?.request()
      } catch (error) {
        // Keep the waiting worker and prompt so the user can retry activation.
        console.warn('[dockrev] service worker activation failed', error)
      }
    },
    [],
  )

  const value = useMemo<PwaStatusContextValue>(
    () => ({
      isOnline,
      offlineReady,
      updatePhase,
      updatePromptVisible,
      updateAvailable: updatePhase === 'ready' && updatePromptVisible,
      dismissOfflineReady: () => setOfflineReady(false),
      dismissUpdate: () => setUpdatePromptVisible(false),
      applyUpdate,
      applyUpdateOnNavigation: applyUpdate,
      checkForUpdates,
    }),
    [applyUpdate, checkForUpdates, isOnline, offlineReady, setOfflineReady, updatePhase, updatePromptVisible],
  )

  return <PwaStatusContext.Provider value={value}>{props.children}</PwaStatusContext.Provider>
}

export function PwaStatusProvider(props: PropsWithChildren) {
  if (!isPwaEnvEnabled()) {
    return (
      <PwaStatusContext.Provider value={buildPwaStatusValue()}>
        {props.children}
      </PwaStatusContext.Provider>
    )
  }
  return <LivePwaStatusProvider {...props} />
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
