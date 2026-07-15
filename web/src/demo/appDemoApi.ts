import { writeDockrevRuntimeMode } from './runtime'
import { installDockrevMockApi } from '../stories/mocks/dockrevMockApi/install'
import {
  loadPublicDemoFixture,
  PUBLIC_DEMO_CLEANUP_SCENARIO,
  PUBLIC_DEMO_SCENARIO,
  savePublicDemoFixture,
} from './publicDemoControls'

type DemoInstallResult = {
  enabled: boolean
  mode: 'app'
}

let installed = false

export function installAppDemoApi(): DemoInstallResult {
  if (installed) return { enabled: true, mode: 'app' }

  const initialFixture = loadPublicDemoFixture()
  savePublicDemoFixture(initialFixture)
  writeDockrevRuntimeMode('app-demo')
  installDockrevMockApi(PUBLIC_DEMO_SCENARIO, {
    cleanupScenario: PUBLIC_DEMO_CLEANUP_SCENARIO,
    initialFixture,
    onStateChange: savePublicDemoFixture,
  })
  installed = true
  return { enabled: true, mode: 'app' }
}
