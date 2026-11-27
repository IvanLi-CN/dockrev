import { useEffect, useRef, useState } from 'react'
import {
  Bug,
  Check,
  ChevronDown,
  ChevronUp,
  Database,
  MonitorCog,
  Moon,
  PanelRightClose,
  PanelRightOpen,
  RefreshCw,
  RotateCcw,
  Save,
  Settings2,
  Share2,
  ShieldCheck,
  SlidersHorizontal,
  Sun,
  UserRound,
  Wifi,
  WifiOff,
  X,
  Zap,
} from 'lucide-react'

import { dockrevBuildInfo } from '../buildInfo'
import { getThemePreference, setThemePreference, subscribeTheme, type ThemePreference } from '../theme'
import { Button, IconButton, Input, SelectField, Switch, ToggleGroup, ToggleGroupItem } from '../ui'
import {
  DEMO_ASYNC_CASE_OPTIONS,
  DEMO_CLEANUP_OPTIONS,
  DEMO_EVENTS_OPTIONS,
  DEMO_MAX_CUSTOM_DELAY_MS,
  DEMO_MOTION_OPTIONS,
  DEMO_NETWORK_OPTIONS,
  DEMO_PERSONA_OPTIONS,
  DEMO_SCENES,
  DEMO_SLOW_DELAY_MS,
  buildDemoHref,
  clampDemoDelayMs,
  demoNetworkMode,
  demoNetworkModePatch,
  demoScene,
  formatDemoDelaySummary,
} from './scenes'
import { readDemoInspectorLayout, resolveDemoInspectorTop, writeDemoInspectorLayout } from './inspectorLayout'
import { resetDemoSession, selectDemoScene, updateDemoShareState, useDemoRuntime } from './store'
import type {
  DemoAsyncCase,
  DemoCleanupScenario,
  DemoManagementEventsMode,
  DemoMotionPreference,
  DemoPersonaId,
  DemoShareState,
} from './types'
import type { DemoNetworkMode } from './scenes'
import './demoInspector.css'

const MOBILE_QUERY = '(max-width: 960px)'
const OPEN_STORAGE_KEY = 'dockrev:demo:inspector-open'
const ADVANCED_STORAGE_KEY = 'dockrev:demo:inspector-advanced'

const THEME_OPTIONS: ReadonlyArray<{ value: ThemePreference; label: string; Icon: typeof Sun }> = [
  { value: 'light', label: '浅色', Icon: Sun },
  { value: 'dark', label: '深色', Icon: Moon },
  { value: 'system', label: '跟随系统', Icon: MonitorCog },
]

const PERSONA_ICONS: Record<DemoPersonaId, typeof UserRound> = {
  allowed: UserRound,
  blocked: ShieldCheck,
}

function readMobile(): boolean {
  return typeof window !== 'undefined' && window.matchMedia(MOBILE_QUERY).matches
}

function useIsMobile() {
  const [mobile, setMobile] = useState(readMobile)
  useEffect(() => {
    if (typeof window === 'undefined') return
    const media = window.matchMedia(MOBILE_QUERY)
    const sync = () => setMobile(media.matches)
    sync()
    media.addEventListener('change', sync)
    return () => media.removeEventListener('change', sync)
  }, [])
  return mobile
}

function readStoredBoolean(key: string, fallback: boolean) {
  if (typeof window === 'undefined') return fallback
  try {
    const stored = window.sessionStorage.getItem(key)
    if (stored === 'true') return true
    if (stored === 'false') return false
  } catch {
    // storage failures fall back to the default
  }
  return fallback
}

function storeBoolean(key: string, value: boolean) {
  try {
    window.sessionStorage.setItem(key, String(value))
  } catch {
    // a blocked sessionStorage must not break the inspector
  }
}

function useThemePreference(): ThemePreference {
  const [preference, setPreference] = useState<ThemePreference>(() => getThemePreference())
  // The product theme control is the single source, so the inspector mirrors it.
  useEffect(() => {
    const unsubscribe = subscribeTheme(() => setPreference(getThemePreference()))
    return () => {
      unsubscribe()
    }
  }, [])
  return preference
}

function formatMutationTime(iso: string) {
  return iso.slice(11, 19)
}

function advancedItems(shareState: DemoShareState): string[] {
  return [
    shareState.delay === 'custom' ? `自定义延迟 ${clampDemoDelayMs(shareState.delayMs)} ms` : null,
    shareState.motion === 'reduce' ? '减少动效' : null,
  ].filter((item): item is string => item !== null)
}

