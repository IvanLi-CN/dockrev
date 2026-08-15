import { describe, expect, test } from 'bun:test'

import type { ServiceReleaseNoteItem } from '../src/api'
import {
  formatVersionDirectoryTimeLabel,
  mergeReleaseNoteItems,
  observeVersionSectionInlineWidth,
} from '../src/components/serviceVersionsSectionUtils'

const NOW = Date.UTC(2026, 6, 16, 12, 0, 0)

function isoOffset(offsetMs: number): string {
  return new Date(NOW - offsetMs).toISOString()
}

function note(id: string, tagName: string): ServiceReleaseNoteItem {
  return {
    id,
    tagName,
    htmlUrl: `https://github.com/acme/app/releases/tag/${tagName}`,
    draft: false,
    prerelease: false,
    publishedAt: '2026-07-16T00:00:00.000Z',
  }
}

describe('formatVersionDirectoryTimeLabel', () => {
  test('formats just now, minutes, hours, and days within the seven-day window', () => {
    expect(formatVersionDirectoryTimeLabel(isoOffset(30_000), NOW)).toBe('刚刚')
    expect(formatVersionDirectoryTimeLabel(isoOffset(5 * 60_000), NOW)).toBe('5 分钟前')
    expect(formatVersionDirectoryTimeLabel(isoOffset(3 * 60 * 60_000), NOW)).toBe('3 小时前')
    expect(formatVersionDirectoryTimeLabel(isoOffset(6 * 24 * 60 * 60_000), NOW)).toBe('6 天前')
  })

  test('keeps the seven-day boundary relative and older values absolute', () => {
    expect(formatVersionDirectoryTimeLabel(isoOffset(7 * 24 * 60 * 60_000), NOW)).toBe('7 天前')
    expect(formatVersionDirectoryTimeLabel(isoOffset((7 * 24 * 60 * 60_000) + 1), NOW)).toBe('2026-07-09')
  })

  test('falls back for invalid or missing timestamps', () => {
    expect(formatVersionDirectoryTimeLabel('not-a-date', NOW)).toBe('not-a-date')
    expect(formatVersionDirectoryTimeLabel('', NOW)).toBe('时间未知')
  })
})

describe('mergeReleaseNoteItems', () => {
  test('deduplicates incoming items by stable id while preserving order', () => {
    const merged = mergeReleaseNoteItems(
      [note('github:1', 'v1.0.0'), note('github:2', 'v1.1.0')],
      [note('github:2', 'v1.1.0'), note('github:3', 'v1.2.0')],
    )

    expect(merged.map((item) => item.id)).toEqual(['github:1', 'github:2', 'github:3'])
  })
})

describe('observeVersionSectionInlineWidth', () => {
  test('uses the app shell mutation fallback when ResizeObserver is unavailable', () => {
    const originalWindow = globalThis.window
    let width = 1_079
    const callbacks = new Map<string, () => void>()
    const queuedFrames = new Map<number, FrameRequestCallback>()
    let nextFrameId = 1
    let observedTarget: object | null = null
    let observedOptions: MutationObserverInit | null = null
    let mutationCallback: MutationCallback | null = null
    let disconnected = false
    const shell = {}
    const fakeWindow = {
      ResizeObserver: undefined,
      MutationObserver: class {
        constructor(callback: MutationCallback) {
          mutationCallback = callback
        }

        observe(target: Node, options?: MutationObserverInit) {
          observedTarget = target
          observedOptions = options ?? null
        }

        disconnect() {
          disconnected = true
        }
      },
      addEventListener(type: string, callback: () => void) {
        callbacks.set(type, callback)
      },
      removeEventListener(type: string) {
        callbacks.delete(type)
      },
      requestAnimationFrame(callback: FrameRequestCallback) {
        const frameId = nextFrameId++
        queuedFrames.set(frameId, callback)
        return frameId
      },
      cancelAnimationFrame(frameId: number) {
        queuedFrames.delete(frameId)
      },
    }
    const element = {
      getBoundingClientRect: () => ({ width }),
      closest: (selector: string) => (selector === '.appShell' ? shell : null),
    } as unknown as HTMLElement
    const widths: number[] = []

    try {
      Reflect.set(globalThis, 'window', fakeWindow)
      const stop = observeVersionSectionInlineWidth(element, (nextWidth) => widths.push(nextWidth))

      expect(widths).toEqual([1_079])
      expect(observedTarget).toBe(shell)
      expect(observedOptions).toEqual({ attributes: true, attributeFilter: ['class', 'style'] })
      expect(callbacks.has('resize')).toBe(true)

      width = 1_080
      mutationCallback?.([], {} as MutationObserver)
      expect(widths).toEqual([1_079])
      expect(queuedFrames.size).toBe(1)
      queuedFrames.values().next().value?.(0)
      expect(widths).toEqual([1_079, 1_080])

      stop()
      expect(disconnected).toBe(true)
      expect(callbacks.has('resize')).toBe(false)
    } finally {
      Reflect.set(globalThis, 'window', originalWindow)
    }
  })
})
