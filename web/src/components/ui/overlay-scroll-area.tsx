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
    autoHide: 'never',
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
        destroyed: () => onViewportReady?.(null),
        initialized: (instance) => {
          const viewport = instance.elements().viewport
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
