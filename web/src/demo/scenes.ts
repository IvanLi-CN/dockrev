import { normalizeAppBasePath, stripAppBase } from '../appBase'
import type { DockrevApiScenario } from '../mock/dockrevMockApi/shared'
import type {
  DemoAsyncCase,
  DemoCleanupScenario,
  DemoManagementEventsMode,
  DemoNetworkMode,
  DemoPersonaId,
  DemoSceneId,
  DemoShareState,
  DemoShareStatePatch,
} from './types'

export type DemoScene = {
  id: DemoSceneId
  label: string
  note: string
  /** App route this scene opens. Empty string is the overview root. */
  path: string
  fixtureScenario: Exclude<DockrevApiScenario, 'error'>
  defaults: Pick<DemoShareState, 'persona' | 'network' | 'asyncCase' | 'events' | 'cleanup'>
}

export const DEMO_PERSONAS: readonly DemoPersonaId[] = ['anonymous', 'admin']
export const DEMO_NETWORK_MODES: readonly DemoNetworkMode[] = ['normal', 'slow', 'faulty']
export const DEMO_ASYNC_CASES: readonly DemoAsyncCase[] = [
  'default',
  'cold',
  'cache-refresh',
  'error',
  'job-detail-retry',
]
export const DEMO_EVENTS_MODES: readonly DemoManagementEventsMode[] = ['default', 'reconnecting']
export const DEMO_CLEANUP_SCENARIOS: readonly DemoCleanupScenario[] = [
  'cleanup-console-storage-normal',
  'cleanup-console-confirm-pending',
  'cleanup-console-confirm-failed',
]

const DEFAULT_SCENE_DEFAULTS: DemoScene['defaults'] = {
  persona: 'anonymous',
  network: 'normal',
  asyncCase: 'default',
  events: 'default',
  cleanup: 'cleanup-console-storage-normal',
}

export const DEMO_SCENES: readonly DemoScene[] = [
  {
    id: 'overview',
    label: 'Overview',
    note: '导航概览与资源摘要',
    path: '',
    fixtureScenario: 'settings-configured',
    defaults: { ...DEFAULT_SCENE_DEFAULTS, persona: 'admin' },
  },
  {
    id: 'services',
    label: 'Services',
    note: '运维大盘与候选版本',
    path: '/services',
    fixtureScenario: 'settings-configured',
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
  },
  {
    id: 'service-actions',
    label: 'Service actions',
    note: '更新进行中与生命周期动作',
    path: '/services/stack-prod/svc-prod-api',
    fixtureScenario: 'service-action-progress',
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
  },
  {
    id: 'service-versions',
    label: 'Service versions',
    note: '版本历史与回滚目标',
    path: '/services/stack-prod/svc-prod-api/versions',
    fixtureScenario: 'service-selected-version-updates',
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
  },
  {
    id: 'queue',
    label: 'Queue',
    note: '任务队列与 GHCR 假写',
    path: '/queue',
    fixtureScenario: 'queue-mixed',
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
  },
  {
    id: 'queue-logs',
    label: 'Queue logs',
    note: '长日志与实时输出',
    path: '/queue/job-live-long',
    fixtureScenario: 'queue-long-logs',
    defaults: { ...DEFAULT_SCENE_DEFAULTS, network: 'slow' },
  },
  {
    id: 'settings',
    label: 'Settings',
    note: '鉴权、通知与集成',
    path: '/settings',
    fixtureScenario: 'settings-configured',
    defaults: { ...DEFAULT_SCENE_DEFAULTS, persona: 'admin' },
  },
  {
    id: 'ghcr-registry',
    label: 'GHCR registry',
    note: 'Webhook 维护与假写',
    path: '/settings/ghcr-webhooks',
    fixtureScenario: 'settings-configured',
    defaults: { ...DEFAULT_SCENE_DEFAULTS, persona: 'admin' },
  },
  {
    id: 'cleanup',
    label: 'Cleanup',
    note: '扫描规则与 apply',
    path: '/cleanup',
    fixtureScenario: 'cleanup-console-storage-normal',
    defaults: {
      ...DEFAULT_SCENE_DEFAULTS,
      persona: 'admin',
      cleanup: 'cleanup-console-storage-normal',
    },
  },
  {
    id: 'deploy-check',
    label: 'Deploy check',
    note: '部署检查报告',
    path: '/deploy-check',
    fixtureScenario: 'settings-configured',
    defaults: { ...DEFAULT_SCENE_DEFAULTS, persona: 'admin' },
  },
]

export const DEFAULT_DEMO_SCENE: DemoSceneId = 'overview'

const SCENE_BY_ID = new Map(DEMO_SCENES.map((scene) => [scene.id, scene]))

