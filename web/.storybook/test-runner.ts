import type { TestRunnerConfig } from '@storybook/test-runner'

const config: TestRunnerConfig = {
  async postVisit(page, context) {
    if (context.id !== 'pages-overviewpage--no-candidates-but-has-services') return

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
  },
}

export default config
