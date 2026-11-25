import { useSyncExternalStore } from 'react'

import { navigateToHref } from '../routes'
import { applyDemoShareStateToSearchParams, buildDemoHref, currentDemoShareState, readDemoShareState, withShareStatePatch } from './scenes'
import { installDemoRuntime } from './runtimeBootstrap'
import { clearDemoFixture, clearDemoMutations, demoSessionSnapshot } from './session'
import type { DemoSceneId, DemoSessionSnapshot, DemoShareState, DemoShareStatePatch } from './types'

export type DemoRuntimeSnapshot = DemoSessionSnapshot & {
  /** Bumped whenever the mock model was rebuilt, so the app subtree remounts. */
  revision: number
  ready: boolean
}

let snapshot: DemoRuntimeSnapshot = {
  shareState: currentDemoShareState(),
  fixtureState: 'seeded',
  fixtureBytes: 0,
  mutations: [],
  revision: 0,
  ready: false,
}

const listeners = new Set<() => void>()

function publish(next: Partial<DemoRuntimeSnapshot>) {
  snapshot = { ...snapshot, ...next }
  for (const listener of listeners) listener()
}

function refreshSessionState(shareState: DemoShareState) {
  publish(demoSessionSnapshot(shareState))
}

/**
 * Writes the share contract into the current URL without a document reload.
 * Scene navigation goes through the app router; only mock-affecting state
 * rebuilds the runtime.
 */
function writeShareState(shareState: DemoShareState, options: { replace: boolean }) {
  const url = new URL(window.location.href)
  applyDemoShareStateToSearchParams(url.searchParams, shareState)
  const next = `${url.pathname}?${url.searchParams.toString()}`
  if (options.replace) window.history.replaceState({}, '', next)
  else window.history.pushState({}, '', next)
}

const MOCK_AFFECTING_KEYS = ['sceneId', 'persona', 'network', 'asyncCase', 'events', 'cleanup'] as const

export function updateDemoShareState(patch: DemoShareStatePatch) {
  const previous = snapshot.shareState
  const next = withShareStatePatch(previous, patch)
  const modelChanged = MOCK_AFFECTING_KEYS.some((key) => previous[key] !== next[key])
  writeShareState(next, { replace: !modelChanged })
  refreshSessionState(next)
  if (!modelChanged) return
  void rebuildDemoRuntime(next)
}

async function rebuildDemoRuntime(shareState: DemoShareState) {
  // The mock layer is re-armed for the new scene, then the app subtree is
  // remounted through `revision` so its data effects re-read the new model.
  await installDemoRuntime(shareState, () => refreshSessionState(shareState))
  publish({ revision: snapshot.revision + 1 })
}

/**
 * Scene selection is a navigation, not a reload: the target path and the share
 * contract are pushed together, then the mock model is rebuilt for the scene.
 */
export function selectDemoScene(sceneId: DemoSceneId) {
  const next = withShareStatePatch(snapshot.shareState, { sceneId })
  const href = buildDemoHref(next, import.meta.env.BASE_URL ?? '/')
  navigateToHref(href)
  refreshSessionState(next)
  void rebuildDemoRuntime(next)
}

/** Router-driven navigation keeps the demo share contract on the new pathname. */
export function syncDemoShareStateFromUrl() {
  const next = readDemoShareState(new URL(window.location.href))
  if (next.sceneId === snapshot.shareState.sceneId) {
    refreshSessionState(next)
    return
  }
  refreshSessionState(next)
  void rebuildDemoRuntime(next)
}

export function resetDemoSession() {
  clearDemoFixture()
  clearDemoMutations()
  const sceneId = snapshot.shareState.sceneId
  const next = withShareStatePatch(snapshot.shareState, { sceneId })
  writeShareState(next, { replace: false })
  refreshSessionState(next)
  void rebuildDemoRuntime(next)
}

export function markDemoRuntimeReady() {
  refreshSessionState(snapshot.shareState)
  publish({ ready: true })
}

/** Boots the demo runtime with the store wired as the mutation observer. */
export async function startDemoRuntime() {
  await installDemoRuntime(snapshot.shareState, () => refreshSessionState(snapshot.shareState))
  markDemoRuntimeReady()
}

export function subscribeDemoRuntime(listener: () => void) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export function readDemoRuntime(): DemoRuntimeSnapshot {
  return snapshot
}

export function useDemoRuntime(): DemoRuntimeSnapshot {
  return useSyncExternalStore(subscribeDemoRuntime, readDemoRuntime, readDemoRuntime)
}
