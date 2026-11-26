import { expect, test, type Page } from '@playwright/test'

const panel = (page: Page) => page.getByRole('region', { name: 'Demo 控制面板' })

/**
 * Desktop starts closed behind a bubble; mobile keeps its own bubble. Every
 * scenario test opens the same control surface first.
 */
async function openInspector(page: Page) {
  const bubble = page.getByRole('button', { name: '打开 Web Demo 调试控制面板' })
  if (await bubble.count()) await bubble.first().click()
  await expect(panel(page)).toBeVisible()
}

/**
 * The shell renders the identity chip in the sidebar slot. Desktop shows it
 * directly, mobile keeps it inside the closed sidebar, so this asserts the
 * rendered identity rather than assuming a viewport-independent position.
 */
async function expectIdentityChip(page: Page, label: string) {
  await expect
    .poll(async () => page.locator(`[aria-label="当前身份：${label}"]`).count())
    .toBeGreaterThan(0)
}

async function readMutations(page: Page): Promise<Array<{ label: string; detail: string }>> {
  return page.evaluate(() => {
    const raw = window.sessionStorage.getItem('dockrev:demo:mutations:v1')
    return raw ? (JSON.parse(raw) as Array<{ label: string; detail: string }>) : []
  })
}

test.describe('demo environment contract', () => {
  test('scene selection is client-side and keeps the share parameter', async ({ page }) => {
    await page.goto('/queue?demo=queue')
    await openInspector(page)

    await page.evaluate(() => {
      window.__demoE2eMarker__ = 'alive'
    })

    await panel(page).getByRole('combobox', { name: '选择 Demo 场景' }).click()
    await page.getByRole('option', { name: 'Cleanup', exact: true }).click()

    await expect(page).toHaveURL(/\/cleanup\?demo=cleanup$/)
    // A document reload would drop this marker.
    expect(await page.evaluate(() => window.__demoE2eMarker__)).toBe('alive')
  })

  test('identity, connection and latency survive a reload and a scene switch', async ({ page }) => {
    await page.goto('/services?demo=services')
    await openInspector(page)

    await panel(page).getByRole('radio', { name: '普通用户' }).click()
    await expect(page).toHaveURL(/d_persona=user/)
    // The simulated identity must reach the product's own identity chip.
    await expectIdentityChip(page, 'demo-user')

    await page.reload()
    // The panel open state is local, so a reload may restore either surface.
    await openInspector(page)
    await expect(panel(page).getByRole('radio', { name: '普通用户' })).toHaveAttribute('data-state', 'on')

    // Connectivity is applied after identity resolves: offline intentionally
    // makes every product request fail, so `/api/settings` cannot answer the
    // identity query while it is off.
    await panel(page).getByRole('radio', { name: '离线' }).click()
    await expect(page).toHaveURL(/d_connection=offline/)

    await page.reload()
    await openInspector(page)
    await expect(panel(page).getByRole('radio', { name: '离线' })).toHaveAttribute('data-state', 'on')
    await expect(panel(page).getByRole('radio', { name: '普通用户' })).toHaveAttribute('data-state', 'on')

    // Environment belongs to the reviewer, so the scene switch keeps it.
    await panel(page).getByRole('combobox', { name: '选择 Demo 场景' }).click()
    await page.getByRole('option', { name: 'Queue', exact: true }).click()
    await expect(page).toHaveURL(/\/queue\?demo=queue/)
    await expect(page).toHaveURL(/d_persona=user/)
    await expect(page).toHaveURL(/d_connection=offline/)

    // Coming back online restores the identity the persisted persona describes.
    // `online` is the default, so it is omitted from the URL by design.
    await panel(page).getByRole('radio', { name: '在线' }).click()
    await expect(page).not.toHaveURL(/d_connection=offline/)
    await expectIdentityChip(page, 'demo-user')
  })

  test('guest identity drops the user object and reaches the chip', async ({ page, isMobile }) => {
    await page.goto('/?demo=overview&d_persona=guest')
    await expectIdentityChip(page, '匿名开发')

    const auth = await page.evaluate(async () => (await (await fetch('/api/settings')).json()).auth)
    expect(auth.currentUser).toBeNull()
    expect(auth.currentGroups).toEqual([])
    expect(auth.matchedBy).toBe('anonymous_dev')

    // The visible chip lives in the sidebar, which desktop renders inline.
    if (!isMobile) {
      await expect(page.locator('[aria-label="当前身份：匿名开发"]:visible')).toHaveCount(1)
    }
  })

  test('demo deep links survive a direct visit', async ({ page }) => {
    for (const route of ['/services', '/queue', '/cleanup', '/settings']) {
      const response = await page.goto(`${route}?demo=overview&d_controls=hidden`)
      expect(response?.status()).toBe(200)
      await expect(page.locator('#root')).not.toBeEmpty()
    }
  })

  test('offline fails product requests without touching the real backend', async ({ page }) => {
    await page.goto('/services?demo=services&d_connection=offline&d_controls=hidden')
    const result = await page.evaluate(async () => {
      try {
        await fetch('/api/settings')
        return 'resolved'
      } catch (error) {
        return error instanceof TypeError ? 'type-error' : String(error)
      }
    })
    expect(result).toBe('type-error')
  })

  test('slow mode adds the documented delay to product requests', async ({ page }) => {
    await page.goto('/services?demo=services&d_delay=slow&d_controls=hidden')
    const elapsed = await page.evaluate(async () => {
      const started = performance.now()
      await fetch('/api/settings')
      return performance.now() - started
    })
    expect(elapsed).toBeGreaterThan(1_400)
  })

  test('theme control is the product preference, not a second state', async ({ page }) => {
    await page.goto('/services?demo=services')
    await openInspector(page)
    await panel(page).getByRole('radio', { name: '深色' }).click({ force: true })
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark')
    expect(await page.evaluate(() => window.localStorage.getItem('dockrev:theme'))).toBe('dark')
  })

  test('custom delay validates input and reports errors', async ({ page }) => {
    await page.goto('/services?demo=services')
    await openInspector(page)

    await panel(page).getByRole('button', { name: /高级/ }).click()
    const input = panel(page).getByLabel('附加延迟（毫秒）')
    await input.fill('99999')
    await input.press('Enter')
    await expect(panel(page).getByText('请输入 0–30000 之间的整数毫秒数。')).toBeVisible()
    await expect(page).not.toHaveURL(/d_delay_ms/)

    await input.fill('1200')
    await input.press('Enter')
    await expect(page).toHaveURL(/d_delay=custom/)
    await expect(page).toHaveURL(/d_delay_ms=1200/)
  })

  test('simulated writes appear in the mutation log', async ({ page }) => {
    await page.goto('/cleanup?demo=cleanup')
    await expect(page.getByRole('button', { name: '重扫' }).first()).toBeVisible()

    const baseline = (await readMutations(page)).length
    await page.getByRole('button', { name: '重扫' }).first().click()
    await expect.poll(async () => (await readMutations(page)).length).toBeGreaterThan(baseline)

    await openInspector(page)
    await panel(page).locator('summary').first().click()
    await expect(panel(page).getByRole('region', { name: '模拟写入记录' }).locator('li').first()).toContainText('POST /api/')
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
  test('desktop panel opens from a bubble and collapses back', async ({ page, isMobile }) => {
    test.skip(isMobile, 'desktop-only contract')
    await page.goto('/services?demo=services')

    await openInspector(page)
    await panel(page).getByRole('button', { name: '收起为气泡' }).click()
    await expect(panel(page)).toHaveCount(0)
    await expect(page.getByRole('button', { name: '打开 Web Demo 调试控制面板' })).toBeVisible()
  })

  test('mobile entry opens a drawer instead of nothing', async ({ page, isMobile }) => {
    test.skip(!isMobile, 'mobile-only contract')
    await page.goto('/cleanup?demo=cleanup')
    await openInspector(page)
    await expect(page.locator('[data-demo-inspector="drawer"]')).toBeVisible()
    await panel(page).getByRole('button', { name: '关闭', exact: true }).click()
    await expect(panel(page)).toHaveCount(0)
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
