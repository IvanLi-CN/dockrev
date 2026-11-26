import type { DemoMotionPreference } from './types'

const REDUCED_MOTION_QUERY = '(prefers-reduced-motion: reduce)'

/**
 * The product already honours the system reduced-motion preference in CSS.
 * This only records the reviewer's explicit choice so demo CSS can force the
 * reduced presentation without touching product data or theme.
 */
export function applyDemoMotionPreference(preference: DemoMotionPreference) {
  if (typeof document === 'undefined') return
  const root = document.documentElement
  if (preference === 'reduce') {
    root.dataset.demoMotion = 'reduce'
  } else {
    delete root.dataset.demoMotion
  }
}

export function effectiveReducedMotion(preference: DemoMotionPreference): boolean {
  if (preference === 'reduce') return true
  if (typeof window === 'undefined') return false
  return window.matchMedia(REDUCED_MOTION_QUERY).matches
}
