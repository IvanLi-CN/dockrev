import { useEffect, useState, type ReactNode } from 'react'
import {
  Button,
  Drawer,
  DrawerClose,
  DrawerContent,
  DrawerDescription,
  DrawerHeader,
  DrawerTitle,
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
    <Drawer direction={direction} onOpenChange={props.onOpenChange} open={props.open}>
      <DrawerContent className="settingsDrawerContent" data-settings-drawer-direction={direction}>
        <DrawerHeader className="settingsDrawerHeader">
          <div className="settingsDrawerTitleRow">
            <div>
              <DrawerTitle className="modalTitle">{props.title}</DrawerTitle>
              {props.description ? (
                <DrawerDescription className="settingsDrawerDescription">{props.description}</DrawerDescription>
              ) : null}
            </div>
            <DrawerClose asChild>
              <Button aria-label="关闭设置抽屉" variant="ghost">
                关闭
              </Button>
            </DrawerClose>
          </div>
        </DrawerHeader>
        <div className="settingsDrawerBody">{props.children}</div>
      </DrawerContent>
    </Drawer>
  )
}
