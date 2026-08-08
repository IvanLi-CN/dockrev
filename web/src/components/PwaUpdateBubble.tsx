import { Clock3, Download, RefreshCw } from 'lucide-react'
import { useRef, useState } from 'react'

import { usePwaStatus } from '../pwaStatus'
import { shouldHidePwaUpdateBubble } from '../pwaUpdateLifecycle'
import { Button } from '../ui'

function copyForPhase(phase: 'downloading' | 'ready' | 'failed') {
  switch (phase) {
    case 'downloading':
      return {
        title: '正在后台准备新版本',
        detail: '资源下载完成后即可切换，不会打断当前操作。',
      }
    case 'failed':
      return {
        title: '新版本未能准备完成',
        detail: '当前版本仍可继续使用，可在联网后重新检查。',
      }
    case 'ready':
      return {
        title: '新版本已准备就绪',
        detail: '立即更新，或在下一次切换页面时自动加载新版本。',
      }
  }
}

export function PwaUpdateBubble() {
  const rootRef = useRef<HTMLDivElement>(null)
  const [engaged, setEngaged] = useState(false)
  const {
    applyUpdate,
    checkForUpdates,
    dismissUpdate,
    isOnline,
    updatePhase,
    updatePromptVisible,
  } = usePwaStatus()

  const phase = updatePhase === 'idle' ? null : updatePhase
  const shouldHideForOffline = shouldHidePwaUpdateBubble({ engaged, isOnline, phase })
  if (!phase || !updatePromptVisible || shouldHideForOffline) return null

  const copy = copyForPhase(phase)
  const isDownloading = phase === 'downloading'
  const isFailed = phase === 'failed'

  return (
    <div
      ref={rootRef}
      className={`pwaUpdateBubble pwaUpdateBubble-${phase}`}
      role="status"
      aria-live="polite"
      aria-atomic="true"
      onPointerEnter={() => setEngaged(true)}
      onPointerLeave={() => setEngaged(false)}
      onFocusCapture={() => setEngaged(true)}
      onBlurCapture={() => {
        window.requestAnimationFrame(() => {
          if (!rootRef.current?.contains(document.activeElement)) setEngaged(false)
        })
      }}
    >
      <div className="pwaUpdateBubbleIcon" aria-hidden="true">
        {isDownloading ? <Download size={18} strokeWidth={2.2} /> : <RefreshCw size={18} strokeWidth={2.2} />}
      </div>
      <div className="pwaUpdateBubbleText">
        <strong>{copy.title}</strong>
        <span>{copy.detail}</span>
      </div>
      <div className="pwaUpdateBubbleActions">
        <Button onClick={dismissUpdate} variant="ghost">
          <span className="btnInlineContent">
            <Clock3 aria-hidden="true" size={14} strokeWidth={2.2} />
            稍后
          </span>
        </Button>
        {isFailed ? (
          <Button onClick={() => void checkForUpdates()} variant="primary">
            <span className="btnInlineContent">
              <RefreshCw aria-hidden="true" size={14} strokeWidth={2.2} />
              重新检查
            </span>
          </Button>
        ) : (
          <Button disabled={isDownloading} onClick={() => void applyUpdate()} variant="primary">
            <span className="btnInlineContent">
              <RefreshCw aria-hidden="true" size={14} strokeWidth={2.2} />
              立即更新
            </span>
          </Button>
        )}
      </div>
    </div>
  )
}
