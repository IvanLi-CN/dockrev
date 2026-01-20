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
    onComposeHint: (hint: { path?: string; profile?: string; lastScan?: string }) => void
  }) => ReactNode
}) {
  const [topActions, setTopActions] = useState<ReactNode>(null)
  const [composeHint, setComposeHint] = useState<{ path?: string; profile?: string; lastScan?: string }>({})

  return (
    <AppShell
      route={props.route}
      title={props.title}
      pageSubtitle={props.pageSubtitle}
      topbarHint={props.topbarHint}
      topActions={topActions}
      composeHint={composeHint}
    >
      {props.children({
        onTopActions: setTopActions,
        onComposeHint: setComposeHint,
      })}
    </AppShell>
  )
}
