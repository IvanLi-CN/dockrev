export type OverviewToolPanelSide = 'left' | 'right'

export type OverviewToolPanelBounds = {
  left: number
  top: number
  right: number
  bottom: number
}

export type OverviewToolPanelSize = {
  width: number
  height: number
}

export type OverviewToolPanelState = {
  collapsed: boolean
  left: number
  side: OverviewToolPanelSide
  top: number
}

export const OVERVIEW_TOOL_PANEL_STORAGE_KEY = 'dockrev:overview:tool-panel:v1'
export const OVERVIEW_TOOL_PANEL_MARGIN = 18
export const OVERVIEW_TOOL_PANEL_COLLAPSE_THRESHOLD = 96
export const OVERVIEW_TOOL_PANEL_DEFAULT_SIZE: OverviewToolPanelSize = {
  width: 320,
  height: 320,
}
export const OVERVIEW_TOOL_BUBBLE_DEFAULT_SIZE: OverviewToolPanelSize = {
  width: 60,
  height: 60,
}
export const OVERVIEW_TOOL_BUBBLE_EDGE_OVERHANG = 14

function clamp(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min
  if (max <= min) return min
  return Math.min(Math.max(value, min), max)
}

export function clampOverviewToolPanelTop(
  top: number,
  bounds: OverviewToolPanelBounds,
  height: number,
  margin: number = OVERVIEW_TOOL_PANEL_MARGIN,
): number {
  const minTop = bounds.top + margin
  const maxTop = Math.max(minTop, bounds.bottom - height - margin)
  return clamp(top, minTop, maxTop)
}

export function clampOverviewToolPanelLeft(
  left: number,
  bounds: OverviewToolPanelBounds,
  width: number,
  margin: number = OVERVIEW_TOOL_PANEL_MARGIN,
): number {
  const minLeft = bounds.left + margin
  const maxLeft = Math.max(minLeft, bounds.right - width - margin)
  return clamp(left, minLeft, maxLeft)
}

export function resolveOverviewToolPanelLeft(
  side: OverviewToolPanelSide,
  bounds: OverviewToolPanelBounds,
  width: number,
  margin: number = OVERVIEW_TOOL_PANEL_MARGIN,
): number {
  const minLeft = bounds.left + margin
  const maxLeft = Math.max(minLeft, bounds.right - width - margin)
  return side === 'left' ? minLeft : maxLeft
}

export function resolveOverviewToolBubbleLeft(
  side: OverviewToolPanelSide,
  bounds: OverviewToolPanelBounds,
  width: number,
  edgeOverhang: number = OVERVIEW_TOOL_BUBBLE_EDGE_OVERHANG,
): number {
  return side === 'left' ? bounds.left - edgeOverhang : bounds.right - width + edgeOverhang
}

export function snapOverviewToolPanelSide(
  left: number,
  bounds: OverviewToolPanelBounds,
  width: number,
  margin: number = OVERVIEW_TOOL_PANEL_MARGIN,
): OverviewToolPanelSide {
  const leftDock = resolveOverviewToolPanelLeft('left', bounds, width, margin)
  const rightDock = resolveOverviewToolPanelLeft('right', bounds, width, margin)
  return Math.abs(left - leftDock) <= Math.abs(left - rightDock) ? 'left' : 'right'
}

export function resolveOverviewToolPanelRect(
  state: OverviewToolPanelState,
  bounds: OverviewToolPanelBounds,
  size: OverviewToolPanelSize,
  margin: number = OVERVIEW_TOOL_PANEL_MARGIN,
): { left: number; top: number } {
  return {
    left: state.collapsed
      ? resolveOverviewToolBubbleLeft(state.side, bounds, size.width)
      : clampOverviewToolPanelLeft(state.left, bounds, size.width, margin),
    top: clampOverviewToolPanelTop(state.top, bounds, size.height, margin),
  }
}

export function createDefaultOverviewToolPanelState(
  bounds: OverviewToolPanelBounds,
  size: OverviewToolPanelSize = OVERVIEW_TOOL_PANEL_DEFAULT_SIZE,
  margin: number = OVERVIEW_TOOL_PANEL_MARGIN,
): OverviewToolPanelState {
  const preferredTop = bounds.bottom - size.height - 28
  return {
    collapsed: false,
    left: clampOverviewToolPanelLeft(bounds.right - size.width - 28, bounds, size.width, margin),
    side: 'right',
    top: clampOverviewToolPanelTop(preferredTop, bounds, size.height, margin),
  }
}

export function readOverviewToolPanelState(): OverviewToolPanelState | null {
  if (typeof window === 'undefined') return null
  try {
    const raw = window.localStorage.getItem(OVERVIEW_TOOL_PANEL_STORAGE_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw) as Partial<OverviewToolPanelState> | null
    if (!parsed || typeof parsed !== 'object') return null
    const side = parsed.side === 'left' || parsed.side === 'right' ? parsed.side : null
    const left = typeof parsed.left === 'number' && Number.isFinite(parsed.left) ? parsed.left : null
    const top = typeof parsed.top === 'number' && Number.isFinite(parsed.top) ? parsed.top : null
    const collapsed = typeof parsed.collapsed === 'boolean' ? parsed.collapsed : null
    if (!side || left == null || top == null || collapsed == null) return null
    return { collapsed, left, side, top }
  } catch {
    return null
  }
}

export function writeOverviewToolPanelState(state: OverviewToolPanelState) {
  if (typeof window === 'undefined') return
  try {
    window.localStorage.setItem(OVERVIEW_TOOL_PANEL_STORAGE_KEY, JSON.stringify(state))
  } catch {
    // Layout preference persistence is best-effort only.
  }
}
