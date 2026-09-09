export type SafariInstallPlatform = 'ios' | 'macos'

export type PwaInstallCapability =
  | { kind: 'browser-prompt' }
  | { kind: 'safari-guide'; platform: SafariInstallPlatform }
  | null

export type PwaInstallPromptOutcome = 'accepted' | 'dismissed'

export interface BeforeInstallPromptEventLike extends Event {
  prompt: () => Promise<void>
  userChoice: Promise<{ outcome: PwaInstallPromptOutcome }>
}

export interface PwaInstallLifecycleTarget {
  addEventListener: (type: string, listener: EventListener) => void
  removeEventListener: (type: string, listener: EventListener) => void
}

export function isStandaloneDisplayMode(options: {
  matchMedia?: (query: string) => { matches: boolean }
  navigatorStandalone?: boolean
}): boolean {
  return Boolean(
    options.matchMedia?.('(display-mode: standalone)').matches || options.navigatorStandalone,
  )
}

export function detectSafariInstallPlatform(options: {
  userAgent: string
  platform?: string
  maxTouchPoints?: number
}): SafariInstallPlatform | null {
  const userAgent = options.userAgent
  const isSafari =
    /Safari\//.test(userAgent) &&
    !/CriOS|FxiOS|EdgiOS|OPiOS|Chrome|Chromium|Edg|Firefox|Android/.test(userAgent)
  if (!isSafari) return null

  const isIos =
    /iPhone|iPad|iPod/.test(userAgent) ||
    (options.platform === 'MacIntel' && (options.maxTouchPoints ?? 0) > 1)
  if (isIos) return 'ios'
  if (/Macintosh/.test(userAgent)) return 'macos'
  return null
}

export function resolvePwaInstallCapability(options: {
  standalone: boolean
  hasBrowserPrompt: boolean
  safariPlatform: SafariInstallPlatform | null
}): PwaInstallCapability {
  if (options.standalone) return null
  if (options.hasBrowserPrompt) return { kind: 'browser-prompt' }
  if (options.safariPlatform) return { kind: 'safari-guide', platform: options.safariPlatform }
  return null
}

export interface PwaInstallLifecycleControllerOptions {
  eventTarget: PwaInstallLifecycleTarget
  isStandalone: boolean
  safariPlatform: SafariInstallPlatform | null
  onCapabilityChange: (capability: PwaInstallCapability) => void
}

export class PwaInstallLifecycleController {
  private readonly options: PwaInstallLifecycleControllerOptions
  private deferredPrompt: BeforeInstallPromptEventLike | null = null
  private capability: PwaInstallCapability
  private attached = false

  constructor(options: PwaInstallLifecycleControllerOptions) {
    this.options = options
    this.capability = resolvePwaInstallCapability({
      standalone: options.isStandalone,
      hasBrowserPrompt: false,
      safariPlatform: options.safariPlatform,
    })
  }

  getCapability(): PwaInstallCapability {
    return this.capability
  }

  attach() {
    if (this.attached || this.options.isStandalone) return
    this.attached = true
    this.options.eventTarget.addEventListener('beforeinstallprompt', this.handleBeforeInstallPrompt)
    this.options.eventTarget.addEventListener('appinstalled', this.handleAppInstalled)
  }

  dispose() {
    if (!this.attached) return
    this.options.eventTarget.removeEventListener('beforeinstallprompt', this.handleBeforeInstallPrompt)
    this.options.eventTarget.removeEventListener('appinstalled', this.handleAppInstalled)
    this.attached = false
    this.deferredPrompt = null
  }

  async requestInstall(): Promise<PwaInstallPromptOutcome | null> {
    const deferredPrompt = this.deferredPrompt
    if (!deferredPrompt) return null

    this.deferredPrompt = null
    this.setCapability(null)
    await deferredPrompt.prompt()
    const choice = await deferredPrompt.userChoice
    return choice.outcome
  }

  private readonly handleBeforeInstallPrompt = (event: Event) => {
    const promptEvent = event as BeforeInstallPromptEventLike
    if (typeof promptEvent.prompt !== 'function' || !promptEvent.userChoice) return
    event.preventDefault()
    this.deferredPrompt = promptEvent
    this.setCapability({ kind: 'browser-prompt' })
  }

  private readonly handleAppInstalled = () => {
    this.deferredPrompt = null
    this.setCapability(null)
  }

  private setCapability(capability: PwaInstallCapability) {
    this.capability = capability
    this.options.onCapabilityChange(capability)
  }
}

export function createPwaInstallLifecycleController(
  options: PwaInstallLifecycleControllerOptions,
) {
  return new PwaInstallLifecycleController(options)
}
