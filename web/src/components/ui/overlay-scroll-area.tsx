import * as React from 'react'
import type { PartialOptions } from 'overlayscrollbars'
import { OverlayScrollbarsComponent } from 'overlayscrollbars-react'

import { cn } from '@/lib/utils'

type OverlayScrollAreaProps = React.ComponentPropsWithoutRef<'div'> & {
  defer?: boolean
  onViewportReady?: (viewport: HTMLElement | null) => void
  options?: PartialOptions
  viewportLabel?: string
}

const defaultOptions: PartialOptions = {
  overflow: {
    x: 'scroll',
    y: 'scroll',
  },
  scrollbars: {
    autoHide: 'move',
    autoHideDelay: 600,
    clickScroll: false,
    dragScroll: true,
    theme: 'os-theme-dockrev',
  },
}

function OverlayScrollArea({
  className,
  defer = true,
  onViewportReady,
  options,
  viewportLabel,
  ...props
}: OverlayScrollAreaProps) {
  const viewportCleanupRef = React.useRef<(() => void) | null>(null)
  const resolvedOptions = React.useMemo<PartialOptions>(
    () => ({
      ...defaultOptions,
      ...options,
      overflow: {
        ...defaultOptions.overflow,
        ...options?.overflow,
      },
      scrollbars: {
        ...defaultOptions.scrollbars,
        ...options?.scrollbars,
      },
    }),
    [options],
  )

  return (
    <OverlayScrollbarsComponent
      {...props}
      className={cn('overlayScrollArea', className)}
      defer={defer}
      events={{
        destroyed: () => {
          viewportCleanupRef.current?.()
          viewportCleanupRef.current = null
          onViewportReady?.(null)
        },
        initialized: (instance) => {
          const viewport = instance.elements().viewport
          const host = viewport.closest<HTMLElement>('.overlayScrollArea')
          const handleFocus = () => host?.setAttribute('data-scrollbar-focus', 'true')
          const handleBlur = () => host?.removeAttribute('data-scrollbar-focus')
          viewportCleanupRef.current?.()
          viewport.addEventListener('focusin', handleFocus)
          viewport.addEventListener('focusout', handleBlur)
          viewportCleanupRef.current = () => {
            viewport.removeEventListener('focusin', handleFocus)
            viewport.removeEventListener('focusout', handleBlur)
            host?.removeAttribute('data-scrollbar-focus')
          }
          if (viewportLabel) {
            viewport.setAttribute('aria-label', viewportLabel)
            viewport.setAttribute('role', 'region')
            viewport.tabIndex = 0
          }
          onViewportReady?.(viewport)
        },
      }}
      options={resolvedOptions}
    />
  )
}
OverlayScrollArea.displayName = 'OverlayScrollArea'

export { OverlayScrollArea }
