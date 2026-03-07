import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Pill } from './ui'
import { ConfirmContext, type ConfirmBadgeTone, type ConfirmOptions, type ConfirmVariant } from './confirm'

type ConfirmRequest = ConfirmOptions & { resolve: (ok: boolean) => void }

function confirmButtonClass(variant: ConfirmVariant) {
  if (variant === 'danger') return 'btn btnDanger'
  if (variant === 'primary') return 'btn btnPrimary'
  return 'btn btnGhost'
}

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
          cardClassName={req.cardClassName}
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

export function ConfirmDialog(props: {
  title: string
  body: ReactNode
  cardClassName?: string
  bodyClassName?: string
  confirmText?: string
  cancelText?: string
  confirmVariant?: ConfirmVariant
  badgeText?: string | null
  badgeTone?: ConfirmBadgeTone | null
  onClose: (ok: boolean) => void
}) {
  const cancelRef = useRef<HTMLButtonElement | null>(null)
  const didResolveRef = useRef(false)
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

  const closeOnce = useCallback(
    (ok: boolean) => {
      if (didResolveRef.current) return
      didResolveRef.current = true
      props.onClose(ok)
    },
    [props],
  )

  useEffect(() => {
    cancelRef.current?.focus()
  }, [])

  return (
    <AlertDialog open onOpenChange={(open) => (!open ? closeOnce(false) : undefined)}>
      <AlertDialogContent className={props.cardClassName ? `modalCard ${props.cardClassName}` : 'modalCard'}>
        <AlertDialogHeader className="modalHeader">
          <div className="modalTitleRow">
            <AlertDialogTitle asChild>
              <div className="modalTitle">{props.title}</div>
            </AlertDialogTitle>
            {badgeText && badgeTone ? <Pill tone={badgeTone}>{badgeText}</Pill> : null}
          </div>
          <AlertDialogDescription asChild>
            <div className={props.bodyClassName ? `modalBody ${props.bodyClassName}` : 'modalBody'}>{props.body}</div>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter className="modalActions">
          <AlertDialogCancel ref={cancelRef} className="btn btnGhost" onClick={() => closeOnce(false)}>
            {cancelText}
          </AlertDialogCancel>
          <AlertDialogAction className={confirmButtonClass(confirmVariant)} onClick={() => closeOnce(true)}>
            {confirmText}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
