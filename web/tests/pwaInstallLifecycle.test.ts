import { describe, expect, test } from 'bun:test'

import {
  createPwaInstallLifecycleController,
  detectSafariInstallPlatform,
  resolvePwaInstallCapability,
  type BeforeInstallPromptEventLike,
} from '../src/pwaInstallLifecycle'

class FakeBeforeInstallPromptEvent extends Event implements BeforeInstallPromptEventLike {
  prompted = 0
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>

  constructor(outcome: 'accepted' | 'dismissed') {
    super('beforeinstallprompt', { cancelable: true })
    this.userChoice = Promise.resolve({ outcome })
  }

  prompt() {
    this.prompted += 1
    return Promise.resolve()
  }
}

describe('PWA install capability detection', () => {
  test('recognizes Safari on iOS and iPadOS while excluding iOS Chrome', () => {
    expect(
      detectSafariInstallPlatform({
        userAgent:
          'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Version/18.0 Mobile/15E148 Safari/604.1',
      }),
    ).toBe('ios')
    expect(
      detectSafariInstallPlatform({
        userAgent:
          'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 CriOS/128.0.0.0 Mobile/15E148 Safari/604.1',
      }),
    ).toBeNull()
  })

  test('recognizes touch iPadOS and macOS Safari fallbacks', () => {
    expect(
      detectSafariInstallPlatform({
        userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 Version/17.0 Safari/605.1.15',
      }),
    ).toBe('macos')
    expect(
      detectSafariInstallPlatform({
        userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 Version/17.0 Mobile/15E148 Safari/604.1',
        platform: 'MacIntel',
        maxTouchPoints: 5,
      }),
    ).toBe('ios')
  })

  test('prefers the browser prompt and hides standalone apps', () => {
    expect(
      resolvePwaInstallCapability({ standalone: false, hasBrowserPrompt: true, safariPlatform: 'ios' }),
    ).toEqual({ kind: 'browser-prompt' })
    expect(
      resolvePwaInstallCapability({ standalone: true, hasBrowserPrompt: true, safariPlatform: 'ios' }),
    ).toBeNull()
  })
})

describe('PwaInstallLifecycleController', () => {
  test('captures a browser prompt, prevents the automatic prompt, and clears after use', async () => {
    const target = new EventTarget()
    const capabilities: unknown[] = []
    const controller = createPwaInstallLifecycleController({
      eventTarget: target,
      isStandalone: false,
      safariPlatform: null,
      onCapabilityChange: (capability) => capabilities.push(capability),
    })
    controller.attach()

    const event = new FakeBeforeInstallPromptEvent('accepted')
    target.dispatchEvent(event)
    expect(event.defaultPrevented).toBe(true)
    expect(controller.getCapability()).toEqual({ kind: 'browser-prompt' })

    await expect(controller.requestInstall()).resolves.toBe('accepted')
    expect(event.prompted).toBe(1)
    expect(controller.getCapability()).toBeNull()
    await expect(controller.requestInstall()).resolves.toBeNull()
    expect(capabilities).toEqual([{ kind: 'browser-prompt' }, null])
  })

  test('exposes Safari guidance until the app is installed', () => {
    const target = new EventTarget()
    const controller = createPwaInstallLifecycleController({
      eventTarget: target,
      isStandalone: false,
      safariPlatform: 'ios',
      onCapabilityChange: () => {},
    })
    expect(controller.getCapability()).toEqual({ kind: 'safari-guide', platform: 'ios' })
    controller.attach()
    target.dispatchEvent(new Event('appinstalled'))
    expect(controller.getCapability()).toBeNull()
  })

  test('does not expose an install prompt for a standalone app', () => {
    const target = new EventTarget()
    const controller = createPwaInstallLifecycleController({
      eventTarget: target,
      isStandalone: true,
      safariPlatform: 'ios',
      onCapabilityChange: () => {},
    })
    controller.attach()
    target.dispatchEvent(new FakeBeforeInstallPromptEvent('accepted'))
    expect(controller.getCapability()).toBeNull()
  })
})
