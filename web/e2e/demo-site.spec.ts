import { expect, test, type Page } from '@playwright/test'

const panel = (page: Page) => page.getByRole('region', { name: 'Demo 控制面板' })

/**
 * Desktop renders the inspector inline; mobile hides it behind the DEMO bubble.
 * Every scenario test starts from the same control surface.
 */
async function openInspector(page: Page) {
  const bubble = page.getByRole('button', { name: '打开 Demo 控制面板' })
  if (await bubble.count()) await bubble.first().click()
  await expect(panel(page)).toBeVisible()
}

async function closeInspector(page: Page) {
  const close = panel(page).getByRole('button', { name: '关闭', exact: true })
  if (await close.count()) {
    await close.click()
    await expect(panel(page)).toHaveCount(0)
  }
}

test.describe('demo site share contract', () => {
  test('scene selection is client-side and keeps the share parameter', async ({ page }) => {
    await page.goto('/queue?demo=queue')
    await openInspector(page)

    await page.evaluate(() => {
      window.__demoE2eMarker__ = 'alive'
    })

    await panel(page).getByRole('button', { name: /Cleanup/ }).click()
    await expect(page).toHaveURL(/\/cleanup\?demo=cleanup$/)
    // A document reload would drop this marker.
    expect(await page.evaluate(() => window.__demoE2eMarker__)).toBe('alive')
  })

  test('data state survives a reload inside the same session', async ({ page }) => {
    await page.goto('/services?demo=services')
    await openInspector(page)

    await panel(page).getByRole('combobox', { name: 'Demo network' }).click()
    await page.getByRole('option', { name: '慢速 (3s)' }).click()
    await expect(page).toHaveURL(/d_net=slow/)

    await page.reload()
    await openInspector(page)
    await expect(panel(page).getByRole('combobox', { name: 'Demo network' })).toHaveText(/慢速/)

    await panel(page).getByRole('combobox', { name: 'Demo persona' }).click()
    await page.getByRole('option', { name: '维护者' }).click()
    await expect(page).toHaveURL(/d_persona=admin/)

    await page.reload()
    await openInspector(page)
    await expect(panel(page).getByRole('combobox', { name: 'Demo persona' })).toHaveText(/维护者/)
  })

  test('simulated writes appear in the mutation log', async ({ page }) => {
    // The cleanup rescan is a write affordance on both viewports, unlike the
    // settings toggles which only render on desktop.
    await page.goto('/cleanup?demo=cleanup')
    await expect(page.getByRole('button', { name: '重扫' }).first()).toBeVisible()

    const readLog = () =>
      page.evaluate(() => {
        const raw = window.sessionStorage.getItem('dockrev:demo:mutations:v1')
        return raw ? (JSON.parse(raw) as unknown[]).length : 0
      })
    const baseline = await readLog()
    await page.getByRole('button', { name: '重扫' }).first().click()
    await expect.poll(readLog).toBeGreaterThan(baseline)

    // The inspector renders the same persisted log once opened.
    await openInspector(page)
    await expect(panel(page).locator('.demoInspectorMutationLabel').first()).toContainText('POST /api/')
  })

  test('hidden controls provide a clean evidence surface', async ({ page }) => {
    await page.goto('/cleanup?demo=cleanup&d_controls=hidden')
    await expect(page.locator('.appShell')).toBeVisible()
    await expect(panel(page)).toHaveCount(0)

    await page.getByRole('button', { name: 'Demo', exact: true }).click()
    await openInspector(page)
    await expect(page).not.toHaveURL(/d_controls=hidden/)
  })

  test('version comes from the build, never from a live backend', async ({ page }) => {
    const buildInfo = await page.request.get('/build-info.json').then((response) => response.json())
    await page.goto('/')
    await expect(page.locator('.sidebarAppMetaVersion').first()).not.toBeEmpty()

    const apiVersion = await page.evaluate(async () => (await (await fetch('/api/version')).json()).version)
    expect(apiVersion).toBe(buildInfo.version)

    // Non-release builds must not fabricate a release link.
    if (buildInfo.channel !== 'release') {
      await expect(page.getByRole('link', { name: /Release on GitHub/ })).toHaveCount(0)
    }
  })
})

test.describe('demo inspector layout', () => {
  test('desktop panel collapses to a bubble and restores', async ({ page, isMobile }) => {
    test.skip(isMobile, 'desktop-only contract')
    await page.goto('/services?demo=services')
    await expect(panel(page)).toBeVisible()

    await panel(page).getByRole('button', { name: '收起', exact: true }).click()
    await expect(panel(page)).toHaveCount(0)

    await page.locator('.demoInspectorBubble').click()
    await expect(panel(page)).toBeVisible()
  })

  test('mobile entry opens a drawer instead of nothing', async ({ page, isMobile }) => {
    test.skip(!isMobile, 'mobile-only contract')
    await page.goto('/cleanup?demo=cleanup')
    await expect(page.getByRole('button', { name: '打开 Demo 控制面板' })).toBeVisible()
    await openInspector(page)
    await closeInspector(page)
  })
})

test.describe('demo mock scope', () => {
  const routes = [
    '/',
    '/services',
    '/services/stack-prod/svc-prod-api',
    '/services/stack-prod/svc-prod-api/history',
    '/queue',
    '/settings',
    '/settings/ghcr-webhooks',
    '/cleanup',
    '/deploy-check',
  ]

  for (const route of routes) {
    test(`${route} renders without an unhandled mock route`, async ({ page }) => {
      await page.goto(`${route}?demo=overview&d_controls=hidden`)
      // /deploy-check is its own standalone page without the app shell.
      await expect(page.locator('#root')).not.toBeEmpty()
      await expect(page.locator('body')).not.toContainText('unhandled mock route')
      await expect(page.locator('body')).not.toContainText('Web Demo 启动失败')
    })
  }
})
