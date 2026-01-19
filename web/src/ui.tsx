import type { ReactNode } from 'react'

export function Button(props: {
  variant?: 'primary' | 'danger' | 'ghost'
  disabled?: boolean
  onClick?: () => void
  children: ReactNode
  title?: string
}) {
  const variant = props.variant ?? 'ghost'
  const className =
    variant === 'primary' ? 'btn btnPrimary' : variant === 'danger' ? 'btn btnDanger' : 'btn btnGhost'
  return (
    <button className={className} disabled={props.disabled} onClick={props.onClick} title={props.title}>
      {props.children}
    </button>
  )
}

export function Chip(props: { children: ReactNode; active?: boolean; onClick?: () => void; title?: string }) {
  const className = props.active ? 'chip chipActive' : 'chip'
  return (
    <button className={className} onClick={props.onClick} title={props.title}>
      {props.children}
    </button>
  )
}

export function Pill(props: { tone: 'ok' | 'warn' | 'bad' | 'muted'; children: ReactNode }) {
  const className =
    props.tone === 'ok'
      ? 'pill pillOk'
      : props.tone === 'warn'
        ? 'pill pillWarn'
        : props.tone === 'bad'
          ? 'pill pillBad'
          : 'pill pillMuted'
  return <span className={className}>{props.children}</span>
}

export function Mono(props: { children: ReactNode }) {
  return <span className="mono">{props.children}</span>
}

export function SectionTitle(props: { children: ReactNode }) {
  return <div className="sectionTitle">{props.children}</div>
}

