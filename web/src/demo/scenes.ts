import { normalizeAppBasePath, stripAppBase } from '../appBase'
import type { DockrevApiScenario } from '../mock/dockrevMockApi/shared'
import { LEGACY_PERSONA_MAP } from './types'
import type {
  DemoAsyncCase,
  DemoCleanupScenario,
  DemoConnection,
  DemoDelayMode,
  DemoManagementEventsMode,
  DemoMotionPreference,
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
  defaults: Pick<DemoShareState, 'asyncCase' | 'events' | 'cleanup'>
  /**
   * Only the entry scene proposes a persona. Once a reviewer chooses one, the
   * environment owns it and switching scenes must not overwrite that choice.
   */
  entryPersona: DemoPersonaId
}

export const DEMO_PERSONA_OPTIONS: ReadonlyArray<{
  value: DemoPersonaId
  label: string
  note: string
}> = [
  { value: 'allowed', label: '已授权', note: '命中 allowlist，可使用全部功能' },
  { value: 'blocked', label: '未授权', note: '401 auth_required，全部功能不可用' },
]

export const DEMO_PERSONAS: readonly DemoPersonaId[] = DEMO_PERSONA_OPTIONS.map((option) => option.value)

export const DEMO_CONNECTION_OPTIONS: ReadonlyArray<{ value: DemoConnection; label: string }> = [
  { value: 'online', label: '在线' },
  { value: 'offline', label: '离线' },
]

/**
 * Connectivity and latency are one reviewer decision, so the inspector exposes a
 * single three-state control. `custom` is reached through the advanced section.
 * The share contract keeps `d_connection` and `d_delay` separate so existing
 * links and the runtime policy stay unchanged.
 */
export type DemoNetworkMode = 'normal' | 'slow' | 'offline' | 'custom'

export const DEMO_NETWORK_OPTIONS: ReadonlyArray<{
  value: Exclude<DemoNetworkMode, 'custom'>
  label: string
}> = [
  { value: 'normal', label: '正常' },
  { value: 'slow', label: '慢速' },
  { value: 'offline', label: '离线' },
]

export function demoNetworkMode(state: DemoShareState): DemoNetworkMode {
  if (state.connection === 'offline') return 'offline'
  if (state.delay === 'custom') return 'custom'
  return state.delay === 'slow' ? 'slow' : 'normal'
}

/**
 * Applying a mode only touches the fields it owns, so leaving offline restores
 * the delay that was in effect instead of resetting it.
 */
export function demoNetworkModePatch(
  state: DemoShareState,
  mode: Exclude<DemoNetworkMode, 'custom'>,
): Pick<DemoShareState, 'connection' | 'delay'> {
  if (mode === 'offline') return { connection: 'offline', delay: state.delay === 'custom' ? 'normal' : state.delay }
  return { connection: 'online', delay: mode }
}

export const DEMO_CONNECTIONS: readonly DemoConnection[] = DEMO_CONNECTION_OPTIONS.map((option) => option.value)

export const DEMO_DELAY_OPTIONS: ReadonlyArray<{ value: Exclude<DemoDelayMode, 'custom'>; label: string }> = [
  { value: 'normal', label: '正常' },
  { value: 'slow', label: '慢速' },
]

export const DEMO_MOTION_OPTIONS: ReadonlyArray<{ value: DemoMotionPreference; label: string }> = [
  { value: 'system', label: '跟随系统' },
  { value: 'reduce', label: '减少动效' },
]

export const DEMO_MOTIONS: readonly DemoMotionPreference[] = DEMO_MOTION_OPTIONS.map((option) => option.value)

export const DEMO_ASYNC_CASE_OPTIONS: ReadonlyArray<{ value: DemoAsyncCase; label: string }> = [
  { value: 'default', label: '默认' },
  { value: 'cold', label: '冷启动' },
  { value: 'cache-refresh', label: '缓存刷新' },
  { value: 'error', label: '错误覆盖' },
  { value: 'job-detail-retry', label: '任务详情重试' },
]

export const DEMO_ASYNC_CASES: readonly DemoAsyncCase[] = DEMO_ASYNC_CASE_OPTIONS.map((option) => option.value)

export const DEMO_EVENTS_OPTIONS: ReadonlyArray<{ value: DemoManagementEventsMode; label: string }> = [
  { value: 'default', label: '默认' },
  { value: 'reconnecting', label: '重连中' },
]

