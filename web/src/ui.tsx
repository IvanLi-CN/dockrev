import { useEffect, useRef, useState, type ComponentProps, type CSSProperties, type ReactNode } from 'react'
import mdiDocker from '@iconify-icons/mdi/docker'
import mdiGithubBox from '@iconify-icons/mdi/github-box'
import mdiGithubCircle from '@iconify-icons/mdi/github-circle'
import mdiGitlab from '@iconify-icons/mdi/gitlab'
import mdiPackageVariantClosed from '@iconify-icons/mdi/package-variant-closed'
import mdiSourceRepository from '@iconify-icons/mdi/source-repository'
import { Icon } from '@iconify/react'
import type { IconifyIcon } from '@iconify/types'
import { Badge } from '@/components/ui/badge'
import { Button as PrimitiveButton } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Switch as PrimitiveSwitch } from '@/components/ui/switch'
import { Toggle } from '@/components/ui/toggle'
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import { octoRillMarkUrl } from './publicAssetUrls'
export { Input } from '@/components/ui/input'
export { Label } from '@/components/ui/label'
export {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
export {
  Drawer,
  DrawerClose,
  DrawerContent,
  DrawerDescription,
  DrawerFooter,
  DrawerHandle,
  DrawerHeader,
  DrawerOverlay,
  DrawerPortal,
  DrawerTitle,
  DrawerTrigger,
} from '@/components/ui/drawer'
export { ScrollArea, ScrollBar } from '@/components/ui/scroll-area'
export { OverlayScrollArea } from '@/components/ui/overlay-scroll-area'
export {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
export { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
export { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
export { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
export { Popover, PopoverAnchor, PopoverContent, PopoverTrigger } from '@/components/ui/popover'

import type { Service } from './api'
import { DiscoveryHistoryPopover } from './components/DiscoveryHistoryPopover'
import { noteFor, statusDotClass, statusIcon, statusLabel, type RowStatus } from './updateStatus'

export function ArrowRightIcon(props: { className?: string }) {
  return (
    <svg
      className={props.className}
      viewBox="0 0 16 16"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M3 8h9" />
      <path d="M9 4l4 4-4 4" />
    </svg>
  )
}

export function RefreshIcon(props: { className?: string }) {
  return (
    <svg className={props.className} viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path d="M21 2v6h-6" />
      <path d="M3 12a9 9 0 0 1 15-6.7L21 8" />
      <path d="M3 22v-6h6" />
      <path d="M21 12a9 9 0 0 1-15 6.7L3 16" />
    </svg>
  )
}

export function SearchIcon(props: { className?: string }) {
  return (
    <svg className={props.className} viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.5-3.5" />
    </svg>
  )
}

export function ExternalLinkIcon(props: { className?: string }) {
  return (
    <svg className={props.className} viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path d="M14 5h5v5" />
      <path d="M10 14 19 5" />
      <path d="M19 13v5a1 1 0 0 1-1 1h-12a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5" />
    </svg>
  )
}

export function TrashIcon(props: { className?: string }) {
  return (
    <svg className={props.className} viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path d="M3 6h18" />
      <path d="M8 6V4h8v2" />
      <path d="M19 6l-1 14H6L5 6" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </svg>
  )
}

function BrandGlyph(props: { className?: string; icon: IconifyIcon }) {
  return <Icon aria-hidden="true" className={cn('brandIcon', props.className)} icon={props.icon} />
}

export function GitHubIcon(props: { className?: string }) {
  return <BrandGlyph className={props.className} icon={mdiGithubCircle} />
}

export function GhcrIcon(props: { className?: string }) {
  return <BrandGlyph className={props.className} icon={mdiGithubBox} />
}

export function GitLabIcon(props: { className?: string }) {
  return <BrandGlyph className={props.className} icon={mdiGitlab} />
}

export function DockerIcon(props: { className?: string }) {
  return <BrandGlyph className={props.className} icon={mdiDocker} />
}

export function OctoRillIcon(props: { className?: string }) {
  return <img alt="" aria-hidden="true" className={cn('brandIcon', props.className)} src={octoRillMarkUrl} />
}

export function RepositoryIcon(props: { className?: string }) {
  return <BrandGlyph className={props.className} icon={mdiSourceRepository} />
}

export function RegistryIcon(props: { className?: string }) {
  return <BrandGlyph className={props.className} icon={mdiPackageVariantClosed} />
}

type AppButtonVariant = 'primary' | 'danger' | 'ghost'

type ButtonProps = {
  variant?: AppButtonVariant
  className?: string
  disabled?: boolean
  loading?: boolean
  loadingClickable?: boolean
  onClick?: () => void
  children: ReactNode
  title?: string
  hint?: string
  ariaPressed?: boolean
  ariaLabel?: string
}

function mapButtonVariant(variant: AppButtonVariant | undefined): 'default' | 'destructive' | 'ghost' {
  return variant === 'primary' ? 'default' : variant === 'danger' ? 'destructive' : 'ghost'
}

function buttonVariantClass(variant: AppButtonVariant): string {
  return variant === 'primary' ? 'btnPrimary' : variant === 'danger' ? 'btnDanger' : 'btnGhost'
}

function maybeWithTooltip(node: ReactNode, text: string | undefined) {
  const content = text?.trim() ?? ''
  if (!content) return node
  return (
    <TooltipProvider delayDuration={160}>
      <Tooltip>
        <TooltipTrigger asChild>{node}</TooltipTrigger>
        <TooltipContent>{content}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}

export type SelectFieldOption<T extends string = string> = {
  value: T
  label: ReactNode
  disabled?: boolean
}

export function SelectField<T extends string>(props: {
  value: T
  onChange: (value: T) => void
  options: Array<SelectFieldOption<T>>
  className?: string
  disabled?: boolean
  placeholder?: string
  title?: string
  id?: string
  ariaLabel?: string
}) {
  return (
    <Select disabled={props.disabled} value={props.value} onValueChange={(value) => props.onChange(value as T)}>
      <SelectTrigger
        aria-label={props.ariaLabel}
        className={cn('select', props.className)}
        id={props.id}
        title={props.title}
      >
        <SelectValue placeholder={props.placeholder} />
      </SelectTrigger>
      <SelectContent>
        {props.options.map((option) => (
          <SelectItem key={option.value} disabled={option.disabled} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

export function Button(props: ButtonProps) {
  const variant = props.variant ?? 'ghost'
  const disabled = props.disabled || (props.loading && !props.loadingClickable)
  const button = (
    <PrimitiveButton
      className={cn('btn', buttonVariantClass(variant), props.className)}
      disabled={disabled}
      aria-label={props.ariaLabel}
      aria-busy={props.loading ? true : undefined}
      aria-pressed={props.ariaPressed}
      data-hint={props.hint ?? undefined}
      onClick={props.onClick}
      title={props.hint ? undefined : props.title}
      variant={mapButtonVariant(variant)}
      type="button"
    >
      {props.loading ? (
        <span className="btnInlineLoading">
          <span className="btnInlineSpinner" aria-hidden="true" />
          <span>{props.children}</span>
        </span>
      ) : (
        props.children
      )}
    </PrimitiveButton>
  )

  const tooltipAnchor = props.hint && disabled ? <span className="btnTooltipAnchor">{button}</span> : button
  return maybeWithTooltip(tooltipAnchor, props.hint)
}

export function IconButton(props: {
  variant?: AppButtonVariant
  disabled?: boolean
  onClick?: () => void
  title: string
  hint?: string
  children: ReactNode
}) {
  const variant = props.variant ?? 'ghost'
  const button = (
    <PrimitiveButton
      className={cn('btn', 'btnIcon', buttonVariantClass(variant))}
      disabled={props.disabled}
      data-hint={props.title}
      onClick={props.onClick}
      title={props.hint ? undefined : props.title}
      variant={mapButtonVariant(variant)}
      size="icon"
      type="button"
      aria-label={props.title}
    >
      {props.children}
    </PrimitiveButton>
  )

  const tooltipAnchor = props.hint && props.disabled ? <span className="btnTooltipAnchor">{button}</span> : button
  return maybeWithTooltip(tooltipAnchor, props.hint ?? props.title)
}

export function IconLink(props: {
  href: string
  title: string
  children: ReactNode
  className?: string
  onClick?: ComponentProps<'a'>['onClick']
  linkKind?: 'registry' | 'repo'
  iconKind?: string
}) {
  const link = (
    <a
      aria-label={props.title}
      className={cn('iconLink', props.className)}
      data-link-icon={props.iconKind}
      data-link-kind={props.linkKind}
      href={props.href}
      onClick={props.onClick}
      rel="noopener noreferrer"
      target="_blank"
      title={props.title}
    >
      {props.children}
    </a>
  )
  return maybeWithTooltip(link, props.title)
}

const SMALL_ACTION_BUTTON_QUERY = '(max-width: 700px)'
const ACTION_BUTTON_LONG_PRESS_MS = 480
const ACTION_BUTTON_HINT_PERSIST_MS = 1200

type ResponsiveActionBubbleStyle = CSSProperties & {
  '--responsive-action-bubble-offset-x'?: string
}

export function ResponsiveActionButton(props: {
  variant?: AppButtonVariant
  disabled?: boolean
  onClick?: () => void
  label: string
  hint?: string
  icon: ReactNode
}) {
  const variant = props.variant ?? 'ghost'
  const hintText = props.hint?.trim() ?? ''
  const bubbleText = hintText || props.label
  const hasHint = hintText.length > 0
  const rootRef = useRef<HTMLButtonElement | null>(null)
  const [isSmallViewport, setIsSmallViewport] = useState(() => {
    if (typeof window === 'undefined') return false
    return window.matchMedia(SMALL_ACTION_BUTTON_QUERY).matches
  })
  const [showLongPressHint, setShowLongPressHint] = useState(false)
  const [bubbleAlign, setBubbleAlign] = useState<'center' | 'left' | 'right'>('center')
  const [bubbleVertical, setBubbleVertical] = useState<'above' | 'below'>('above')
  const [bubbleOffsetX, setBubbleOffsetX] = useState(0)
  const pressTimerRef = useRef<number | null>(null)
  const hideHintTimerRef = useRef<number | null>(null)
  const longPressTriggeredRef = useRef(false)
  const suppressNextClickRef = useRef(false)

  const clearPressTimer = () => {
    if (pressTimerRef.current == null) return
    window.clearTimeout(pressTimerRef.current)
    pressTimerRef.current = null
  }

  const clearHideHintTimer = () => {
    if (hideHintTimerRef.current == null) return
    window.clearTimeout(hideHintTimerRef.current)
    hideHintTimerRef.current = null
  }

  const dismissHint = () => {
    clearHideHintTimer()
    setShowLongPressHint(false)
    longPressTriggeredRef.current = false
  }

  const updateBubbleAlign = () => {
    if (typeof window === 'undefined') return
    const root = rootRef.current
    const bubble = root?.querySelector<HTMLElement>('.responsiveActionBtnBubble')
    if (!root || !bubble) return

    const rootRect = root.getBoundingClientRect()
    const bubbleRect = bubble.getBoundingClientRect()
    const bubbleWidth = bubbleRect.width
    const bubbleHeight = bubbleRect.height
    if (bubbleWidth <= 0 || bubbleHeight <= 0) {
      setBubbleAlign('center')
      setBubbleOffsetX(0)
      return
    }

    const margin = 8
    const viewportRight = window.innerWidth - margin
    const bubbleGap = 10
    const viewportBottom = window.innerHeight - margin
    const aboveTop = rootRect.top - bubbleGap - bubbleHeight
    const belowBottom = rootRect.bottom + bubbleGap + bubbleHeight
    const aboveOverflow = Math.max(0, margin - aboveTop)
    const belowOverflow = Math.max(0, belowBottom - viewportBottom)
    const candidates: Array<{ align: 'center' | 'left' | 'right'; left: number }> = [
      { align: 'center', left: rootRect.left + rootRect.width / 2 - bubbleWidth / 2 },
      { align: 'left', left: rootRect.left },
      { align: 'right', left: rootRect.right - bubbleWidth },
    ]

    const overflowScore = (left: number) => {
      const right = left + bubbleWidth
      return Math.max(0, margin - left) + Math.max(0, right - viewportRight)
    }

    let best = candidates[0]
    let bestScore = overflowScore(best.left)
    for (const candidate of candidates.slice(1)) {
      const score = overflowScore(candidate.left)
      if (score < bestScore) {
        best = candidate
        bestScore = score
      }
    }

    const bestRight = best.left + bubbleWidth
    let offsetX = 0
    if (best.left < margin) {
      offsetX = margin - best.left
    } else if (bestRight > viewportRight) {
      offsetX = viewportRight - bestRight
    }

    setBubbleAlign(best.align)
    setBubbleOffsetX(offsetX)
    setBubbleVertical(aboveOverflow <= belowOverflow ? 'above' : 'below')
  }

  useEffect(() => {
    if (typeof window === 'undefined') return undefined
    const media = window.matchMedia(SMALL_ACTION_BUTTON_QUERY)
    const handleChange = () => {
      const nextIsSmallViewport = media.matches
      setIsSmallViewport(nextIsSmallViewport)
      if (nextIsSmallViewport) return
      if (pressTimerRef.current != null) {
        window.clearTimeout(pressTimerRef.current)
        pressTimerRef.current = null
      }
      if (hideHintTimerRef.current != null) {
        window.clearTimeout(hideHintTimerRef.current)
        hideHintTimerRef.current = null
      }
      setShowLongPressHint(false)
      longPressTriggeredRef.current = false
      suppressNextClickRef.current = false
      setBubbleAlign('center')
      setBubbleVertical('above')
      setBubbleOffsetX(0)
    }
    handleChange()
    media.addEventListener('change', handleChange)
    return () => media.removeEventListener('change', handleChange)
  }, [])

  useEffect(() => {
    if (!showLongPressHint) return undefined
    const onResize = () => updateBubbleAlign()
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [showLongPressHint])

  useEffect(() => {
    return () => {
      if (pressTimerRef.current != null) {
        window.clearTimeout(pressTimerRef.current)
        pressTimerRef.current = null
      }
      if (hideHintTimerRef.current != null) {
        window.clearTimeout(hideHintTimerRef.current)
        hideHintTimerRef.current = null
      }
    }
  }, [])

  const scheduleHintDismiss = () => {
    clearHideHintTimer()
    hideHintTimerRef.current = window.setTimeout(() => {
      setShowLongPressHint(false)
      longPressTriggeredRef.current = false
      hideHintTimerRef.current = null
    }, ACTION_BUTTON_HINT_PERSIST_MS)
  }

  const handlePointerDown = () => {
    if (!isSmallViewport || props.disabled) return
    updateBubbleAlign()
    dismissHint()
    suppressNextClickRef.current = false
    clearPressTimer()
    pressTimerRef.current = window.setTimeout(() => {
      updateBubbleAlign()
      longPressTriggeredRef.current = true
      suppressNextClickRef.current = true
      setShowLongPressHint(true)
      pressTimerRef.current = null
    }, ACTION_BUTTON_LONG_PRESS_MS)
  }

  const handlePointerEnd = () => {
    if (!isSmallViewport) return
    clearPressTimer()
    if (!longPressTriggeredRef.current) return
    scheduleHintDismiss()
  }

  const className = [
    'btn',
    'responsiveActionBtn',
    buttonVariantClass(variant),
    bubbleAlign === 'left'
      ? 'responsiveActionBtnBubbleAlignLeft'
      : bubbleAlign === 'right'
        ? 'responsiveActionBtnBubbleAlignRight'
        : '',
    bubbleVertical === 'below' ? 'responsiveActionBtnBubbleBelow' : '',
    hasHint ? 'responsiveActionBtnHasHint' : '',
    showLongPressHint ? 'responsiveActionBtnShowBubble' : '',
  ]
    .filter(Boolean)
    .join(' ')
  const bubbleStyle: ResponsiveActionBubbleStyle = {
    '--responsive-action-bubble-offset-x': `${bubbleOffsetX}px`,
  }

  return (
    <PrimitiveButton
      ref={rootRef}
      className={className}
      style={bubbleStyle}
      disabled={props.disabled}
      aria-label={props.label}
      data-hint={props.hint ?? undefined}
      onPointerEnter={updateBubbleAlign}
      onPointerDown={handlePointerDown}
      onPointerUp={handlePointerEnd}
      onPointerCancel={handlePointerEnd}
      onPointerLeave={handlePointerEnd}
      onFocus={updateBubbleAlign}
      onBlur={() => {
        setBubbleAlign('center')
        setBubbleVertical('above')
        setBubbleOffsetX(0)
        dismissHint()
      }}
      onClick={(event) => {
        if (suppressNextClickRef.current) {
          event.preventDefault()
          event.stopPropagation()
          suppressNextClickRef.current = false
          return
        }
        dismissHint()
        props.onClick?.()
      }}
      type="button"
      variant={mapButtonVariant(variant)}
    >
      <span className="responsiveActionBtnIcon" aria-hidden="true">
        {props.icon}
      </span>
      <span className="responsiveActionBtnLabel">{props.label}</span>
      <span className="responsiveActionBtnBubble" aria-hidden="true">
        {bubbleText}
      </span>
    </PrimitiveButton>
  )
}

export function Chip(props: { children: ReactNode; active?: boolean; onClick?: () => void; title?: string }) {
  const active = props.active ?? false
  return (
    <Toggle
      aria-pressed={active}
      className={cn('chip', active && 'chipActive')}
      onPressedChange={() => props.onClick?.()}
      pressed={active}
      title={props.title}
      variant="outline"
    >
      {props.children}
    </Toggle>
  )
}

export function Pill(props: {
  tone: 'ok' | 'warn' | 'bad' | 'muted' | 'info'
  children: ReactNode
  breathing?: boolean
}) {
  const toneClass =
    props.tone === 'ok'
      ? 'pill pillOk'
      : props.tone === 'warn'
        ? 'pill pillWarn'
        : props.tone === 'bad'
          ? 'pill pillBad'
          : props.tone === 'info'
            ? 'pill pillInfo'
            : 'pill pillMuted'
  const className = props.breathing ? `${toneClass} pillBreathing` : toneClass
  return <Badge className={className} variant="outline">{props.children}</Badge>
}

export function DiscoveryCountPill(props: { count: number | null | undefined }) {
  const count = props.count ?? 0
  if (count <= 0) return null
  return <Pill tone="muted">{`发现 ${count} 次`}</Pill>
}

export function Switch(
  props: Omit<ComponentProps<typeof PrimitiveSwitch>, 'onCheckedChange' | 'onChange'> & {
    checked: boolean
    disabled?: boolean
    onChange: (checked: boolean) => void
  },
) {
  const { onChange, ...rest } = props
  return <PrimitiveSwitch {...rest} onCheckedChange={onChange} />
}

export function Mono(props: { children: ReactNode }) {
  return <span className="mono">{props.children}</span>
}

export function SectionTitle(props: { children: ReactNode }) {
  return <div className="sectionTitle">{props.children}</div>
}

function splitWarningMarker(note: string): { marker: string; content: string } | null {
  const trimmed = note.trimStart()
  if (!trimmed.startsWith('⚠')) return null
  const content = trimmed.slice('⚠'.length).trimStart()
  return { marker: '⚠', content }
}

export function StatusRemark(props: { service: Service; status: RowStatus }) {
  const note = noteFor(props.service, props.status).trim()
  const warning = splitWarningMarker(note)
  const discoveryCount = props.service.newVersionDiscoveryCount ?? 0
  const statusText = <span className="label statusLineLabelText">{statusLabel(props.status)}</span>
  return (
    <div className={`statusCol${discoveryCount > 0 ? ' statusColHasCompactBadge' : ''}`}>
      <div className="statusLine">
        <Icon icon={statusIcon(props.status)} className={statusDotClass(props.status)} aria-hidden="true" />
        {discoveryCount > 0 ? (
          <span className="statusLineLabelAnchor">
            {statusText}
            <DiscoveryHistoryPopover
              serviceId={props.service.id}
              count={discoveryCount}
              triggerVariant="compact-count"
            />
          </span>
        ) : (
          statusText
        )}
      </div>
      {note ? (
        <div className={`muted statusNote${warning ? ' statusNoteAnomaly' : ''}`}>
          {warning ? (
            <>
              <span className="statusNoteAnomalyMarker" aria-hidden="true">
                {warning.marker}
              </span>{' '}
              <span>{warning.content}</span>
            </>
          ) : (
            note
          )}
        </div>
      ) : null}
    </div>
  )
}
