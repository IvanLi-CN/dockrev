import type { ReactNode } from 'react'

export function AppShellStatusBanner(props: {
  tone: 'update' | 'offline' | 'ready'
  title: string
  detail: string
  actions?: ReactNode
}) {
  return (
    <div className={`shellStatusBanner shellStatusBanner-${props.tone}`} role="status">
      <div className="shellStatusBannerText">
        <strong>{props.title}</strong>
        <span>{props.detail}</span>
      </div>
      {props.actions ? <div className="shellStatusBannerActions">{props.actions}</div> : null}
    </div>
  )
}
