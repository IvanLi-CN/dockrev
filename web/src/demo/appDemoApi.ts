import { appBasePath } from '../appBase'
import { installDockrevMockApi } from '../stories/mocks/dockrevMockApi/install'
import { buildFixture } from '../stories/mocks/dockrevMockApi/fixturesMisc'
import type { Fixture } from '../stories/mocks/dockrevMockApi/shared'

type DemoInstallResult = {
  enabled: boolean
  mode: 'app'
}

type StoredDemoFixture = {
  version: 1
  fixture: Fixture
}

const DEMO_FIXTURE_STORAGE_KEY = 'dockrev:public-demo:fixture:v1'

let installed = false

function currentDemoBaseUrl(): string | null {
  if (typeof window === 'undefined') return null
  try {
    return new URL(appBasePath(), window.location.origin).toString()
  } catch {
    return null
  }
}

function applyPublicDemoOverrides(fixture: Fixture): Fixture {
  const demoBaseUrl = currentDemoBaseUrl()
  if (!demoBaseUrl) return fixture

  fixture.settings.instance = {
    ...fixture.settings.instance,
    publicBaseUrl: demoBaseUrl,
  }
  fixture.githubPackagesSettings = {
    ...fixture.githubPackagesSettings,
    enabled: true,
    callbackUrl: new URL('api/webhooks/github-packages', demoBaseUrl).toString(),
  }
  fixture.deployCheckReport = {
    ...fixture.deployCheckReport,
    report: fixture.deployCheckReport.report
      ? {
          ...fixture.deployCheckReport.report,
          overall: {
            ...fixture.deployCheckReport.report.overall,
            summary: 'Public demo uses seeded mock state and never talks to a live backend.',
          },
        }
      : fixture.deployCheckReport.report,
  }
  return fixture
}

function buildSeedFixture(): Fixture {
  return applyPublicDemoOverrides(buildFixture('settings-configured'))
}

function readStoredFixture(): Fixture | null {
  if (typeof window === 'undefined') return null
  try {
    const raw = window.sessionStorage.getItem(DEMO_FIXTURE_STORAGE_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw) as Partial<StoredDemoFixture>
    if (parsed.version !== 1 || !parsed.fixture) return null
    return applyPublicDemoOverrides(parsed.fixture)
  } catch {
    return null
  }
}

function saveFixture(fixture: Fixture) {
  if (typeof window === 'undefined') return
  const payload: StoredDemoFixture = {
    version: 1,
    fixture: applyPublicDemoOverrides(fixture),
  }
  window.sessionStorage.setItem(DEMO_FIXTURE_STORAGE_KEY, JSON.stringify(payload))
}

function loadFixture(): Fixture {
  return readStoredFixture() ?? buildSeedFixture()
}

export function installAppDemoApi(): DemoInstallResult {
  if (installed) return { enabled: true, mode: 'app' }

  const initialFixture = loadFixture()
  saveFixture(initialFixture)
  installDockrevMockApi('settings-configured', {
    cleanupScenario: 'cleanup-console-storage-normal',
    initialFixture,
    onStateChange: saveFixture,
  })
  installed = true
  return { enabled: true, mode: 'app' }
}
