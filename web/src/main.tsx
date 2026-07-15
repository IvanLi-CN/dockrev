import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { TooltipProvider } from '@/components/ui/tooltip'
import { PwaStatusProvider } from './pwaStatus'
import { restorePendingPagesDemoPath } from './demo/pagesDemoRestore'
import { isDockrevAppDemoBuild } from './demo/runtime'
import { initTheme } from './theme'

async function bootstrap() {
  initTheme()
  if (isDockrevAppDemoBuild()) {
    const { installAppDemoApi } = await import('./demo/appDemoApi')
    restorePendingPagesDemoPath()
    installAppDemoApi()
  }

  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <PwaStatusProvider>
        <TooltipProvider>
          <App />
        </TooltipProvider>
      </PwaStatusProvider>
    </StrictMode>,
  )
}

void bootstrap()
