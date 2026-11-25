import { useEffect, useState } from 'react'
import { ChevronLeft, ChevronRight, Copy, RotateCcw, X } from 'lucide-react'

import { Button, IconButton, SelectField } from '../ui'
import { dockrevBuildInfo } from '../buildInfo'
import { buildDemoHref, DEMO_SCENES, demoScene } from './scenes'
import { demoShareStateSummary, readDemoInspectorLayout, resolveDemoInspectorTop, writeDemoInspectorLayout } from './inspectorLayout'
import { resetDemoSession, selectDemoScene, updateDemoShareState, useDemoRuntime } from './store'
import type { DemoAsyncCase, DemoCleanupScenario, DemoManagementEventsMode, DemoNetworkMode, DemoPersonaId } from './types'
import './demoInspector.css'

const MOBILE_QUERY = '(max-width: 960px)'

const PERSONA_OPTIONS = [
  { value: 'anonymous', label: '匿名访客' },
  { value: 'admin', label: '维护者' },
] as const satisfies ReadonlyArray<{ value: DemoPersonaId; label: string }>

const NETWORK_OPTIONS = [
  { value: 'normal', label: '正常' },
  { value: 'slow', label: '慢速 (3s)' },
  { value: 'faulty', label: '失败重试' },
] as const satisfies ReadonlyArray<{ value: DemoNetworkMode; label: string }>

const ASYNC_OPTIONS = [
  { value: 'default', label: '默认' },
  { value: 'cold', label: '冷启动' },
  { value: 'cache-refresh', label: '缓存刷新' },
  { value: 'error', label: '错误覆盖' },
  { value: 'job-detail-retry', label: '任务详情重试' },
] as const satisfies ReadonlyArray<{ value: DemoAsyncCase; label: string }>

const EVENTS_OPTIONS = [
  { value: 'default', label: '默认' },
  { value: 'reconnecting', label: '重连中' },
] as const satisfies ReadonlyArray<{ value: DemoManagementEventsMode; label: string }>

const CLEANUP_OPTIONS = [
  { value: 'cleanup-console-storage-normal', label: '常规' },
  { value: 'cleanup-console-confirm-pending', label: '确认待处理' },
  { value: 'cleanup-console-confirm-failed', label: '确认失败' },
] as const satisfies ReadonlyArray<{ value: DemoCleanupScenario; label: string }>

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

