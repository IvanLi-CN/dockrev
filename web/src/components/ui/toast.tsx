import * as React from 'react'
import * as ToastPrimitive from '@radix-ui/react-toast'

import { cn } from '@/lib/utils'

function ToastProvider({ ...props }: React.ComponentProps<typeof ToastPrimitive.Provider>) {
  return <ToastPrimitive.Provider data-slot="toast-provider" {...props} />
}

function ToastViewport({ className, ...props }: React.ComponentProps<typeof ToastPrimitive.Viewport>) {
  return (
    <ToastPrimitive.Viewport
      data-slot="toast-viewport"
      className={cn(
        'fixed top-20 right-4 z-[100] flex w-[min(22rem,calc(100vw-2rem))] max-w-full flex-col gap-2 outline-none sm:right-5',
        className,
      )}
      {...props}
    />
  )
}

function Toast({ className, ...props }: React.ComponentProps<typeof ToastPrimitive.Root>) {
  return (
    <ToastPrimitive.Root
      data-slot="toast"
      className={cn(
        'rounded-md border border-border bg-popover px-3 py-2.5 text-sm text-popover-foreground shadow-[0_16px_42px_rgba(2,6,23,0.45)] animate-in fade-in-0 zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95',
        className,
      )}
      {...props}
    />
  )
}

function ToastTitle({ className, ...props }: React.ComponentProps<typeof ToastPrimitive.Title>) {
  return <ToastPrimitive.Title data-slot="toast-title" className={cn('leading-5', className)} {...props} />
}

export { Toast, ToastProvider, ToastTitle, ToastViewport }