function EnvironmentControls(props: {
  shareState: DemoShareState
  advancedOpen: boolean
  onToggleAdvanced: (open: boolean) => void
  themePreference: ThemePreference
}) {
  const { shareState } = props
  const networkMode = demoNetworkMode(shareState)
  const items = advancedItems(shareState)
  // The custom delay is a draft+commit field: it never writes a half-typed value
  // into the environment, and it re-seeds whenever the synced value changes.
  const [customDelayDraft, setCustomDelayDraft] = useState(() => String(clampDemoDelayMs(shareState.delayMs)))
  const [customDelayError, setCustomDelayError] = useState<string | null>(null)
  const [draftSource, setDraftSource] = useState(() => String(clampDemoDelayMs(shareState.delayMs)))
  const syncedDelay = String(clampDemoDelayMs(shareState.delayMs))
  if (shareState.delay !== 'custom' && draftSource !== syncedDelay) {
    setDraftSource(syncedDelay)
    setCustomDelayDraft(syncedDelay)
  }
  const customDelayInputRef = useRef<HTMLInputElement | null>(null)

  const commitCustomDelay = () => {
    const value = /^\d+$/.test(customDelayDraft) ? Number(customDelayDraft) : Number.NaN
    if (!Number.isSafeInteger(value) || value < 0 || value > DEMO_MAX_CUSTOM_DELAY_MS) {
      setCustomDelayError('请输入 0–30000 之间的整数毫秒数。')
      return
    }
    setCustomDelayError(null)
    updateDemoShareState({ delay: 'custom', delayMs: value })
  }

  const toggleCustomDelay = (checked: boolean) => {
    setCustomDelayError(null)
    if (!checked) {
      updateDemoShareState({ delay: 'normal' })
      return
    }
    props.onToggleAdvanced(true)
    const parsed = /^\d+$/.test(customDelayDraft) ? Number(customDelayDraft) : Number.NaN
    const delayMs =
      Number.isSafeInteger(parsed) && parsed >= 0 && parsed <= DEMO_MAX_CUSTOM_DELAY_MS ? parsed : shareState.delayMs
    updateDemoShareState({ delay: 'custom', delayMs })
  }

  // Enabling custom mode is an explicit intent to edit it, so the field takes focus.
  useEffect(() => {
    if (!props.advancedOpen || shareState.delay !== 'custom') return
    customDelayInputRef.current?.focus()
  }, [props.advancedOpen, shareState.delay])
  const advancedSummary = items.length > 0 ? items.join(' · ') : props.advancedOpen ? '收起设置' : '展开设置'

  return (
    <section className="demoInspectorGroup" aria-labelledby="demo-inspector-global-heading">
      <h3 id="demo-inspector-global-heading" className="demoInspectorGroupHeading">
        <Settings2 aria-hidden="true" size={14} />
        全局环境
      </h3>

      <div className="demoInspectorSetting">
        <span className="demoInspectorSettingLabel">
          <UserRound aria-hidden="true" size={13} />
          模拟鉴权
        </span>
        <ToggleGroup
          className="demoInspectorSegmented"
          onValueChange={(value) => {
            if (value) updateDemoShareState({ persona: value as DemoPersonaId })
          }}
          type="single"
          value={shareState.persona}
          aria-label="模拟鉴权"
        >
          {DEMO_PERSONA_OPTIONS.map((option) => {
            const Icon = PERSONA_ICONS[option.value]
            return (
              <ToggleGroupItem
                className="demoInspectorSegment"
                key={option.value}
                value={option.value}
                size="sm"
                variant="outline"
                aria-label={option.label}
                data-testid={`demo-persona-${option.value}`}
              >
                <Icon aria-hidden="true" size={12} />
                {option.label}
              </ToggleGroupItem>
            )
          })}
        </ToggleGroup>
        <small className="demoInspectorSettingNote">
          {shareState.persona === 'blocked'
            ? '未命中 allowlist：所有 API 返回 401，控制台不可用。'
            : '命中 allowlist：可使用全部功能。'}
        </small>
      </div>

      <div className="demoInspectorSetting">
        <span className="demoInspectorSettingLabel">
          {networkMode === 'offline' ? (
            <WifiOff aria-hidden="true" size={13} />
          ) : (
            <Wifi aria-hidden="true" size={13} />
          )}
          网络与延迟
        </span>
        <ToggleGroup
          className="demoInspectorSegmented"
          onValueChange={(value) => {
            if (value) updateDemoShareState(demoNetworkModePatch(shareState, value as Exclude<DemoNetworkMode, 'custom'>))
          }}
          type="single"
          value={networkMode === 'custom' ? '' : networkMode}
          aria-label="网络与延迟"
        >
          {DEMO_NETWORK_OPTIONS.map((option) => (
            <ToggleGroupItem
              className="demoInspectorSegment"
              key={option.value}
              value={option.value}
              size="sm"
              variant="outline"
              aria-label={option.label}
              data-testid={`demo-network-${option.value}`}
            >
              {option.label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        {networkMode === 'custom' ? (
          <button
            className="demoInspectorCustomDelay"
            onClick={() => props.onToggleAdvanced(true)}
            type="button"
          >
            {formatDemoDelaySummary(shareState)}
            <ChevronDown aria-hidden="true" size={13} />
          </button>
        ) : (
          <small className="demoInspectorSettingNote">
            {networkMode === 'offline'
              ? '所有请求以连接失败结束，不会回退真实后端。'
              : networkMode === 'slow'
                ? `每次请求额外 +${DEMO_SLOW_DELAY_MS} ms。`
                : '请求立即返回。'}
          </small>
        )}
      </div>

      <div className="demoInspectorSetting">
        <span className="demoInspectorSettingLabel">
          {props.themePreference === 'dark' ? (
            <Moon aria-hidden="true" size={13} />
          ) : props.themePreference === 'light' ? (
            <Sun aria-hidden="true" size={13} />
          ) : (
            <MonitorCog aria-hidden="true" size={13} />
          )}
          产品主题
        </span>
        <ToggleGroup
          className="demoInspectorSegmented"
          onValueChange={(value) => {
            if (value) setThemePreference(value as ThemePreference)
          }}
          type="single"
          value={props.themePreference}
          aria-label="产品主题"
        >
          {THEME_OPTIONS.map(({ value, label, Icon }) => (
            <ToggleGroupItem
              className="demoInspectorSegment"
              key={value}
              value={value}
              size="sm"
              variant="outline"
              aria-label={label}
              data-testid={`demo-theme-${value}`}
            >
              <Icon aria-hidden="true" size={12} />
              {label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>

      <div className="demoInspectorAdvanced">
        <button
          aria-controls="demo-inspector-advanced-content"
          aria-expanded={props.advancedOpen}
          className="demoInspectorAdvancedTrigger"
          onClick={() => props.onToggleAdvanced(!props.advancedOpen)}
          type="button"
        >
          <span className="demoInspectorAdvancedLabel">
            <SlidersHorizontal aria-hidden="true" size={13} />
            高级
            {items.length > 0 ? ` · ${items.length}` : ''}
          </span>
          <span className="demoInspectorAdvancedSummary">
            {advancedSummary}
            {props.advancedOpen ? <ChevronUp aria-hidden="true" size={13} /> : <ChevronDown aria-hidden="true" size={13} />}
          </span>
        </button>
        {props.advancedOpen ? (
          <div className="demoInspectorAdvancedContent" id="demo-inspector-advanced-content">
            <div className="demoInspectorAdvancedRow">
              <div className="demoInspectorAdvancedText">
                <span className="demoInspectorSettingLabel">自定义延迟</span>
                <small className="demoInspectorSettingNote">启用后使用下方毫秒数。</small>
              </div>
              <Switch
                aria-label="启用自定义请求延迟"
                checked={shareState.delay === 'custom'}
                onChange={toggleCustomDelay}
              />
            </div>
            <label className="demoInspectorDelayInput" htmlFor="demo-custom-delay">
              <span>附加延迟（毫秒）</span>
              <Input
                aria-describedby={customDelayError ? 'demo-custom-delay-error' : undefined}
                aria-invalid={customDelayError ? 'true' : undefined}
                id="demo-custom-delay"
                inputMode="numeric"
                max={DEMO_MAX_CUSTOM_DELAY_MS}
                min={0}
                onChange={(event) => setCustomDelayDraft(event.currentTarget.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') commitCustomDelay()
                }}
                ref={customDelayInputRef}
                step={1}
                type="number"
                value={customDelayDraft}
              />
              {customDelayError ? (
                <small className="demoInspectorError" id="demo-custom-delay-error">
                  {customDelayError}
                </small>
              ) : null}
            </label>
            <div className="demoInspectorAdvancedRow">
              <div className="demoInspectorAdvancedText">
                <span className="demoInspectorSettingLabel">动效偏好</span>
                <small className="demoInspectorSettingNote">同时作用于产品动效与环境背景。</small>
              </div>
              <ToggleGroup
                className="demoInspectorSegmented"
                onValueChange={(value) => {
                  if (value) updateDemoShareState({ motion: value as DemoMotionPreference })
                }}
                type="single"
                value={shareState.motion}
                aria-label="动效偏好"
              >
                {DEMO_MOTION_OPTIONS.map((option) => (
                  <ToggleGroupItem
                    className="demoInspectorSegment"
                    key={option.value}
                    value={option.value}
                    size="sm"
                    variant="outline"
                    aria-label={option.label}
                  >
                    {option.label}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
            </div>
          </div>
        ) : null}
      </div>
    </section>
  )
}

function SceneControls() {
  const { shareState } = useDemoRuntime()
  const scene = demoScene(shareState.sceneId)
  return (
    <section className="demoInspectorGroup" aria-labelledby="demo-inspector-scene-heading">
      <h3 className="demoInspectorGroupHeading" id="demo-inspector-scene-heading">
        <SlidersHorizontal aria-hidden="true" size={14} />
        Scene
      </h3>
      <label className="demoInspectorField">
        <span>场景</span>
        <SelectField
          ariaLabel="选择 Demo 场景"
          onChange={(value) => selectDemoScene(value as DemoShareState['sceneId'])}
          options={DEMO_SCENES.map((item) => ({ value: item.id, label: item.label }))}
          value={shareState.sceneId}
        />
        <small className="demoInspectorSettingNote">{scene.note}</small>
      </label>
    </section>
  )
}

function DataControls() {
  const { shareState } = useDemoRuntime()
  return (
    <section className="demoInspectorGroup" aria-labelledby="demo-inspector-data-heading">
      <h3 className="demoInspectorGroupHeading" id="demo-inspector-data-heading">
        <Database aria-hidden="true" size={14} />
        Data
      </h3>
      <label className="demoInspectorField">
        <span>异步数据态</span>
        <SelectField
          ariaLabel="异步数据态"
          onChange={(value) => updateDemoShareState({ asyncCase: value as DemoAsyncCase })}
          options={DEMO_ASYNC_CASE_OPTIONS.map((option) => ({ value: option.value, label: option.label }))}
          value={shareState.asyncCase}
        />
      </label>
      <label className="demoInspectorField">
        <span>管理事件流</span>
        <SelectField
          ariaLabel="管理事件流"
          onChange={(value) => updateDemoShareState({ events: value as DemoManagementEventsMode })}
          options={DEMO_EVENTS_OPTIONS.map((option) => ({ value: option.value, label: option.label }))}
          value={shareState.events}
        />
      </label>
      <label className="demoInspectorField">
        <span>清理场景</span>
        <SelectField
          ariaLabel="清理场景"
          onChange={(value) => updateDemoShareState({ cleanup: value as DemoCleanupScenario })}
          options={DEMO_CLEANUP_OPTIONS.map((option) => ({ value: option.value, label: option.label }))}
          value={shareState.cleanup}
        />
      </label>
    </section>
  )
}

function ActionControls() {
  return (
    <section className="demoInspectorGroup" aria-labelledby="demo-inspector-actions-heading">
      <h3 className="demoInspectorGroupHeading" id="demo-inspector-actions-heading">
        <Zap aria-hidden="true" size={14} />
        Actions
      </h3>
      <div className="demoInspectorActions">
        <Button onClick={() => updateDemoShareState({ asyncCase: 'cold' })} variant="ghost">
          <RefreshCw aria-hidden="true" size={13} />
          <span>冷启动</span>
        </Button>
        <Button onClick={() => updateDemoShareState({ asyncCase: 'error' })} variant="ghost">
          <Save aria-hidden="true" size={13} />
          <span>错误态</span>
        </Button>
        <Button onClick={() => resetDemoSession()} variant="ghost">
          <RotateCcw aria-hidden="true" size={13} />
          <span>重置</span>
        </Button>
      </div>
    </section>
  )
}

function ShareAndRecords(props: { shareState: DemoShareState }) {
  const runtime = useDemoRuntime()
  const shareUrl = buildDemoHref(props.shareState, import.meta.env.BASE_URL ?? '/')
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'manual'>('idle')
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(new URL(shareUrl, window.location.origin).toString())
      setCopyState('copied')
    } catch {
      setCopyState('manual')
    }
    window.setTimeout(() => setCopyState('idle'), 2400)
  }

  return (
    <details className="demoInspectorSecondary">
      <summary>
        <span>
          <Share2 aria-hidden="true" size={14} />
          分享与记录
        </span>
        <span className="demoInspectorSecondarySummary">
          {runtime.mutations.length > 0 ? `${runtime.mutations.length} 条记录` : '分享链接'}
          <ChevronDown aria-hidden="true" size={13} />
        </span>
      </summary>

      <section className="demoInspectorGroup">
        <h3 className="demoInspectorGroupHeading">Share state</h3>
        <div className="demoInspectorShareRow">
          <input
            aria-label="当前 Demo 分享状态 URL"
            onFocus={(event) => event.currentTarget.select()}
            readOnly
            value={shareUrl}
          />
          <IconButton aria-label="复制 Demo 分享状态 URL" onClick={() => void copy()} title="复制分享状态">
            {copyState === 'copied' ? <Check aria-hidden="true" size={15} /> : <Share2 aria-hidden="true" size={15} />}
          </IconButton>
        </div>
        <small className="demoInspectorSettingNote">
          {copyState === 'manual'
            ? '浏览器禁止自动复制，请聚焦输入框后手动复制。'
            : '场景、身份、连接、延迟、主题与动效写入 d_* 参数。'}
        </small>
      </section>

      <div aria-label="模拟写入记录" aria-live="polite" className="demoInspectorMutations" role="region">
        <div className="demoInspectorMutationsHeading">
          <span>Recent mutations</span>
        </div>
        {runtime.mutations.length === 0 ? (
          <p className="demoInspectorEmpty">暂无模拟操作</p>
        ) : (
          <ul>
            {runtime.mutations
              .slice()
              .reverse()
              .map((mutation) => (
                <li key={mutation.id}>
                  <span>{mutation.label}</span>
                  <small>{formatMutationTime(mutation.at)}</small>
                  <p>{mutation.detail}</p>
                </li>
              ))}
          </ul>
        )}
      </div>
    </details>
  )
}

function PanelBody(props: {
  shareState: DemoShareState
  advancedOpen: boolean
  onToggleAdvanced: (open: boolean) => void
  themePreference: ThemePreference
}) {
  return (
    <div className="demoInspectorScroll">
      <EnvironmentControls
        advancedOpen={props.advancedOpen}
        onToggleAdvanced={props.onToggleAdvanced}
        shareState={props.shareState}
        themePreference={props.themePreference}
      />
      <SceneControls />
      <DataControls />
      <ActionControls />
      <ShareAndRecords shareState={props.shareState} />
    </div>
  )
}

export function DemoInspector() {
  const runtime = useDemoRuntime()
  const shareState = runtime.shareState
  const mobile = useIsMobile()
  const themePreference = useThemePreference()

  const [isOpen, setIsOpen] = useState(() => readStoredBoolean(OPEN_STORAGE_KEY, false))
  const [advancedOpen, setAdvancedOpen] = useState(() => readStoredBoolean(ADVANCED_STORAGE_KEY, false))
  const [drawerOpen, setDrawerOpen] = useState(false)
  const [layoutState, setLayoutState] = useState(() => ({
    sceneId: shareState.sceneId,
    layout: readDemoInspectorLayout(shareState.sceneId),
  }))

  if (layoutState.sceneId !== shareState.sceneId) {
    setLayoutState({ sceneId: shareState.sceneId, layout: readDemoInspectorLayout(shareState.sceneId) })
  }
  const layout = layoutState.sceneId === shareState.sceneId ? layoutState.layout : readDemoInspectorLayout(shareState.sceneId)
  const setLayout = (update: (current: typeof layout) => typeof layout) =>
    setLayoutState({ sceneId: shareState.sceneId, layout: update(layout) })

  useEffect(() => {
    writeDemoInspectorLayout(shareState.sceneId, layout)
  }, [layout, shareState.sceneId])

  const setOpen = (next: boolean) => {
    setIsOpen(next)
    storeBoolean(OPEN_STORAGE_KEY, next)
  }
  const setAdvanced = (next: boolean) => {
    setAdvancedOpen(next)
    storeBoolean(ADVANCED_STORAGE_KEY, next)
  }

  const bodyProps = {
    shareState,
    advancedOpen,
    onToggleAdvanced: setAdvanced,
    themePreference,
  }

  const scene = demoScene(shareState.sceneId)

  if (shareState.controlsHidden) {
    return (
      <button
        className="demoInspectorReveal"
        onClick={() => updateDemoShareState({ controlsHidden: false })}
        type="button"
      >
        Demo
      </button>
    )
  }

  if (mobile) {
    return (
      <>
        <button
          aria-expanded={drawerOpen}
          aria-label="打开 Web Demo 调试控制面板"
          className="demoInspectorMobileBubble"
          onClick={() => setDrawerOpen(true)}
          type="button"
        >
          <Bug aria-hidden="true" size={16} />
          Demo
        </button>
        {drawerOpen ? (
          <div className="demoInspectorDrawerRoot" role="presentation">
            <div className="demoInspectorDrawerScrim" onClick={() => setDrawerOpen(false)} />
            <section
              aria-label="Demo 控制面板"
              className="demoInspectorPanel demoInspectorDrawer"
              data-demo-inspector="drawer"
            >
              <InspectorHeader
                onClose={() => setDrawerOpen(false)}
                pathname={shareState.sceneId === scene.id ? scene.path || '/' : '/'}
                version={dockrevBuildInfo().version}
              />
              <PanelBody {...bodyProps} />
            </section>
          </div>
        ) : null}
      </>
    )
  }

  if (!isOpen) {
    return (
      <button
        aria-label="打开 Web Demo 调试控制面板"
        className="demoInspectorBubble"
        onClick={() => setOpen(true)}
        type="button"
      >
        <PanelRightOpen aria-hidden="true" size={16} />
        <span>Demo</span>
        <ChevronUp aria-hidden="true" size={13} />
      </button>
    )
  }

  return (
    <section
      aria-label="Demo 控制面板"
      className="demoInspectorPanel"
      data-demo-inspector="floating"
      style={{
        [layout.side]: 16,
        top: resolveDemoInspectorTop(layout, typeof window === 'undefined' ? 900 : window.innerHeight, 560),
      } as React.CSSProperties}
    >
      <InspectorHeader
        onBubble={() => setOpen(false)}
        onDockToggle={() => setLayout((current) => ({ ...current, side: current.side === 'left' ? 'right' : 'left' }))}
        onClose={() => setOpen(false)}
        pathname={scene.path || '/'}
        side={layout.side}
        version={dockrevBuildInfo().version}
      />
      <PanelBody {...bodyProps} />
    </section>
  )
}

function InspectorHeader(props: {
  pathname: string
  version: string
  side?: 'left' | 'right'
  onClose: () => void
  onBubble?: () => void
  onDockToggle?: () => void
}) {
  return (
    <>
      <header className="demoInspectorHead">
        <div className="demoInspectorHeading">
          <span className="demoInspectorKicker">
            <Bug aria-hidden="true" size={13} />
            WEB DEMO
          </span>
          <h2>Inspector</h2>
          <p>构建时隔离 · 仅内存模拟</p>
        </div>
        <div className="demoInspectorHeadActions">
          {props.onDockToggle ? (
            <IconButton aria-label="切换停靠边" onClick={props.onDockToggle} title="切换停靠边">
              <PanelRightClose aria-hidden="true" size={16} />
            </IconButton>
          ) : null}
          {props.onBubble ? (
            <IconButton aria-label="收起 Demo 控制面板" onClick={props.onBubble} title="收起为气泡">
              <ChevronDown aria-hidden="true" size={16} />
            </IconButton>
          ) : null}
          <IconButton aria-label="关闭" onClick={props.onClose} title="关闭">
            <X aria-hidden="true" size={16} />
          </IconButton>
        </div>
      </header>
      <div className="demoInspectorStatusRow">
        <span className="demoInspectorStatus">
          <ShieldCheck aria-hidden="true" size={12} />
          SIMULATED
        </span>
        <code title={props.pathname}>{props.pathname}</code>
        <code className="demoInspectorVersion" title={`build ${props.version}`}>
          {props.version}
        </code>
      </div>
    </>
  )
}
