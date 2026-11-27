import { defineConfig, devices } from '@playwright/test'

const PORT = Number(process.env.DEMO_E2E_PORT ?? 50899)

/**
 * E2E runs against the built demo artifact, not the dev server, so it exercises
 * the same interception, share-state and rewrite contract that ships.
 */
export default defineConfig({
  testDir: './e2e',
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'list' : 'line',
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'node ./scripts/demo-e2e-server.mjs',
    port: PORT,
    reuseExistingServer: true,
    env: {
      DEMO_E2E_PORT: String(PORT),
      DEMO_E2E_DIST: process.env.DEMO_E2E_DIST ?? 'dist-demo',
    },
  },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 } } },
    { name: 'mobile', use: { ...devices['Pixel 7'] } },
  ],
})
