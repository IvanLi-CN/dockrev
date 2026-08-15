import { useCallback, useEffect, useState, useSyncExternalStore, type CSSProperties, type ReactNode } from 'react'
import { Monitor, Moon, Sun } from 'lucide-react'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuLabel,
  ContextMenuRadioGroup,
  ContextMenuRadioItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import {
  cycleThemePreference,
  getSystemTheme,
  getThemePreference,
  initTheme,
  setThemePreference,
  subscribeTheme,
  type ThemePreference,
  type ThemeTransitionOrigin,
} from '../theme'
import { cn } from '@/lib/utils'

const THEME_OPTIONS: Array<{
  value: ThemePreference
  label: string
  Icon: typeof Monitor
}> = [
  { value: 'system', label: '跟随系统', Icon: Monitor },
  { value: 'light', label: '亮色', Icon: Sun },
  { value: 'dark', label: '暗色', Icon: Moon },
]

function useThemePreference() {
  return useSyncExternalStore(subscribeTheme, getThemePreference, () => 'system' as const)
}

function themeLabel(preference: ThemePreference) {
  return THEME_OPTIONS.find((option) => option.value === preference)?.label ?? '跟随系统'
}

function ThemeMenu(props: {
  preference: ThemePreference
  onPreferenceChange: (preference: ThemePreference) => void
  children: ReactNode
  label: string
}) {
  return (
    <ContextMenu>
      <Tooltip>
        <ContextMenuTrigger asChild>
          <TooltipTrigger asChild>{props.children}</TooltipTrigger>
        </ContextMenuTrigger>
        <TooltipContent>{props.label}</TooltipContent>
      </Tooltip>
      <ContextMenuContent className="themePreferenceMenu">
        <ContextMenuLabel>主题</ContextMenuLabel>
        <ContextMenuRadioGroup
          value={props.preference}
          onValueChange={(value) => props.onPreferenceChange(value as ThemePreference)}
        >
          {THEME_OPTIONS.map(({ value, label, Icon }) => (
            <ContextMenuRadioItem
              key={value}
              value={value}
              className="themePreferenceMenuItem"
            >
              <Icon size={16} strokeWidth={2} aria-hidden="true" />
              <span>{label}</span>
            </ContextMenuRadioItem>
          ))}
        </ContextMenuRadioGroup>
      </ContextMenuContent>
    </ContextMenu>
  )
}

function ThemeIcon(props: { preference: ThemePreference; size?: number }) {
  const option = THEME_OPTIONS.find(({ value }) => value === props.preference) ?? THEME_OPTIONS[0]
  const Icon = option.Icon
  return (
    <span className="themePreferenceGlyph" aria-hidden="true">
      <Icon size={props.size ?? 18} strokeWidth={2} />
    </span>
  )
}

function elementCenter(element: HTMLElement): ThemeTransitionOrigin {
  const rect = element.getBoundingClientRect()
  return {
    x: rect.left + rect.width / 2,
    y: rect.top + rect.height / 2,
  }
}

export function ThemePreferenceControl(props: {
  variant?: 'icon' | 'segmented'
  className?: string
}) {
  useEffect(() => {
    if (!document.documentElement.dataset.theme) initTheme()
  }, [])
  const preference = useThemePreference()
  const systemTheme = getSystemTheme()
  const label = themeLabel(preference)
  const [feedbackKey, setFeedbackKey] = useState(0)
  const selectPreference = useCallback((next: ThemePreference, origin?: ThemeTransitionOrigin) => {
    setFeedbackKey((value) => value + 1)
    setThemePreference(next, origin)
  }, [])
  const cycle = useCallback((element: HTMLElement) => {
    selectPreference(cycleThemePreference(preference, systemTheme), elementCenter(element))
  }, [preference, selectPreference, systemTheme])

  if (props.variant === 'segmented') {
    const activeIndex = Math.max(
      0,
      THEME_OPTIONS.findIndex(({ value }) => value === preference),
    )
    return (
      <div
        className={cn('themePreferenceSegmented', props.className)}
        role="radiogroup"
        aria-label="主题"
        style={{ '--theme-preference-index': activeIndex } as CSSProperties}
      >
        <span className="themePreferenceSliderThumb" aria-hidden="true" />
        {THEME_OPTIONS.map(({ value, label: optionLabel }) => (
          <Tooltip key={value}>
            <TooltipTrigger asChild>
              <button
                type="button"
                className="themePreferenceSegment"
                role="radio"
                aria-checked={preference === value}
                aria-label={optionLabel}
                title={optionLabel}
                onClick={(event) => selectPreference(value, elementCenter(event.currentTarget))}
              >
                <ThemeIcon preference={value} size={17} />
              </button>
            </TooltipTrigger>
            <TooltipContent>{optionLabel}</TooltipContent>
          </Tooltip>
        ))}
      </div>
    )
  }

  return (
    <ThemeMenu preference={preference} onPreferenceChange={selectPreference} label={label}>
      <button
        type="button"
        className="themePreferenceIconButton"
        aria-label={`主题：${label}`}
        title={label}
        onClick={(event) => cycle(event.currentTarget)}
        onKeyDown={(event) => {
          if (event.key === 'ContextMenu' || (event.key === 'F10' && event.shiftKey)) {
            event.preventDefault()
            const rect = event.currentTarget.getBoundingClientRect()
            event.currentTarget.dispatchEvent(
              new MouseEvent('contextmenu', {
                bubbles: true,
                clientX: rect.left + rect.width / 2,
                clientY: rect.top + rect.height / 2,
              }),
            )
          }
        }}
      >
        <span key={feedbackKey} className={feedbackKey > 0 ? 'themePreferenceGlyphFeedback' : undefined}>
          <ThemeIcon preference={preference} />
        </span>
      </button>
    </ThemeMenu>
  )
}
