import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Button, Pill } from './ui'
import { ConfirmContext, type ConfirmBadgeTone, type ConfirmOptions, type ConfirmVariant } from './confirm'

type ConfirmRequest = ConfirmOptions & { resolve: (ok: boolean) => void }

export function ConfirmProvider(props: { children: ReactNode }) {
  const [req, setReq] = useState<ConfirmRequest | null>(null)

  const confirm = useCallback(async (opts: ConfirmOptions) => {
    return await new Promise<boolean>((resolve) => {
      setReq({ ...opts, resolve })
    })
  }, [])

  const api = useMemo(() => ({ confirm }), [confirm])

  return (
    <ConfirmContext.Provider value={api}>
      {props.children}
      {req ? (
        <ConfirmDialog
          title={req.title}
          body={req.body}
          bodyClassName={req.bodyClassName}
          confirmText={req.confirmText}
          cancelText={req.cancelText}
          confirmVariant={req.confirmVariant}
          badgeText={req.badgeText}
          badgeTone={req.badgeTone}
          onClose={(ok) => {
            req.resolve(ok)
            setReq(null)
          }}
        />
      ) : null}
    </ConfirmContext.Provider>
  )
}

function ConfirmDialog(props: {
  title: string
  body: ReactNode
  bodyClassName?: string
  confirmText?: string
  cancelText?: string
  confirmVariant?: ConfirmVariant
  badgeText?: string | null
  badgeTone?: ConfirmBadgeTone | null
  onClose: (ok: boolean) => void
}) {
  const cancelRef = useRef<HTMLButtonElement | null>(null)
  const confirmVariant = props.confirmVariant ?? 'danger'
  const confirmText = props.confirmText ?? '确定'
  const cancelText = props.cancelText ?? '取消'

  const defaultBadgeTone: ConfirmBadgeTone =
    confirmVariant === 'danger' ? 'bad' : confirmVariant === 'primary' ? 'warn' : 'muted'

  const defaultBadgeText = confirmVariant === 'danger' ? '高影响' : confirmVariant === 'primary' ? '将触发任务' : '确认'
  const badgeTextCandidate = props.badgeText === undefined ? defaultBadgeText : props.badgeText
  const badgeText =
    typeof badgeTextCandidate === 'string' && badgeTextCandidate.trim() === '' ? null : badgeTextCandidate
  const badgeTone = badgeText ? (props.badgeTone ?? defaultBadgeTone) : null

  useEffect(() => {
    cancelRef.current?.focus()
  }, [])

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        props.onClose(false)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [props])

  return (
    <div
      className="modalOverlay"
      role="presentation"
      onClick={() => {
        props.onClose(false)
      }}
    >
      <div
        className="modalCard"
        role="dialog"
        aria-modal="true"
        aria-label={props.title}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modalHeader">
          <div className="modalTitle">{props.title}</div>
          {badgeText && badgeTone ? <Pill tone={badgeTone}>{badgeText}</Pill> : null}
        </div>
        <div className={props.bodyClassName ? `modalBody ${props.bodyClassName}` : 'modalBody'}>{props.body}</div>
        <div className="modalActions">
          <button className="btn btnGhost" ref={cancelRef} onClick={() => props.onClose(false)}>
            {cancelText}
          </button>
          <Button variant={confirmVariant} onClick={() => props.onClose(true)}>
            {confirmText}
          </Button>
        </div>
      </div>
    </div>
  )
}
