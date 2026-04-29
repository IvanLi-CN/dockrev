import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { TooltipProvider } from '@/components/ui/tooltip'
import { initTheme } from './theme'

function shouldInstallAppDemoApi(): boolean {
  const flag = import.meta.env.VITE_DOCKREV_DEMO
  const normalizedFlag = (flag ?? '').trim().toLowerCase()
  return normalizedFlag === 'app' || normalizedFlag === 'true' || normalizedFlag === '1'
}

async function bootstrap() {
  initTheme()
  if (shouldInstallAppDemoApi()) {
    const { installAppDemoApi } = await import('./demo/appDemoApi')
    installAppDemoApi()
  }

  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <TooltipProvider>
        <App />
      </TooltipProvider>
    </StrictMode>,
  )
}

void bootstrap()
