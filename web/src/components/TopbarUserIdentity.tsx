import { useEffect, useRef, useState } from 'react'
import { ShieldCheck, UserRound } from 'lucide-react'
import { Mono, Popover, PopoverContent, PopoverTrigger } from '../ui'
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

export type UserIdentityPlacement = 'topbar' | 'sidebar'

export function TopbarUserIdentity(props: {
  authIdentity?: TopbarAuthIdentity | null
  placement?: UserIdentityPlacement
}) {
  const authIdentity = props.authIdentity ?? buildFallbackTopbarAuthIdentity()
  const placement = props.placement ?? 'topbar'
  const hoverCapable = useHoverCapable()
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const { contentProps, open, triggerProps } = useHoverPinnedPopover({
    hoverEnabled: hoverCapable,
  })

  return (
    <Popover open={open} onOpenChange={() => {}}>
      <div className={`topbarUserSlot topbarUserSlot${placement === 'sidebar' ? 'Sidebar' : 'Topbar'}`}>
        <PopoverTrigger asChild>
          <button
            type="button"
            className="chipStatic chipStaticUser topbarUserTrigger"
            ref={triggerRef}
            aria-label={`当前身份：${authIdentity.triggerLabel}`}
            title={authIdentity.triggerLabel}
            {...triggerProps}
          >
            <span className="topbarUserTriggerIcon" aria-hidden="true">
              {authIdentity.avatarUrl ? (
                <img className="topbarUserAvatarImage" src={authIdentity.avatarUrl} alt="" loading="lazy" decoding="async" />
              ) : (
                <UserRound size={14} strokeWidth={2.1} />
              )}
            </span>
            <span className="topbarUserTriggerLabel">{authIdentity.triggerLabel}</span>
          </button>
        </PopoverTrigger>

        <PopoverContent
          className="topbarUserPopover"
          role="dialog"
          aria-label="当前身份"
          side={placement === 'sidebar' ? 'right' : 'bottom'}
          align="end"
          onPointerEnter={contentProps.onPointerEnter}
          onPointerLeave={contentProps.onPointerLeave}
          onPointerDownOutside={(event) => {
            if (triggerRef.current?.contains(event.target as Node)) {
              event.preventDefault()
              return
            }
            contentProps.onPointerDownOutside?.(event)
          }}
          onFocusOutside={contentProps.onFocusOutside}
          onEscapeKeyDown={contentProps.onEscapeKeyDown}
          onOpenAutoFocus={contentProps.onOpenAutoFocus}
          onCloseAutoFocus={contentProps.onCloseAutoFocus}
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

        </PopoverContent>
      </div>
    </Popover>
  )
}