export const DEMO_EVENTS_MODES: readonly DemoManagementEventsMode[] = DEMO_EVENTS_OPTIONS.map((option) => option.value)

export const DEMO_CLEANUP_OPTIONS: ReadonlyArray<{ value: DemoCleanupScenario; label: string }> = [
  { value: 'cleanup-console-storage-normal', label: '常规' },
  { value: 'cleanup-console-confirm-pending', label: '确认待处理' },
  { value: 'cleanup-console-confirm-failed', label: '确认失败' },
]

export const DEMO_CLEANUP_SCENARIOS: readonly DemoCleanupScenario[] = DEMO_CLEANUP_OPTIONS.map(
  (option) => option.value,
)

/** Slow mode adds one fixed, documented latency on top of the mock's own timing. */
export const DEMO_SLOW_DELAY_MS = 1500
export const DEMO_MAX_CUSTOM_DELAY_MS = 30_000

const DEFAULT_SCENE_DEFAULTS: DemoScene['defaults'] = {
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
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
    entryPersona: 'allowed',
  },
  {
    id: 'services',
    label: 'Services',
    note: '运维大盘与候选版本',
    path: '/services',
    fixtureScenario: 'settings-configured',
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
    entryPersona: 'allowed',
  },
  {
    id: 'service-actions',
    label: 'Service actions',
    note: '更新进行中与生命周期动作',
    path: '/services/stack-prod/svc-prod-api',
    fixtureScenario: 'service-action-progress',
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
    entryPersona: 'allowed',
  },
  {
    id: 'service-versions',
    label: 'Service versions',
    note: '版本历史与回滚目标',
    path: '/services/stack-prod/svc-prod-api/versions',
    fixtureScenario: 'service-selected-version-updates',
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
    entryPersona: 'allowed',
  },
  {
    id: 'queue',
    label: 'Queue',
    note: '任务队列与 GHCR 假写',
    path: '/queue',
    fixtureScenario: 'queue-mixed',
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
    entryPersona: 'allowed',
  },
  {
    id: 'queue-logs',
    label: 'Queue logs',
    note: '长日志与实时输出',
    path: '/queue/job-live-long',
    fixtureScenario: 'queue-long-logs',
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
    entryPersona: 'allowed',
  },
  {
    id: 'settings',
    label: 'Settings',
    note: '鉴权、通知与集成',
    path: '/settings',
    fixtureScenario: 'settings-configured',
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
    entryPersona: 'allowed',
  },
  {
    id: 'ghcr-registry',
    label: 'GHCR registry',
    note: 'Webhook 维护与假写',
    path: '/settings/ghcr-webhooks',
    fixtureScenario: 'settings-configured',
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
    entryPersona: 'allowed',
  },
  {
    id: 'cleanup',
    label: 'Cleanup',
    note: '扫描规则与 apply',
    path: '/cleanup',
    fixtureScenario: 'cleanup-console-storage-normal',
    defaults: { ...DEFAULT_SCENE_DEFAULTS, cleanup: 'cleanup-console-storage-normal' },
    entryPersona: 'allowed',
  },
  {
    id: 'deploy-check',
    label: 'Deploy check',
    note: '部署检查报告',
    path: '/deploy-check',
    fixtureScenario: 'settings-configured',
    defaults: { ...DEFAULT_SCENE_DEFAULTS },
    entryPersona: 'allowed',
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

function normalizePersona(value: string | null | undefined, fallback: DemoPersonaId): DemoPersonaId {
  const mapped = value != null ? LEGACY_PERSONA_MAP[value] ?? value : null
  return normalizeFromList(mapped, DEMO_PERSONAS, fallback)
}

/** The environment defaults are independent of the current scene. */
export function defaultDemoEnvironment(sceneId: DemoSceneId) {
  return {
    persona: demoScene(sceneId).entryPersona,
    connection: 'online' as const,
    delay: 'normal' as const,
    delayMs: DEMO_SLOW_DELAY_MS,
    motion: 'system' as const,
  }
}

export function defaultShareState(sceneId: DemoSceneId): DemoShareState {
  const scene = demoScene(sceneId)
  return {
    sceneId,
    ...defaultDemoEnvironment(sceneId),
    ...scene.defaults,
    controlsHidden: false,
  }
}

export function clampDemoDelayMs(value: number): number {
  if (!Number.isFinite(value)) return 0
  return Math.min(Math.max(Math.round(value), 0), DEMO_MAX_CUSTOM_DELAY_MS)
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

/**
 * Valid URL fields override the session; missing fields keep the session value
 * and only then fall back to entry defaults.
 */
export function readDemoShareState(url: URL, session?: DemoShareState | null): DemoShareState {
  const requestedScene = url.searchParams.get('demo')
  const requested = isSceneId(requestedScene) ? requestedScene : null
  const sceneId = demoSceneForPath(url.pathname, requested) ?? requested ?? session?.sceneId ?? DEFAULT_DEMO_SCENE
  const defaults = session && session.sceneId === sceneId ? session : defaultShareState(sceneId)

  const delayParam = url.searchParams.get('d_delay')
  const delayMsParam = url.searchParams.get('d_delay_ms')
  const parsedDelayMs = delayMsParam != null && /^\d+$/.test(delayMsParam) ? Number(delayMsParam) : null
  const validDelayMs = parsedDelayMs != null && parsedDelayMs <= DEMO_MAX_CUSTOM_DELAY_MS

  return {
    sceneId,
    persona: normalizePersona(url.searchParams.get('d_persona'), defaults.persona),
    connection: normalizeFromList(url.searchParams.get('d_connection'), DEMO_CONNECTIONS, defaults.connection),
    delay: normalizeFromList(delayParam, ['normal', 'slow', 'custom'] as const, defaults.delay),
    delayMs: validDelayMs ? parsedDelayMs : defaults.delayMs,
    motion: normalizeFromList(url.searchParams.get('d_motion'), DEMO_MOTIONS, defaults.motion),
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

function sameValue(a: unknown, b: unknown): boolean {
  return a === b
}

/** Writes the canonical share contract and drops stale demo parameters. */
export function applyDemoShareStateToSearchParams(params: URLSearchParams, state: DemoShareState) {
  params.delete('demo')
  for (const key of Array.from(params.keys())) {
    if (key.startsWith('d_')) params.delete(key)
  }
  params.set('demo', state.sceneId)
  const defaults = defaultShareState(state.sceneId)
  if (!sameValue(state.persona, defaults.persona)) params.set('d_persona', state.persona)
  if (!sameValue(state.connection, defaults.connection)) params.set('d_connection', state.connection)
  if (!sameValue(state.delay, defaults.delay)) params.set('d_delay', state.delay)
  if (!sameValue(state.delayMs, defaults.delayMs)) params.set('d_delay_ms', String(state.delayMs))
  if (!sameValue(state.motion, defaults.motion)) params.set('d_motion', state.motion)
  if (!sameValue(state.asyncCase, defaults.asyncCase)) params.set('d_case', state.asyncCase)
  if (!sameValue(state.events, defaults.events)) params.set('d_events', state.events)
  if (!sameValue(state.cleanup, defaults.cleanup)) params.set('d_cleanup', state.cleanup)
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

/**
 * Scene switches only replace scene-owned state. Environment choices belong to
 * the reviewer and survive navigation.
 */
export function withShareStatePatch(state: DemoShareState, patch: DemoShareStatePatch): DemoShareState {
  if (patch.sceneId && patch.sceneId !== state.sceneId) {
    const scene = demoScene(patch.sceneId)
    return {
      ...state,
      ...scene.defaults,
      ...patch,
      sceneId: patch.sceneId,
    }
  }
  return { ...state, ...patch }
}

/** Effective extra latency for every demo-intercepted product request. */
export function demoRequestDelayMs(state: DemoShareState): number {
  if (state.connection === 'offline') return 0
  if (state.delay === 'custom') return clampDemoDelayMs(state.delayMs)
  return state.delay === 'slow' ? DEMO_SLOW_DELAY_MS : 0
}

export function formatDemoDelaySummary(state: DemoShareState): string {
  if (state.connection === 'offline') return '离线 · 请求直接失败'
  if (state.delay === 'custom') return `自定义 · +${clampDemoDelayMs(state.delayMs)} ms`
  return state.delay === 'slow' ? `慢速 · +${DEMO_SLOW_DELAY_MS} ms` : '正常 · +0 ms'
}
