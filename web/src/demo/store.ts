import { useSyncExternalStore } from 'react'

import { navigateToHref } from '../routes'
import { applyDemoShareStateToSearchParams, buildDemoHref, currentDemoShareState, defaultShareState, readDemoShareState, withShareStatePatch } from './scenes'
import { applyDemoEnvironment, installDemoRuntime, setDemoMutationObserver } from './runtimeBootstrap'
import { applyDemoMotionPreference } from './motion'
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
 * The product router owns the pathname; only the query is rewritten here.
 */
function writeShareState(shareState: DemoShareState) {
  const url = new URL(window.location.href)
  applyDemoShareStateToSearchParams(url.searchParams, shareState)
  window.history.replaceState({}, '', `${url.pathname}?${url.searchParams.toString()}`)
}

/**
 * Scene-owned state needs the mock model rebuilt. Environment state (identity,
 * connection, latency, motion) is applied in place so a review keeps the page.
 */
const MODEL_KEYS = ['sceneId', 'persona', 'asyncCase', 'events', 'cleanup'] as const

export function updateDemoShareState(patch: DemoShareStatePatch) {
  const previous = snapshot.shareState
  const next = withShareStatePatch(previous, patch)
  writeShareState(next)
  applyDemoEnvironment(next)
  applyDemoMotionPreference(next.motion)
  refreshSessionState(next)

  const modelChanged = MODEL_KEYS.some((key) => previous[key] !== next[key])
  // Recovering connectivity must re-read the product state that failed offline
  // (identity included). Going offline deliberately does not, so already
  // rendered content is not cleared by the switch itself.
  const recovered = previous.connection === 'offline' && next.connection === 'online'
  if (modelChanged || recovered) void rebuildDemoRuntime(next)
}

async function rebuildDemoRuntime(shareState: DemoShareState) {
  // The mock layer is re-armed for the new model, then the app subtree is
  // remounted through `revision` so its data effects re-read the new state.
  const observer = () => refreshSessionState(shareState)
  setDemoMutationObserver(observer)
  await installDemoRuntime(shareState, observer)
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
  const next = readDemoShareState(new URL(window.location.href), snapshot.shareState)
  applyDemoMotionPreference(next.motion)
  refreshSessionState(next)
  if (next.sceneId !== snapshot.shareState.sceneId) void rebuildDemoRuntime(next)
}

/** Back to the entry defaults, keeping the current route. */
export function resetDemoSession() {
  clearDemoFixture()
  clearDemoMutations()
  const next = { ...defaultShareState(snapshot.shareState.sceneId), controlsHidden: snapshot.shareState.controlsHidden }
  writeShareState(next)
  applyDemoEnvironment(next)
  applyDemoMotionPreference(next.motion)
  refreshSessionState(next)
  void rebuildDemoRuntime(next)
}

export function markDemoRuntimeReady() {
  applyDemoMotionPreference(snapshot.shareState.motion)
  refreshSessionState(snapshot.shareState)
  publish({ ready: true })
}

/** Boots the demo runtime with the store wired as the mutation observer. */
export async function startDemoRuntime() {
  const observer = () => refreshSessionState(snapshot.shareState)
  setDemoMutationObserver(observer)
  await installDemoRuntime(snapshot.shareState, observer)
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
