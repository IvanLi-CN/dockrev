import type { ReactNode } from 'react'
import type { Service } from './api'
import { noteFor, statusDotClass, statusLabel, type RowStatus } from './updateStatus'

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

export function GitHubIcon(props: { className?: string }) {
  // GitHub mark (octicon style) as an inline SVG to avoid extra deps.
  return (
    <svg className={props.className} viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <path
        fill="currentColor"
        d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38
        0-.19-.01-.82-.01-1.49-2 .37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52
        -.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2
        -3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82
        .64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08
        2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07
        -.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8Z"
      />
    </svg>
  )
}

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

export function IconButton(props: {
  variant?: 'primary' | 'danger' | 'ghost'
  disabled?: boolean
  onClick?: () => void
  title: string
  children: ReactNode
}) {
  const variant = props.variant ?? 'ghost'
  const className =
    variant === 'primary'
      ? 'btn btnIcon btnPrimary'
      : variant === 'danger'
        ? 'btn btnIcon btnDanger'
        : 'btn btnIcon btnGhost'
  return (
    <button
      className={className}
      disabled={props.disabled}
      onClick={props.onClick}
      title={props.title}
      aria-label={props.title}
    >
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

function splitWarningMarker(note: string): { marker: string; content: string } | null {
  const trimmed = note.trimStart()
  if (!trimmed.startsWith('⚠')) return null
  const content = trimmed.slice('⚠'.length).trimStart()
  return { marker: '⚠', content }
}

export function StatusRemark(props: { service: Service; status: RowStatus }) {
  const note = noteFor(props.service, props.status).trim()
  const warning = splitWarningMarker(note)
  return (
    <div className="statusCol">
      <div className="statusLine">
        <span className={statusDotClass(props.status)} aria-hidden="true" />
        <span className="label">{statusLabel(props.status)}</span>
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
