import { useEffect, useState, type ReactNode } from 'react'
import { X } from 'lucide-react'
import {
  Drawer,
  DrawerClose,
  DrawerContent,
  DrawerDescription,
  DrawerHandle,
  DrawerHeader,
  DrawerTitle,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '../ui'

function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() =>
    typeof window === 'undefined' ? false : window.matchMedia(query).matches,
  )

  useEffect(() => {
    const media = window.matchMedia(query)
    const onChange = () => setMatches(media.matches)
    onChange()
    media.addEventListener('change', onChange)
    return () => media.removeEventListener('change', onChange)
  }, [query])

  return matches
}

export function ResponsiveSettingsDrawer(props: {
  children: ReactNode
  description?: string
  onOpenChange: (open: boolean) => void
  open: boolean
  title: string
}) {
  const desktop = useMediaQuery('(min-width: 820px)')
  const direction = desktop ? 'right' : 'bottom'

  return (
    <Drawer direction={direction} handleOnly onOpenChange={props.onOpenChange} open={props.open}>
      <DrawerContent className="settingsDrawerContent" data-settings-drawer-direction={direction}>
        <div className="settingsDrawerDragZone" aria-label="拖动设置抽屉" data-settings-drawer-drag-zone="true">
          <DrawerHandle className="settingsDrawerHandle" />
        </div>
        <DrawerHeader className="settingsDrawerHeader">
          <div className="settingsDrawerTitleRow">
            <div>
              <DrawerTitle className="modalTitle">{props.title}</DrawerTitle>
              {props.description ? (
                <DrawerDescription className="settingsDrawerDescription">{props.description}</DrawerDescription>
              ) : null}
            </div>
            <Tooltip>
              <TooltipTrigger asChild>
                <DrawerClose asChild>
                  <button aria-label="关闭设置抽屉" className="settingsDrawerCloseIcon" type="button">
                    <X aria-hidden="true" className="iconSm" />
                  </button>
                </DrawerClose>
              </TooltipTrigger>
              <TooltipContent>关闭</TooltipContent>
            </Tooltip>
          </div>
        </DrawerHeader>
        <div className="settingsDrawerBody">{props.children}</div>
      </DrawerContent>
    </Drawer>
  )
}
