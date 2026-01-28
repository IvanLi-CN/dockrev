import type { ReactNode } from 'react'
import type { Service } from './api'
import { noteFor, statusDotClass, statusLabel, type RowStatus } from './updateStatus'

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

export function Switch(props: { checked: boolean; disabled?: boolean; onChange: (checked: boolean) => void }) {
  return (
    <label className={props.disabled ? 'switch switchDisabled' : 'switch'}>
      <input
        type="checkbox"
        checked={props.checked}
        disabled={props.disabled}
        onChange={(e) => props.onChange(e.target.checked)}
      />
      <span className="switchSlider" />
    </label>
  )
}

export function Mono(props: { children: ReactNode }) {
  return <span className="mono">{props.children}</span>
}

export function SectionTitle(props: { children: ReactNode }) {
  return <div className="sectionTitle">{props.children}</div>
}

export function StatusRemark(props: { service: Service; status: RowStatus }) {
  return (
    <div className="statusCol">
      <div className="statusLine">
        <span className={statusDotClass(props.status)} aria-hidden="true" />
        <span className="label">{statusLabel(props.status)}</span>
      </div>
      <div className="muted statusNote">{noteFor(props.service, props.status)}</div>
    </div>
  )
}
