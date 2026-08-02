import { Fragment, useState } from 'react'
import { ChevronDown, Ellipsis, Layers3, type LucideIcon } from 'lucide-react'
import { Button as UiButton } from './ui/button'
import { ButtonGroup } from './ui/button-group'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
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

export type ServiceMobileActionGroup = {
  id: string
  items: ServiceSplitActionItem[]
}

function ServiceActionMenuItem(props: {
  item: ServiceSplitActionItem
  dataAttribute: 'data-service-mobile-action-item' | 'data-service-split-item'
  onUnavailable: (message: string) => void
}) {
  const { item } = props
  const ItemIcon = item.icon
  const unavailable = Boolean(item.disabled)
  const unavailableMessage = item.description?.trim() || '当前操作暂不可用'
  const itemIconClassName = `serviceSplitActionMenuItemIcon${item.iconVariant === 'solid' ? ' serviceSplitActionIconSolid' : ''}`
  const dataProps = { [props.dataAttribute]: item.id }
  const menuItem = (
    <DropdownMenuItem
      aria-disabled={unavailable || undefined}
      className="min-h-9 py-2 aria-disabled:cursor-not-allowed aria-disabled:opacity-70"
      {...dataProps}
      onSelect={() => {
        if (unavailable) {
          props.onUnavailable(unavailableMessage)
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
    <Tooltip>
      <TooltipTrigger asChild>{menuItem}</TooltipTrigger>
      <TooltipContent className="max-w-[min(18rem,calc(100vw-2rem))] whitespace-normal leading-5" side="bottom">
        {unavailableMessage}
      </TooltipContent>
    </Tooltip>
  ) : menuItem
}

export function ServiceMobileActionMenu(props: {
  ariaLabel?: string
  groups: ServiceMobileActionGroup[]
}) {
  const ariaLabel = props.ariaLabel ?? '服务操作'
  const groups = props.groups.filter((group) => group.items.length > 0)
  const [unavailableToast, setUnavailableToast] = useState<{ id: number; message: string } | null>(null)

  return (
    <ToastProvider duration={3200}>
      <TooltipProvider delayDuration={160}>
        <div className="serviceMobileActionMenu">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <UiButton
                aria-label={ariaLabel}
                className="btn btnGhost serviceMobileActionMenuTrigger"
                size="icon"
                type="button"
                variant="ghost"
              >
                <Ellipsis aria-hidden="true" className="serviceMobileActionMenuTriggerIcon" />
              </UiButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent
              align="end"
              aria-label={ariaLabel}
              className="serviceMobileActionMenuContent w-max min-w-[9rem] max-w-[calc(100vw-1.5rem)] border-border/90 shadow-[0_18px_42px_rgba(1,8,20,0.5)]"
            >
              {groups.map((group, groupIndex) => (
                <Fragment key={group.id}>
                  {groupIndex > 0 ? <DropdownMenuSeparator data-service-mobile-action-separator="true" /> : null}
                  <DropdownMenuGroup data-service-mobile-action-group={group.id}>
                    {group.items.map((item) => (
                      <ServiceActionMenuItem
                        dataAttribute="data-service-mobile-action-item"
                        item={item}
                        key={item.id}
                        onUnavailable={(message) => setUnavailableToast({ id: Date.now(), message })}
                      />
                    ))}
                  </DropdownMenuGroup>
                </Fragment>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </TooltipProvider>
      {unavailableToast ? (
        <Toast
          data-service-mobile-action-toast="true"
          data-testid="service-mobile-action-toast"
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
  disabled?: boolean
  disabledReason?: string
}) {
  const PrimaryIcon = props.primary.icon
  const primaryIconClassName = `serviceSplitActionPrimaryIcon${props.primary.iconVariant === 'solid' ? ' serviceSplitActionIconSolid' : ''}`
  const [unavailableToast, setUnavailableToast] = useState<{ id: number; message: string } | null>(null)
  const [groupTooltipOpen, setGroupTooltipOpen] = useState(false)
  const groupDisabled = Boolean(props.disabled)
  const groupDisabledReason = groupDisabled ? props.disabledReason?.trim() : ''

  const actionGroup = (
    <ButtonGroup
      aria-disabled={groupDisabled || undefined}
      aria-label={props.ariaLabel}
      className="serviceSplitAction"
      data-service-split-action={props.ariaLabel}
    >
      <Button
        variant="primary"
        disabled={groupDisabled || props.primary.disabled}
        hint={groupDisabledReason || props.primary.description}
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
            disabled={groupDisabled}
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
              return (
                <ServiceActionMenuItem
                  dataAttribute="data-service-split-item"
                  item={item}
                  key={item.id}
                  onUnavailable={(message) => setUnavailableToast({ id: Date.now(), message })}
                />
              )
            })}
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </ButtonGroup>
  )

  const actionGroupWithTooltip = groupDisabledReason ? (
    <Tooltip open={groupTooltipOpen} onOpenChange={setGroupTooltipOpen}>
      <TooltipTrigger asChild>
        <span
          aria-label={`${props.ariaLabel}：${groupDisabledReason}`}
          className="serviceSplitActionDisabledAnchor"
          onClick={() => setGroupTooltipOpen(true)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' || event.key === ' ') {
              event.preventDefault()
              setGroupTooltipOpen(true)
            }
          }}
          onPointerDown={() => setGroupTooltipOpen(true)}
          tabIndex={0}
        >
          {actionGroup}
        </span>
      </TooltipTrigger>
      <TooltipContent className="max-w-[min(22rem,calc(100vw-2rem))] whitespace-normal leading-5" side="bottom">
        {groupDisabledReason}
      </TooltipContent>
    </Tooltip>
  ) : actionGroup

  return (
    <ToastProvider duration={3200}>
      <TooltipProvider delayDuration={160}>
        {actionGroupWithTooltip}
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
