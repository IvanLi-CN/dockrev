import { useEffect, useRef, useState } from 'react'
import { ShieldCheck, UserRound } from 'lucide-react'
import { Mono } from '../ui'
import { buildFallbackTopbarAuthIdentity, type TopbarAuthIdentity } from '../topbarAuthIdentity'
import { useHoverPinnedPopover } from './HoverPinnedPopover'

const HOVER_CAPABLE_QUERY = '(hover: hover) and (pointer: fine)'

function useHoverCapable(): boolean {
  const [hoverCapable, setHoverCapable] = useState(() => {
    if (typeof window === 'undefined') return true
    return window.matchMedia(HOVER_CAPABLE_QUERY).matches
  })

  useEffect(() => {
    if (typeof window === 'undefined') return
    const media = window.matchMedia(HOVER_CAPABLE_QUERY)
    const update = () => setHoverCapable(media.matches)
    update()
    media.addEventListener('change', update)
    return () => media.removeEventListener('change', update)
  }, [])

  return hoverCapable
}

export function TopbarUserIdentity(props: { authIdentity?: TopbarAuthIdentity | null }) {
  const authIdentity = props.authIdentity ?? buildFallbackTopbarAuthIdentity()
  const hoverCapable = useHoverCapable()
  const containerRef = useRef<HTMLDivElement | null>(null)
  const { close, contentProps, open, triggerProps } = useHoverPinnedPopover({
    hoverEnabled: hoverCapable,
  })

  useEffect(() => {
    if (!open) return

    const handlePointerDown = (event: PointerEvent) => {
      const container = containerRef.current
      if (!container?.contains(event.target as Node)) close()
    }
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') close()
    }

    document.addEventListener('pointerdown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [close, open])

  return (
    <div className="topbarUserSlot" ref={containerRef}>
      <button
        type="button"
        className="chipStatic chipStaticUser topbarUserTrigger"
        aria-haspopup="dialog"
        aria-label={`当前身份：${authIdentity.triggerLabel}`}
        title={authIdentity.triggerLabel}
        {...triggerProps}
      >
        <span className="topbarUserTriggerIcon" aria-hidden="true">
          <UserRound size={14} strokeWidth={2.1} />
        </span>
        <span className="topbarUserTriggerLabel">{authIdentity.triggerLabel}</span>
      </button>

      {open ? (
        <div
          className="topbarUserPopover"
          role="dialog"
          aria-label="当前身份"
          onPointerEnter={contentProps.onPointerEnter}
          onPointerLeave={contentProps.onPointerLeave}
        >
          <div className="topbarUserPopoverHeader">
            <div className="topbarUserPopoverTitleWrap">
              <div className="topbarUserPopoverTitle">当前身份</div>
              <div className="topbarUserPopoverSubtitle">用户信息与认证方式</div>
            </div>
            <div className="topbarUserPopoverMeta">
              <span className="topbarUserPopoverMetaIcon" aria-hidden="true">
                <ShieldCheck size={14} strokeWidth={2.1} />
              </span>
              <span>{authIdentity.authSource}</span>
            </div>
          </div>

          <div className="kv topbarUserPopoverKv">
            <div className="kvRow">
              <div className="label">当前用户</div>
              <div className="mono">{authIdentity.currentUser}</div>
            </div>
            <div className="kvRow">
              <div className="label">当前组</div>
              <div className="mono">{authIdentity.currentGroups}</div>
            </div>
            <div className="kvRow">
              <div className="label">认证来源</div>
              <div className="mono">{authIdentity.authSource}</div>
            </div>
            <div className="kvRow">
              <div className="label">鉴权模式</div>
              <div className="mono">{authIdentity.authorizationMode}</div>
            </div>
            <div className="kvRow">
              <div className="label">命中方式</div>
              <div className="mono">{authIdentity.matchedBy}</div>
            </div>
            <div className="kvRow">
              <div className="label">用户头</div>
              <div className="mono">
                <Mono>{authIdentity.forwardHeaderName}</Mono>
              </div>
            </div>
            <div className="kvRow">
              <div className="label">组头</div>
              <div className="mono">
                <Mono>{authIdentity.groupHeaderName}</Mono>
              </div>
            </div>
          </div>

          <div className="topbarUserPopoverHint muted">
            {hoverCapable ? '悬浮预览，点击固定；Esc 或点外部关闭。' : '点击查看详情；Esc 或点外部关闭。'}
          </div>
        </div>
      ) : null}
    </div>
  )
}
