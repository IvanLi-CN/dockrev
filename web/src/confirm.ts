import { createContext, useContext, type ReactNode } from 'react'

export type ConfirmVariant = 'primary' | 'danger' | 'ghost'

export type ConfirmOptions = {
  title: string
  body: ReactNode
  confirmText?: string
  cancelText?: string
  confirmVariant?: ConfirmVariant
}

export type ConfirmApi = {
  confirm: (opts: ConfirmOptions) => Promise<boolean>
}

export const ConfirmContext = createContext<ConfirmApi | null>(null)

export function useConfirm(): ConfirmApi['confirm'] {
  const ctx = useContext(ConfirmContext)
  if (!ctx) throw new Error('useConfirm must be used within ConfirmProvider')
  return ctx.confirm
}