function SceneControls() {
  const { shareState } = useDemoRuntime()
  return (
    <>
      <div className="demoInspectorSection">
        <div className="demoInspectorLabel">场景</div>
        <div className="demoInspectorSceneList">
          {DEMO_SCENES.map((scene) => (
            <button
              aria-pressed={scene.id === shareState.sceneId}
              className="demoInspectorSceneButton"
              key={scene.id}
              onClick={() => selectDemoScene(scene.id)}
              type="button"
            >
              <span>{scene.label}</span>
              <span className="demoInspectorSceneNote">{scene.note}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="demoInspectorSection">
        <div className="demoInspectorLabel">身份</div>
        <SelectField
          ariaLabel="Demo persona"
          onChange={(persona) => updateDemoShareState({ persona })}
          options={PERSONA_OPTIONS.map((option) => ({ value: option.value, label: option.label }))}
          value={shareState.persona}
        />
      </div>

      <div className="demoInspectorSection">
        <div className="demoInspectorLabel">网络</div>
        <SelectField
          ariaLabel="Demo network"
          onChange={(network) => updateDemoShareState({ network })}
          options={NETWORK_OPTIONS.map((option) => ({ value: option.value, label: option.label }))}
          value={shareState.network}
        />
      </div>

      <div className="demoInspectorSection">
        <div className="demoInspectorLabel">数据</div>
        <div className="demoInspectorFieldStack">
          <SelectField
            ariaLabel="Demo async case"
            onChange={(asyncCase) => updateDemoShareState({ asyncCase })}
            options={ASYNC_OPTIONS.map((option) => ({ value: option.value, label: option.label }))}
            value={shareState.asyncCase}
          />
          <SelectField
            ariaLabel="Demo management events"
            onChange={(events) => updateDemoShareState({ events })}
            options={EVENTS_OPTIONS.map((option) => ({ value: option.value, label: option.label }))}
            value={shareState.events}
          />
          <SelectField
            ariaLabel="Demo cleanup scenario"
            onChange={(cleanup) => updateDemoShareState({ cleanup })}
            options={CLEANUP_OPTIONS.map((option) => ({ value: option.value, label: option.label }))}
            value={shareState.cleanup}
          />
        </div>
      </div>
    </>
  )
}

function MutationList() {
  const { mutations } = useDemoRuntime()
  return (
    <div className="demoInspectorSection">
      <div className="demoInspectorLabel">模拟写入</div>
      {mutations.length === 0 ? (
        <p className="demoInspectorEmpty">当前会话还没有写入。操作会即时回显并记录在这里。</p>
      ) : (
        <ol className="demoInspectorMutations">
          {mutations
            .slice()
            .reverse()
            .map((record) => (
              <li key={record.id}>
                <span className="demoInspectorMutationLabel">{record.label}</span>
                <span className="demoInspectorMutationTime">{record.at.slice(11, 19)}</span>
              </li>
            ))}
        </ol>
      )}
    </div>
  )
}

function ShareControls() {
  const runtime = useDemoRuntime()
  const shareHref = buildDemoHref(runtime.shareState, import.meta.env.BASE_URL ?? '/')
  const [copied, setCopied] = useState<'idle' | 'ok' | 'failed'>('idle')
  const copyToClipboard = async () => {
    const absolute = new URL(shareHref, window.location.origin).toString()
    try {
      await navigator.clipboard.writeText(absolute)
      setCopied('ok')
    } catch {
      setCopied('failed')
    }
    window.setTimeout(() => setCopied('idle'), 2000)
  }
  return (
    <div className="demoInspectorSection">
      <div className="demoInspectorLabel">分享</div>
      <code className="demoInspectorShare">{shareHref}</code>
      <div className="demoInspectorActionRow">
        <Button onClick={() => void copyToClipboard()} variant="ghost">
          <Copy aria-hidden="true" size={14} />
          <span>{copied === 'ok' ? '已复制' : copied === 'failed' ? '复制失败' : '复制链接'}</span>
        </Button>
        <Button
          onClick={() => updateDemoShareState({ controlsHidden: true })}
          variant="ghost"
        >
          <span>隐藏面板</span>
        </Button>
      </div>
    </div>
  )
}

function PanelBody() {
  return (
    <div className="demoInspectorBody">
      <SceneControls />
      <MutationList />
      <ShareControls />
      <div className="demoInspectorFooter">
        <span>mock-only · 会话内持久化</span>
        <span>{dockrevBuildInfo().version}</span>
      </div>
    </div>
  )
}

export function DemoInspector() {
  const runtime = useDemoRuntime()
  const mobile = useIsMobile()
  const sceneId = runtime.shareState.sceneId
  const [layoutState, setLayoutState] = useState(() => ({
    sceneId,
    layout: readDemoInspectorLayout(sceneId),
  }))
  const [drawerOpen, setDrawerOpen] = useState(false)

  // Adjusting state during render keeps each scene's own geometry without an
  // extra effect pass.
  if (layoutState.sceneId !== sceneId) {
    setLayoutState({ sceneId, layout: readDemoInspectorLayout(sceneId) })
  }
  const layout = layoutState.sceneId === sceneId ? layoutState.layout : readDemoInspectorLayout(sceneId)
  const setLayout = (update: (current: typeof layout) => typeof layout) =>
    setLayoutState({ sceneId, layout: update(layout) })

  useEffect(() => {
    writeDemoInspectorLayout(sceneId, layout)
  }, [layout, sceneId])

  const scene = demoScene(sceneId)

  if (runtime.shareState.controlsHidden) {
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
          aria-label="打开 Demo 控制面板"
          className="demoInspectorMobileBubble"
          onClick={() => setDrawerOpen(true)}
          type="button"
        >
          DEMO
        </button>
        {drawerOpen ? (
          <div className="demoInspectorDrawerRoot" role="presentation">
            <div className="demoInspectorDrawerScrim" onClick={() => setDrawerOpen(false)} />
            <section
              aria-label="Demo 控制面板"
              className="demoInspectorDrawer"
              data-demo-inspector="drawer"
            >
              <header className="demoInspectorHead">
                <div>
                  <div className="demoInspectorEyebrow">Public demo · {scene.label}</div>
                  <div className="demoInspectorTitle">{demoShareStateSummary(runtime.shareState)}</div>
                </div>
                <IconButton aria-label="关闭 Demo 控制面板" onClick={() => setDrawerOpen(false)} title="关闭" variant="ghost">
                  <X aria-hidden="true" size={16} />
                </IconButton>
              </header>
              <PanelBody />
            </section>
          </div>
        ) : null}
      </>
    )
  }

  if (layout.collapsed) {
    return (
      <button
        className="demoInspectorBubble"
        onClick={() => setLayout((current) => ({ ...current, collapsed: false }))}
        style={{ [layout.side]: 0 } as React.CSSProperties}
        type="button"
      >
        <ChevronLeft aria-hidden="true" size={14} />
        <span>DEMO</span>
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
        top: resolveDemoInspectorTop(layout, typeof window === 'undefined' ? 900 : window.innerHeight, 520),
      } as React.CSSProperties}
    >
      <header className="demoInspectorHead">
        <div>
          <div className="demoInspectorEyebrow">Public demo · {scene.label}</div>
          <div className="demoInspectorTitle">{demoShareStateSummary(runtime.shareState)}</div>
        </div>
        <div className="demoInspectorHeadActions">
          <IconButton
            aria-label={layout.side === 'left' ? '停靠到右侧' : '停靠到左侧'}
            onClick={() => setLayout((current) => ({ ...current, side: current.side === 'left' ? 'right' : 'left' }))}
            title="切换停靠边"
            variant="ghost"
          >
            {layout.side === 'left' ? <ChevronRight aria-hidden="true" size={16} /> : <ChevronLeft aria-hidden="true" size={16} />}
          </IconButton>
          <IconButton
            aria-label="收起 Demo 控制面板"
            onClick={() => setLayout((current) => ({ ...current, collapsed: true }))}
            title="收起"
            variant="ghost"
          >
            <ChevronRight aria-hidden="true" size={16} />
          </IconButton>
        </div>
      </header>
      <PanelBody />
      <div className="demoInspectorReset">
        <Button onClick={() => resetDemoSession()} variant="ghost">
          <RotateCcw aria-hidden="true" size={14} />
          <span>重置 Seed</span>
        </Button>
        <span className="demoInspectorResetNote">清空当前会话的 fixture 与写入记录</span>
      </div>
    </section>
  )
}
