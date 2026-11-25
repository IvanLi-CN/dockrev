import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import './demo/demoBootFailure.css'
import App from './App.tsx'
import { TooltipProvider } from '@/components/ui/tooltip'
import { PwaStatusProvider } from './pwaStatus'
import { ManagementEventsProvider } from './managementEvents'
import { isDemoBundle } from './buildInfo'
import { startDemoRuntime } from './demo/store'
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
            <App />
          </TooltipProvider>
        </ManagementEventsProvider>
      </PwaStatusProvider>
    </StrictMode>,
  )
}

bootstrap().catch(renderDemoBootFailure)
