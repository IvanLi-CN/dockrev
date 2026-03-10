import { useCallback, useEffect, useRef, useState } from 'react'
import type { ComponentPropsWithoutRef } from 'react'
import { PopoverContent } from '@/components/ui/popover'

const DEFAULT_HOVER_CLOSE_DELAY_MS = 300

type HoverPinnedPopoverOptions = {
  closeDelayMs?: number
}

export function useHoverPinnedPopover(options: HoverPinnedPopoverOptions = {}) {
  const closeDelayMs = options.closeDelayMs ?? DEFAULT_HOVER_CLOSE_DELAY_MS
  const hoverCloseTimerRef = useRef<number | null>(null)
  const pinnedRef = useRef(false)
  const [pinned, setPinned] = useState(false)
  const [hoverOpen, setHoverOpen] = useState(false)
  const open = pinned || hoverOpen

  const clearHoverCloseTimer = useCallback(() => {
    if (hoverCloseTimerRef.current == null) return
    window.clearTimeout(hoverCloseTimerRef.current)
    hoverCloseTimerRef.current = null
  }, [])

  const close = useCallback(() => {
    clearHoverCloseTimer()
    pinnedRef.current = false
    setPinned(false)
    setHoverOpen(false)
  }, [clearHoverCloseTimer])

  const scheduleHoverClose = useCallback(() => {
    if (pinnedRef.current) return
    clearHoverCloseTimer()
    hoverCloseTimerRef.current = window.setTimeout(() => {
      hoverCloseTimerRef.current = null
      if (pinnedRef.current) return
      setHoverOpen(false)
    }, closeDelayMs)
  }, [clearHoverCloseTimer, closeDelayMs])

  useEffect(() => {
    return () => clearHoverCloseTimer()
  }, [clearHoverCloseTimer])

  const setPinnedState = useCallback((next: boolean) => {
    pinnedRef.current = next
    setPinned(next)
  }, [])

  const togglePinned = useCallback(() => {
    clearHoverCloseTimer()
    const next = !pinnedRef.current
    setPinnedState(next)
    setHoverOpen(true)
    return next
  }, [clearHoverCloseTimer, setPinnedState])

  const popoverProps = {
    open,
    // Open/close is driven by the hover+pin state machine. Radix will still call
    // `onOpenChange` for trigger clicks / outside interactions, but we intentionally
    // ignore it here and handle dismissal explicitly via content event handlers.
    onOpenChange: () => {},
  }

  const triggerProps = {
    onPointerEnter: () => {
      clearHoverCloseTimer()
      setHoverOpen(true)
    },
    onPointerLeave: () => {
      scheduleHoverClose()
    },
    onClick: () => togglePinned(),
    'aria-expanded': open,
    'data-state': open ? 'open' : 'closed',
  } as const

  const contentProps: Pick<
    ComponentPropsWithoutRef<typeof PopoverContent>,
    | 'onPointerEnter'
    | 'onPointerLeave'
    | 'onEscapeKeyDown'
    | 'onOpenAutoFocus'
    | 'onCloseAutoFocus'
    | 'onPointerDownOutside'
    | 'onFocusOutside'
  > = {
    onPointerEnter: () => {
      clearHoverCloseTimer()
      setHoverOpen(true)
    },
    onPointerLeave: () => {
      scheduleHoverClose()
    },
    onPointerDownOutside: (event) => {
      // Allow clicks on other version popover triggers to proceed; the trigger click handler
      // decides whether to close/open. Closing here can cause a close+reopen race.
      const target = event.target as Element | null
      if (target?.closest?.('button.versionTagsTrigger')) return
      close()
    },
    onFocusOutside: (event) => {
      const target = event.target as Element | null
      if (target?.closest?.('button.versionTagsTrigger')) return
      close()
    },
    onEscapeKeyDown: () => close(),
    onOpenAutoFocus: (event) => event.preventDefault(),
    onCloseAutoFocus: (event) => event.preventDefault(),
  }

  return {
    close,
    contentProps,
    open,
    pinned,
    popoverProps,
    scheduleHoverClose,
    setPinned: setPinnedState,
    togglePinned,
    triggerProps,
  }
}
