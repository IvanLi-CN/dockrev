import { useState, type ReactNode } from 'react'
import { AppShell } from '../../Shell'
import type { Route } from '../../routes'

export function PageHarness(props: {
  route: Route
  title: string
  pageSubtitle?: string
  topbarHint?: string
  children: (ctx: {
    onTopActions: (node: ReactNode) => void
    onLastScanHint: (lastScan?: string) => void
  }) => ReactNode
}) {
  const [topActions, setTopActions] = useState<ReactNode>(null)
  const [lastScanHint, setLastScanHint] = useState<string | undefined>(undefined)

  return (
    <AppShell
      route={props.route}
      title={props.title}
      pageSubtitle={props.pageSubtitle}
      topbarHint={props.topbarHint}
      topActions={topActions}
      lastScanHint={lastScanHint}
    >
      {props.children({
        onTopActions: setTopActions,
        onLastScanHint: setLastScanHint,
      })}
    </AppShell>
  )
}