export function demoScene(id: DemoSceneId): DemoScene {
  const scene = SCENE_BY_ID.get(id)
  if (!scene) throw new Error(`unknown demo scene: ${id}`)
  return scene
}

function isSceneId(value: string | null | undefined): value is DemoSceneId {
  return value != null && SCENE_BY_ID.has(value as DemoSceneId)
}

function normalizeFromList<T extends string>(value: string | null | undefined, allowed: readonly T[], fallback: T): T {
  return value != null && (allowed as readonly string[]).includes(value) ? (value as T) : fallback
}

export function defaultShareState(sceneId: DemoSceneId): DemoShareState {
  const scene = demoScene(sceneId)
  return { sceneId, ...scene.defaults, controlsHidden: false }
}

/**
 * The page is the source of truth: a pathname that names a scene wins over the
 * `demo` parameter, so the inspector label can never disagree with the route.
 */
export function demoSceneForPath(pathname: string, requested?: DemoSceneId | null): DemoSceneId | null {
  const normalized = stripAppBase(pathname).replace(/\/+$/, '') || '/'
  const candidates = DEMO_SCENES.filter((scene) => (scene.path === '' ? '/' : scene.path) === normalized)
  if (candidates.length === 0) return null
  if (requested && candidates.some((scene) => scene.id === requested)) return requested
  return candidates[0].id
}

/** Scene selection comes from the URL; the panel must never invent its own. */
export function readDemoShareState(url: URL): DemoShareState {
  const requestedScene = url.searchParams.get('demo')
  const requested = isSceneId(requestedScene) ? requestedScene : null
  const sceneId = demoSceneForPath(url.pathname, requested) ?? requested ?? DEFAULT_DEMO_SCENE
  const defaults = defaultShareState(sceneId)
  return {
    sceneId,
    persona: normalizeFromList(url.searchParams.get('d_persona'), DEMO_PERSONAS, defaults.persona),
    network: normalizeFromList(url.searchParams.get('d_net'), DEMO_NETWORK_MODES, defaults.network),
    asyncCase: normalizeFromList(url.searchParams.get('d_case'), DEMO_ASYNC_CASES, defaults.asyncCase),
    events: normalizeFromList(url.searchParams.get('d_events'), DEMO_EVENTS_MODES, defaults.events),
    cleanup: normalizeFromList(url.searchParams.get('d_cleanup'), DEMO_CLEANUP_SCENARIOS, defaults.cleanup),
    controlsHidden: url.searchParams.get('d_controls') === 'hidden',
  }
}

export function currentDemoShareState(): DemoShareState {
  if (typeof window === 'undefined') return defaultShareState(DEFAULT_DEMO_SCENE)
  return readDemoShareState(new URL(window.location.href))
}

/** Writes the canonical share contract and drops any stale demo parameters. */
export function applyDemoShareStateToSearchParams(params: URLSearchParams, state: DemoShareState) {
  params.delete('demo')
  for (const key of Array.from(params.keys())) {
    if (key.startsWith('d_')) params.delete(key)
  }
  params.set('demo', state.sceneId)
  const defaults = defaultShareState(state.sceneId)
  if (state.persona !== defaults.persona) params.set('d_persona', state.persona)
  if (state.network !== defaults.network) params.set('d_net', state.network)
  if (state.asyncCase !== defaults.asyncCase) params.set('d_case', state.asyncCase)
  if (state.events !== defaults.events) params.set('d_events', state.events)
  if (state.cleanup !== defaults.cleanup) params.set('d_cleanup', state.cleanup)
  if (state.controlsHidden) params.set('d_controls', 'hidden')
}

export function buildDemoHref(state: DemoShareState, basePath: string): string {
  const scene = demoScene(state.sceneId)
  const params = new URLSearchParams()
  applyDemoShareStateToSearchParams(params, state)
  const normalizedBase = normalizeAppBasePath(basePath)
  const scenePath = scene.path ? `${normalizedBase.replace(/\/$/, '')}${scene.path}` : normalizedBase
  return `${scenePath}?${params.toString()}`
}

export function withShareStatePatch(state: DemoShareState, patch: DemoShareStatePatch): DemoShareState {
  const next: DemoShareState = { ...state, ...patch }
  if (patch.sceneId && patch.sceneId !== state.sceneId) {
    // Switching scene re-seeds the scenario-specific defaults so the URL stays
    // minimal and the new page cannot inherit state that belongs to another scene.
    const defaults = defaultShareState(patch.sceneId)
    return { ...defaults, controlsHidden: state.controlsHidden }
  }
  return next
}

/** Demo query parameters are stripped before real navigation on the live app. */
export function copyDemoSearchParams(source: URLSearchParams, target: URLSearchParams) {
  for (const [key, value] of source.entries()) {
    if (key === 'demo' || key.startsWith('d_')) target.set(key, value)
  }
}
