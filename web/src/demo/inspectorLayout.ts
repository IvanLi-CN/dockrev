import type { DemoSceneId, DemoShareState } from './types'
import { DEMO_INSPECTOR_LAYOUT_STORAGE_KEY } from './session'

export type DemoInspectorSide = 'left' | 'right'

export type DemoInspectorLayout = {
  side: DemoInspectorSide
  collapsed: boolean
  /** Vertical offsets are stored as ratios so a scene reopens where the reviewer left it. */
  topRatio: number
}

const DEFAULT_LAYOUT: DemoInspectorLayout = { side: 'right', collapsed: false, topRatio: 0.18 }

type StoredLayouts = {
  version: 1
  scenes: Partial<Record<DemoSceneId, DemoInspectorLayout>>
}

function readStore(): StoredLayouts {
  if (typeof window === 'undefined') return { version: 1, scenes: {} }
  try {
    const raw = window.localStorage.getItem(DEMO_INSPECTOR_LAYOUT_STORAGE_KEY)
    if (!raw) return { version: 1, scenes: {} }
    const parsed = JSON.parse(raw) as Partial<StoredLayouts>
    if (parsed.version !== 1 || !parsed.scenes) return { version: 1, scenes: {} }
    return { version: 1, scenes: parsed.scenes }
  } catch {
    return { version: 1, scenes: {} }
  }
}

function normalizeLayout(value: Partial<DemoInspectorLayout> | undefined): DemoInspectorLayout {
  if (!value) return DEFAULT_LAYOUT
  const topRatio = Number(value.topRatio)
  return {
    side: value.side === 'left' ? 'left' : 'right',
    collapsed: value.collapsed === true,
    topRatio: Number.isFinite(topRatio) ? Math.min(Math.max(topRatio, 0), 1) : DEFAULT_LAYOUT.topRatio,
  }
}

export function readDemoInspectorLayout(sceneId: DemoSceneId): DemoInspectorLayout {
  return normalizeLayout(readStore().scenes[sceneId])
}

export function writeDemoInspectorLayout(sceneId: DemoSceneId, layout: DemoInspectorLayout) {
  if (typeof window === 'undefined') return
  const store = readStore()
  store.scenes[sceneId] = normalizeLayout(layout)
  try {
    window.localStorage.setItem(DEMO_INSPECTOR_LAYOUT_STORAGE_KEY, JSON.stringify(store))
  } catch {
    // Panel geometry is local convenience only; a failed write is not fatal.
  }
}

/** Panel geometry never enters the share URL; only scene and data state do. */
export function resolveDemoInspectorTop(layout: DemoInspectorLayout, viewportHeight: number, panelHeight: number) {
  const margin = 16
  const available = Math.max(viewportHeight - panelHeight - margin * 2, 0)
  return margin + Math.round(available * layout.topRatio)
}

export function demoShareStateSummary(state: DemoShareState): string {
  return [
    state.sceneId,
    state.persona,
    state.connection,
    state.delay,
    state.motion !== 'system' ? state.motion : null,
    state.asyncCase,
  ]
    .filter((part) => part !== null)
    .map(String)
    .join(' · ')
}
