import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import './demo/demoBootFailure.css'
import App from './App.tsx'
import { TooltipProvider } from '@/components/ui/tooltip'
import { PwaStatusProvider } from './pwaStatus'
import { ManagementEventsProvider } from './managementEvents'
import { isDemoBundle } from './buildInfo'
import { startDemoRuntime, useDemoRuntime } from './demo/store'
import { DemoInspector } from './demo/DemoInspector'
import { initTheme } from './theme'

function renderDemoBootFailure(error: unknown) {
  const root = document.getElementById('root')
  if (!root) return
  const detail = error instanceof Error && error.message.trim() ? error.message.trim() : 'Unknown demo bootstrap error.'

  root.replaceChildren()
  const main = document.createElement('main')
  main.className = 'demoBootFailure'
  main.dataset.demoBootFailure = 'true'

  const heading = document.createElement('h1')
  heading.textContent = 'Web Demo 启动失败'

  const why = document.createElement('p')
  why.textContent = '这个站点是 mock-only 演示构建，不会退回真实后端。请把下面的错误报告给维护者。'

  const code = document.createElement('pre')
  code.textContent = detail

  const home = document.createElement('a')
  home.href = import.meta.env.BASE_URL || '/'
  home.textContent = '返回 Demo 首页'

  main.append(heading, why, code, home)
  root.append(main)
}

/**
 * A rebuilt mock model must reach every consumer, including the app's own boot
 * reads (settings/auth). Keying the tree on the runtime revision remounts it
 * exactly once per model change; navigation stays in the URL.
 */
function DemoRuntimeBoundary({ children }: { children: React.ReactNode }) {
  const runtime = useDemoRuntime()
  // The inspector lives outside the remount boundary: its open/drawer state is
  // a reviewer preference, so rebuilding the mock model must not close it.
  return (
    <>
      <div key={runtime.revision} style={{ display: 'contents' }}>{children}</div>
      <DemoInspector />
    </>
  )
}

async function bootstrap() {
  initTheme()
  if (isDemoBundle()) {
    // Mock interception must be armed before the first render. A failure here
    // must never fall through to the live app tree.
    await startDemoRuntime()
  }

  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <PwaStatusProvider>
        <ManagementEventsProvider>
          <TooltipProvider>
            {isDemoBundle() ? (
              <DemoRuntimeBoundary>
                <App />
              </DemoRuntimeBoundary>
            ) : (
              <App />
            )}
          </TooltipProvider>
        </ManagementEventsProvider>
      </PwaStatusProvider>
    </StrictMode>,
  )
}

bootstrap().catch(renderDemoBootFailure)
