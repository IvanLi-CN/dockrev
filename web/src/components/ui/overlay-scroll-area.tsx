import * as React from 'react'
import type { PartialOptions } from 'overlayscrollbars'
import { OverlayScrollbarsComponent } from 'overlayscrollbars-react'

import { cn } from '@/lib/utils'

type OverlayScrollAreaProps = React.ComponentPropsWithoutRef<'div'> & {
  onViewportReady?: (viewport: HTMLElement | null) => void
  options?: PartialOptions
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

function OverlayScrollArea({ className, onViewportReady, options, ...props }: OverlayScrollAreaProps) {
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
      events={{
        destroyed: () => onViewportReady?.(null),
        initialized: (instance) => onViewportReady?.(instance.elements().viewport),
      }}
      options={resolvedOptions}
    />
  )
}
OverlayScrollArea.displayName = 'OverlayScrollArea'

export { OverlayScrollArea }
