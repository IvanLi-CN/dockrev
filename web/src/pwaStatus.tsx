import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type PropsWithChildren } from 'react'
import { useRegisterSW } from 'virtual:pwa-register/react'
import {
  createPwaUpdateActivator,
  createPwaUpdateLifecycleController,
  phaseAfterSuccessfulUpdateCheck,
  type PwaUpdatePhase,
} from './pwaUpdateLifecycle'
import {
  createPwaInstallLifecycleController,
  detectSafariInstallPlatform,
  isStandaloneDisplayMode,
  resolvePwaInstallCapability,
  type PwaInstallCapability,
  type PwaInstallLifecycleController,
} from './pwaInstallLifecycle'

const UPDATE_CHECK_INTERVAL_MS = 60 * 60 * 1000

export type PwaStatusContextValue = {
  isOnline: boolean
  offlineReady: boolean
  installCapability: PwaInstallCapability
  updatePhase: PwaUpdatePhase
  updatePromptVisible: boolean
  updateAvailable: boolean
  dismissOfflineReady: () => void
  dismissUpdate: () => void
  applyUpdate: () => Promise<void>
  applyUpdateOnNavigation: () => Promise<void>
  checkForUpdates: () => Promise<void>
  requestPwaInstall: () => Promise<void>
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
    installCapability: null,
    updatePhase: 'idle',
    updatePromptVisible: false,
    updateAvailable: false,
    dismissOfflineReady: () => {},
    dismissUpdate: () => {},
    applyUpdate: async () => {},
    applyUpdateOnNavigation: async () => {},
    checkForUpdates: async () => {},
    requestPwaInstall: async () => {},
    ...overrides,
  }
}

function readOnlineStatus(): boolean {
  if (typeof navigator === 'undefined') return true
  return navigator.onLine
}

function readInitialPwaInstallCapability(): PwaInstallCapability {
  if (typeof window === 'undefined') return null
  const standalone = isStandaloneDisplayMode({
    matchMedia: typeof window.matchMedia === 'function' ? window.matchMedia.bind(window) : undefined,
    navigatorStandalone: (navigator as Navigator & { standalone?: boolean }).standalone,
  })
  return resolvePwaInstallCapability({
    standalone,
    hasBrowserPrompt: false,
    safariPlatform: detectSafariInstallPlatform({
      userAgent: navigator.userAgent,
      platform: navigator.platform,
      maxTouchPoints: navigator.maxTouchPoints,
    }),
  })
}

function LivePwaStatusProvider(props: PropsWithChildren) {
  const registrationRef = useRef<ServiceWorkerRegistration | null>(null)
  const installLifecycleRef = useRef<PwaInstallLifecycleController | null>(null)
  const updateLifecycleRef = useRef<ReturnType<typeof createPwaUpdateLifecycleController> | null>(null)
  const updatePhaseRef = useRef<PwaUpdatePhase>('idle')
  const updateServiceWorkerRef = useRef<(reloadPage?: boolean) => Promise<void>>(async () => {})
  const updateActivatorRef = useRef<ReturnType<typeof createPwaUpdateActivator> | null>(null)
  const [isOnline, setIsOnline] = useState(readOnlineStatus)
  const [installCapability, setInstallCapability] = useState<PwaInstallCapability>(readInitialPwaInstallCapability)
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
    if (typeof window === 'undefined') return

    const standalone = isStandaloneDisplayMode({
      matchMedia: window.matchMedia.bind(window),
      navigatorStandalone: (navigator as Navigator & { standalone?: boolean }).standalone,
    })
    const safariPlatform = detectSafariInstallPlatform({
      userAgent: navigator.userAgent,
      platform: navigator.platform,
      maxTouchPoints: navigator.maxTouchPoints,
    })
    const controller = createPwaInstallLifecycleController({
      eventTarget: window,
      isStandalone: standalone,
      safariPlatform,
      onCapabilityChange: setInstallCapability,
    })
    installLifecycleRef.current = controller
    controller.attach()

    return () => {
      controller.dispose()
      installLifecycleRef.current = null
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

  const requestPwaInstall = useCallback(async () => {
    await installLifecycleRef.current?.requestInstall()
  }, [])

  const value = useMemo<PwaStatusContextValue>(
    () => ({
      isOnline,
      offlineReady,
      installCapability,
      updatePhase,
      updatePromptVisible,
      updateAvailable: updatePhase === 'ready' && updatePromptVisible,
      dismissOfflineReady: () => setOfflineReady(false),
      dismissUpdate: () => setUpdatePromptVisible(false),
      applyUpdate,
      applyUpdateOnNavigation: applyUpdate,
      checkForUpdates,
      requestPwaInstall,
    }),
    [
      applyUpdate,
      checkForUpdates,
      installCapability,
      isOnline,
      offlineReady,
      requestPwaInstall,
      setOfflineReady,
      updatePhase,
      updatePromptVisible,
    ],
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
