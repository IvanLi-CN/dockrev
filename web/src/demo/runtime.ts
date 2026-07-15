const APP_DEMO_FLAG_VALUES = new Set(['app', 'true', '1'])
const DOCKREV_RUNTIME_MODE_ATTRIBUTE = 'data-dockrev-runtime-mode'

export type DockrevRuntimeMode = 'app-demo'

function normalizeDockrevDemoFlag(value: string | null | undefined): string {
  return (value ?? '').trim().toLowerCase()
}

export function isDockrevAppDemoBuild(): boolean {
  return APP_DEMO_FLAG_VALUES.has(
    normalizeDockrevDemoFlag(import.meta.env.VITE_DOCKREV_DEMO),
  )
}

export function writeDockrevRuntimeMode(mode: DockrevRuntimeMode | null) {
  if (typeof document === 'undefined') return
  if (mode) {
    document.documentElement.setAttribute(DOCKREV_RUNTIME_MODE_ATTRIBUTE, mode)
    return
  }
  document.documentElement.removeAttribute(DOCKREV_RUNTIME_MODE_ATTRIBUTE)
}

export function readDockrevRuntimeMode(): DockrevRuntimeMode | null {
  if (typeof document !== 'undefined') {
    const mode = document.documentElement.getAttribute(
      DOCKREV_RUNTIME_MODE_ATTRIBUTE,
    )
    if (mode === 'app-demo') return mode
  }
  return isDockrevAppDemoBuild() ? 'app-demo' : null
}

export function isDockrevAppDemoRuntime(): boolean {
  return readDockrevRuntimeMode() === 'app-demo'
}
