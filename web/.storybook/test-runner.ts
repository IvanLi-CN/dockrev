import type { TestRunnerConfig } from '@storybook/test-runner'

const config: TestRunnerConfig = {
  async postVisit(page, context) {
    if (context.id === 'pages-overviewpage--no-candidates-but-has-services') {
      const groupTitle = page.locator('.tableGroup .groupTitle').first()
      try {
        await groupTitle.waitFor({ state: 'visible', timeout: 6_000 })
      } catch {
        const errorText = (await page.locator('.error').first().textContent().catch(() => null)) ?? ''
        const noStacksVisible = await page
          .getByText('尚未注册 stack')
          .isVisible()
          .catch(() => false)
        throw new Error(
          `OverviewPage did not render any stack groups in time. error=${JSON.stringify(errorText)} noStacksVisible=${noStacksVisible}`,
        )
      }

      const groupMeta = page.locator('.tableGroup .groupMeta').first()

      const text = (await groupMeta.textContent()) ?? ''
      if (!text.includes('3 services')) {
        throw new Error(`Expected OverviewPage group summary to include "3 services", got: ${JSON.stringify(text)}`)
      }

      const groupHead = page.locator('.tableGroup .groupHead').first()
      await groupHead.click()

      const firstRow = page.locator('.tableGroup .rowLine').first()
      await firstRow.waitFor({ state: 'visible', timeout: 6_000 })
      const rowText = (await firstRow.textContent()) ?? ''
      if (!rowText.includes('无更新')) {
        throw new Error(`Expected expanded OverviewPage group to show ok rows with "无更新", got: ${JSON.stringify(rowText)}`)
      }
      return
    }

    if (context.id === 'pages-queuepage--default') {
      // queue-mixed: should have failed job + logs
      const filterBar = page.locator('.chipRow').first()
      await filterBar.waitFor({ state: 'visible', timeout: 6_000 })
      const filterFailed = filterBar.locator('button', { hasText: 'failed' }).first()
      await filterFailed.waitFor({ state: 'visible', timeout: 6_000 })
      await filterFailed.click()
      const anyFailed = await page.locator('.queueItem').first().isVisible().catch(() => false)
      if (!anyFailed) throw new Error('Expected at least one failed job in queue-mixed scenario.')

      await page.locator('.queueItem').first().click()
      const logs = page.locator('.logs')
      await logs.waitFor({ state: 'visible', timeout: 6_000 })
      const logsText = (await logs.textContent().catch(() => null)) ?? ''
      if (!logsText.includes('Backup failed')) {
        throw new Error(`Expected failed job logs to include "Backup failed", got: ${JSON.stringify(logsText)}`)
      }
      return
    }

    if (context.id === 'pages-servicedetailpage--updatable') {
      const bannerTitle = page.locator('.svcBannerTitle')
      await bannerTitle.waitFor({ state: 'visible', timeout: 6_000 })

      await page.getByRole('button', { name: '阻止此服务更新' }).click()
      await page.waitForTimeout(200)

      const text = (await bannerTitle.textContent().catch(() => null)) ?? ''
      if (!text.includes('已阻止')) {
        throw new Error(`Expected ServiceDetailPage to become blocked after clicking, got banner=${JSON.stringify(text)}`)
      }
      return
    }

    if (context.id === 'pages-interactiveapp--dashboard') {
      // App-level navigation should work (route changes without full reload)
      await page.getByRole('link', { name: '服务' }).click()
      const h1 = page.locator('.h1')
      await h1.waitFor({ state: 'visible', timeout: 6_000 })
      const t1 = (await h1.textContent().catch(() => null)) ?? ''
      if (!t1.includes('服务')) throw new Error(`Expected App to navigate to Services page, got title=${JSON.stringify(t1)}`)

      await page.getByRole('link', { name: '概览' }).click()
      const t2 = (await h1.textContent().catch(() => null)) ?? ''
      if (!t2.includes('概览')) throw new Error(`Expected App to navigate back to Overview, got title=${JSON.stringify(t2)}`)
      return
    }
  },
}

export default config
