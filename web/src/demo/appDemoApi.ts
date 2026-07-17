import { writeDockrevRuntimeMode } from './runtime'
import { installDockrevMockApi } from '../stories/mocks/dockrevMockApi/install'
import {
  loadPublicDemoFixture,
  PUBLIC_DEMO_CLEANUP_SCENARIO,
  PUBLIC_DEMO_GITHUB_RELEASES_BY_SERVICE_ID,
  readPublicDemoScenario,
  savePublicDemoFixture,
} from './publicDemoControls'

type DemoInstallResult = {
  enabled: boolean
  mode: 'app'
}

let installed = false

export function installAppDemoApi(): DemoInstallResult {
  if (installed) return { enabled: true, mode: 'app' }

  const scenario = readPublicDemoScenario()
  const initialFixture = loadPublicDemoFixture()
  savePublicDemoFixture(initialFixture)
  writeDockrevRuntimeMode('app-demo')
  installDockrevMockApi(scenario, {
    cleanupScenario: PUBLIC_DEMO_CLEANUP_SCENARIO,
    githubReleasesByServiceId: PUBLIC_DEMO_GITHUB_RELEASES_BY_SERVICE_ID,
    initialFixture,
    onStateChange: savePublicDemoFixture,
  })
  installed = true
  return { enabled: true, mode: 'app' }
}
