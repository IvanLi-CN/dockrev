import { expect, test } from 'bun:test'

import {
  clampOverviewToolPanelTop,
  createDefaultOverviewToolPanelState,
  resolveOverviewToolPanelRect,
  snapOverviewToolPanelSide,
  type OverviewToolPanelBounds,
} from '../src/pages/overviewToolPanelState'

const bounds: OverviewToolPanelBounds = {
  left: 320,
  top: 84,
  right: 1520,
  bottom: 980,
}

test('default tool panel state docks to the right edge near the bottom', () => {
  const state = createDefaultOverviewToolPanelState(bounds, { width: 320, height: 320 })
  const rect = resolveOverviewToolPanelRect(state, bounds, { width: 320, height: 320 })

  expect(state.collapsed).toBe(false)
  expect(state.left).toBe(1172)
  expect(state.side).toBe('right')
  expect(rect.left).toBe(1172)
  expect(rect.top).toBe(632)
})

test('collapsed tool bubble resolves to the chosen edge while preserving vertical clamp', () => {
  const rect = resolveOverviewToolPanelRect(
    { collapsed: true, left: 900, side: 'right', top: 700 },
    bounds,
    { width: 60, height: 60 },
  )

  expect(rect.left).toBe(1474)
  expect(rect.top).toBe(700)
})

test('tool panel top clamps into the available viewport slice', () => {
  expect(clampOverviewToolPanelTop(40, bounds, 320)).toBe(102)
  expect(clampOverviewToolPanelTop(900, bounds, 320)).toBe(642)
})

test('tool panel snap side prefers the nearest dock edge', () => {
  expect(snapOverviewToolPanelSide(350, bounds, 320)).toBe('left')
  expect(snapOverviewToolPanelSide(1100, bounds, 320)).toBe('right')
})
