import { useState } from 'react'
import { ChevronDown, Layers3, type LucideIcon } from 'lucide-react'
import { Button as UiButton } from './ui/button'
import { ButtonGroup } from './ui/button-group'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu'
import { Toast, ToastProvider, ToastTitle, ToastViewport } from './ui/toast'
import { Button, Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui'

export type ServiceSplitActionItem = {
  id: string
  label: string
  icon: LucideIcon
  iconVariant?: 'solid'
  description?: string
  disabled?: boolean
  loading?: boolean
  loadingClickable?: boolean
  onSelect: () => void
}

export function ServiceStackDetailAction(props: {
  disabled?: boolean
  onClick: () => void
}) {
  return (
    <Button
      ariaLabel="Stack 详情"
      className="serviceStackDetailAction"
      disabled={props.disabled}
      hint="Stack 详情"
      onClick={props.onClick}
      variant="ghost"
    >
      <span className="serviceStackDetailActionContent">
        <Layers3 aria-hidden="true" className="serviceStackDetailActionIcon" />
        <span className="serviceStackDetailActionLabel">Stack 详情</span>
      </span>
    </Button>
  )
}

export function ServiceSplitActionButton(props: {
  ariaLabel: string
  primary: ServiceSplitActionItem
  items: ServiceSplitActionItem[]
}) {
  const PrimaryIcon = props.primary.icon
  const primaryIconClassName = `serviceSplitActionPrimaryIcon${props.primary.iconVariant === 'solid' ? ' serviceSplitActionIconSolid' : ''}`
  const [unavailableToast, setUnavailableToast] = useState<{ id: number; message: string } | null>(null)

  return (
    <ToastProvider duration={3200}>
      <TooltipProvider delayDuration={160}>
        <ButtonGroup aria-label={props.ariaLabel} className="serviceSplitAction" data-service-split-action={props.ariaLabel}>
          <Button
            variant="primary"
            disabled={props.primary.disabled}
            hint={props.primary.description}
            loading={props.primary.loading}
            loadingClickable={props.primary.loadingClickable}
            onClick={props.primary.onSelect}
          >
            {props.primary.loading ? props.primary.label : (
              <span className="serviceSplitActionPrimaryContent">
                <PrimaryIcon aria-hidden="true" className={primaryIconClassName} />
                <span>{props.primary.label}</span>
              </span>
            )}
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <UiButton
                aria-label={`${props.ariaLabel}菜单`}
                className="btn btnPrimary serviceSplitActionMenuTrigger"
                size="icon"
                type="button"
              >
                <ChevronDown aria-hidden="true" className="serviceSplitActionMenuIcon" />
              </UiButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              align="end"
              aria-label={props.ariaLabel}
              className="w-max min-w-0 max-w-[calc(100vw-2rem)] border-border/90 shadow-[0_18px_42px_rgba(1,8,20,0.5)]"
            >
              <DropdownMenuGroup>
                {props.items.map((item) => {
                  const ItemIcon = item.icon
                  const unavailable = Boolean(item.disabled)
                  const unavailableMessage = item.description?.trim() || '当前操作暂不可用'
                  const itemIconClassName = `serviceSplitActionMenuItemIcon${item.iconVariant === 'solid' ? ' serviceSplitActionIconSolid' : ''}`
                  const menuItem = (
                    <DropdownMenuItem
                      aria-disabled={unavailable || undefined}
                      className="min-h-9 py-2 aria-disabled:cursor-not-allowed aria-disabled:opacity-70"
                      data-service-split-item={item.id}
                      key={item.id}
                      onSelect={() => {
                        if (unavailable) {
                          setUnavailableToast({ id: Date.now(), message: unavailableMessage })
                          return
                        }
                        item.onSelect()
                      }}
                    >
                      <ItemIcon aria-hidden="true" className={itemIconClassName} />
                      <span>{item.label}</span>
                    </DropdownMenuItem>
                  )

                  return unavailable ? (
                    <Tooltip key={item.id}>
                      <TooltipTrigger asChild>{menuItem}</TooltipTrigger>
                      <TooltipContent className="max-w-[min(18rem,calc(100vw-2rem))] whitespace-normal leading-5" side="bottom">
                        {unavailableMessage}
                      </TooltipContent>
                    </Tooltip>
                  ) : menuItem
                })}
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </ButtonGroup>
      </TooltipProvider>
      {unavailableToast ? (
        <Toast
          data-service-split-toast="true"
          data-testid="service-split-toast"
          key={unavailableToast.id}
          onOpenChange={(open) => {
            if (!open) setUnavailableToast(null)
          }}
          open
        >
          <ToastTitle>{unavailableToast.message}</ToastTitle>
        </Toast>
      ) : null}
      <ToastViewport />
    </ToastProvider>
  )
}
