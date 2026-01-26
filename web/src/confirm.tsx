/* eslint-disable react-refresh/only-export-components */
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Button } from './ui'

type ConfirmVariant = 'primary' | 'danger' | 'ghost'

export type ConfirmOptions = {
  title: string
  body: string
  confirmText?: string
  cancelText?: string
  confirmVariant?: ConfirmVariant
}

type ConfirmRequest = ConfirmOptions & { resolve: (ok: boolean) => void }

type ConfirmApi = {
  confirm: (opts: ConfirmOptions) => Promise<boolean>
}

const ConfirmContext = createContext<ConfirmApi | null>(null)

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
          confirmText={req.confirmText}
          cancelText={req.cancelText}
          confirmVariant={req.confirmVariant}
          onClose={(ok) => {
            req.resolve(ok)
            setReq(null)
          }}
        />
      ) : null}
    </ConfirmContext.Provider>
  )
}

export function useConfirm(): ConfirmApi['confirm'] {
  const ctx = useContext(ConfirmContext)
  if (!ctx) throw new Error('useConfirm must be used within ConfirmProvider')
  return ctx.confirm
}

function ConfirmDialog(props: {
  title: string
  body: string
  confirmText?: string
  cancelText?: string
  confirmVariant?: ConfirmVariant
  onClose: (ok: boolean) => void
}) {
  const cancelRef = useRef<HTMLButtonElement | null>(null)
  const confirmVariant = props.confirmVariant ?? 'danger'
  const confirmText = props.confirmText ?? '确定'
  const cancelText = props.cancelText ?? '取消'

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
        <div className="modalTitle">{props.title}</div>
        <div className="modalBody">{props.body}</div>
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
