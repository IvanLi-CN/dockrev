import { useEffect, useState } from 'react'
import { GitHubIcon, Mono, Popover, PopoverContent, PopoverTrigger } from '../ui'
import { useHoverPinnedPopover } from './HoverPinnedPopover'

const HOVER_CAPABLE_QUERY = '(hover: hover) and (pointer: fine)'

function useHoverCapable() {
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

function AppMetaContent(props: {
  versionDisplay: string
  versionHref: string | null
  popover?: boolean
}) {
  const className = props.popover
    ? 'sidebarAppMetaContent sidebarAppMetaContentPopover'
    : 'sidebarAppMetaContent'

  return (
    <div className={className} data-slot="sidebar-app-meta-content">
      {props.versionHref ? (
        <a
          className="sidebarAppMetaVersion"
          href={props.versionHref}
          target="_blank"
          rel="noopener noreferrer"
          aria-label={`Release on GitHub: ${props.versionDisplay}`}
          title={`Release: ${props.versionDisplay}`}
        >
          <Mono>{props.versionDisplay}</Mono>
        </a>
      ) : (
        <span className="sidebarAppMetaVersion sidebarAppMetaVersionDisabled">
          <Mono>{props.versionDisplay}</Mono>
        </span>
      )}
      <a
        className="sidebarAppMetaLink sidebarAppMetaGithub"
        href="https://github.com/IvanLi-CN/dockrev"
        target="_blank"
        rel="noopener noreferrer"
        aria-label="GitHub repository"
        title="GitHub: IvanLi-CN/dockrev"
      >
        <GitHubIcon />
      </a>
      <a
        className="sidebarAppMetaPowered"
        href="https://github.com/IvanLi-CN"
        target="_blank"
        rel="noopener noreferrer"
      >
        Powered by <Mono>Ivan Li</Mono>
      </a>
    </div>
  )
}

export function SidebarAppMeta(props: {
  collapsed: boolean
  versionDisplay: string
  versionHref: string | null
}) {
  const hoverCapable = useHoverCapable()
  const { contentProps, open, triggerProps } = useHoverPinnedPopover({
    hoverEnabled: hoverCapable,
  })

  if (!props.collapsed) {
    return (
      <div className="sidebarAppMeta" data-slot="sidebar-app-meta">
        <AppMetaContent
          versionDisplay={props.versionDisplay}
          versionHref={props.versionHref}
        />
      </div>
    )
  }

  return (
    <div className="sidebarAppMeta" data-slot="sidebar-app-meta">
      <Popover open={open} onOpenChange={() => {}}>
        <PopoverTrigger asChild>
          <button
            type="button"
            className="sidebarAppMetaTrigger"
            aria-label={`应用信息：版本 ${props.versionDisplay}，GitHub 仓库，Powered by Ivan Li`}
            title="应用信息"
            {...triggerProps}
          >
            <span className="sidebarAppMetaTriggerVersion" aria-hidden="true">
              <Mono>v</Mono>
            </span>
            <GitHubIcon />
          </button>
        </PopoverTrigger>
        <PopoverContent
          side="right"
          align="end"
          className="sidebarAppMetaPopover"
          {...contentProps}
        >
          <AppMetaContent
            versionDisplay={props.versionDisplay}
            versionHref={props.versionHref}
            popover
          />
        </PopoverContent>
      </Popover>
    </div>
  )
}
