import type { Meta, StoryObj } from '@storybook/react'
import { userEvent, within } from 'storybook/test'
import { GhcrWebhookRegistryPage } from '../../pages/GhcrWebhookRegistryPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof GhcrWebhookRegistryPage> = {
  title: 'Pages/GhcrWebhookRegistryPage',
  component: GhcrWebhookRegistryPage,
  decorators: [withDockrevMockApi],
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
}

export default meta
type Story = StoryObj<typeof GhcrWebhookRegistryPage>

export const Default: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'ghcr-webhook-registry' }}
        title="GHCR Webhook 维护"
        pageSubtitle="集中维护仓库 webhook 注册状态、重试注册与删除反注册任务"
      >
        {({ onTopActions }) => <GhcrWebhookRegistryPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Empty: Story = {
  parameters: { dockrevApiScenario: 'empty' },
  render: () => {
    return (
      <PageHarness route={{ name: 'ghcr-webhook-registry' }} title="GHCR Webhook 维护">
        {({ onTopActions }) => <GhcrWebhookRegistryPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const LargeDatasetPagination: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'ghcr-webhook-registry' }}
        title="GHCR Webhook 维护"
        pageSubtitle="204 个已跟踪仓库使用服务端分页与状态筛选"
      >
        {({ onTopActions }) => <GhcrWebhookRegistryPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    const waitForRows = async (count: number) => {
      for (let attempt = 0; attempt < 30; attempt += 1) {
        if (canvasElement.querySelectorAll('.ghcrRegistryRow').length === count) return
        await new Promise((resolve) => setTimeout(resolve, 25))
      }
      throw new globalThis.Error(`expected ${count} repository rows`)
    }

    await waitForRows(50)
    const pager = canvasElement.querySelector<HTMLElement>('[aria-label="仓库分页"]')
    if (!pager?.textContent?.includes('第 1 / 5 页，共 204 个仓库')) {
      throw new globalThis.Error('default pagination should expose the first 50-repository page')
    }
    const initialRepoRequest = globalThis.__DOCKREV_MOCK_DEBUG__?.ghcrReposUrls.at(-1) ?? ''
    if (!initialRepoRequest.includes('page=1') || !initialRepoRequest.includes('perPage=50') || !initialRepoRequest.includes('selectedFilter=selected')) {
      throw new globalThis.Error('registry should request only the current 50-repository selected page')
    }
    const activeJobRequests = globalThis.__DOCKREV_MOCK_DEBUG__?.jobsListUrls ?? []
    if (activeJobRequests.length !== 2 || !activeJobRequests.every((request) => request.includes('limit=200') && request.includes('type=github_packages_webhook%2Cgithub_packages_webhook_sync_all%2Cgithub_packages_webhook_sync_repo') && (request.includes('status=queued') || request.includes('status=running')))) {
      throw new globalThis.Error('registry should query only bounded queued and running GHCR jobs')
    }

    const next = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>('button')).find((button) => button.textContent?.trim() === '下一页')
    if (!next) throw new globalThis.Error('next page button missing')
    next.click()
    await waitForRows(50)
    if (!pager.textContent?.includes('第 2 / 5 页')) throw new globalThis.Error('next page should advance the registry page')

    const documentBody = within(document.body)
    const perPage = canvasElement.querySelector<HTMLButtonElement>('[aria-label="每页仓库数量"]')
    if (!perPage) throw new globalThis.Error('per-page select missing')
    await userEvent.click(perPage)
    await userEvent.click(documentBody.getByRole('option', { name: '100' }))
    await waitForRows(100)
    if (!pager.textContent?.includes('第 1 / 3 页')) throw new globalThis.Error('per-page changes should reset to the first page')

    next.click()
    await waitForRows(100)
    if (!pager.textContent?.includes('第 2 / 3 页')) throw new globalThis.Error('second page should retain the selected page size')
    next.click()
    await waitForRows(4)
    if (!pager.textContent?.includes('第 3 / 3 页')) throw new globalThis.Error('last page should expose only its remaining repositories')
    if (!next.disabled) throw new globalThis.Error('next page should be disabled on the last page')

    const previous = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>('button')).find((button) => button.textContent?.trim() === '上一页')
    if (!previous) throw new globalThis.Error('previous page button missing')
    previous.click()
    await waitForRows(100)
    if (!pager.textContent?.includes('第 2 / 3 页')) throw new globalThis.Error('previous page should return from the last page')

    const errorFilter = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>('button')).find((button) => button.textContent?.includes('失败') && !button.closest('[aria-hidden="true"]'))
    if (errorFilter) {
      await userEvent.click(errorFilter)
    } else {
      const filterSelect = canvasElement.querySelector<HTMLButtonElement>('#ghcr-registry-filter-select')
      if (!filterSelect) throw new globalThis.Error('error filter control missing')
      await userEvent.click(filterSelect)
      await userEvent.click(documentBody.getByRole('option', { name: /失败/ }))
    }
    await waitForRows(1)
    if (!pager.textContent?.includes('第 1 / 1 页，共 1 个仓库')) throw new globalThis.Error('state filtering should remain server paginated')

    const search = canvasElement.querySelector<HTMLInputElement>('input[placeholder*="owner/repo"]')
    if (!search) throw new globalThis.Error('registry search input missing')
    await userEvent.clear(search)
    await userEvent.type(search, 'permission denied')
    const submit = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>('button')).find((button) => button.textContent?.trim() === '搜索')
    if (!submit) throw new globalThis.Error('registry search button missing')
    await userEvent.click(submit)
    await waitForRows(1)
    if (!canvasElement.textContent?.includes('permission denied (mock)')) {
      throw new globalThis.Error('extended error search should return the matching repository')
    }
  },
}

export const RegistryLinks: Story = {
  parameters: { dockrevApiScenario: 'settings-configured' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'ghcr-webhook-registry' }}
        title="GHCR Webhook 维护"
        pageSubtitle="验证仓库标题链接与 webhook 页面跳转规则"
      >
        {({ onTopActions }) => <GhcrWebhookRegistryPage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await new Promise((resolve) => setTimeout(resolve, 120))

    const firstTitleLink = canvasElement.querySelector<HTMLAnchorElement>('.ghcrRegistryTitleLink')
    if (!firstTitleLink) throw new globalThis.Error('repo title link missing')
    const firstHref = firstTitleLink.getAttribute('href')
    if (firstHref !== 'https://github.com/IvanLi-CN/dockrev') {
      throw new globalThis.Error(`unexpected repo link href: ${firstHref ?? '<null>'}`)
    }

    const rows = Array.from(canvasElement.querySelectorAll<HTMLElement>('.ghcrRegistryRow'))
    if (rows.length < 2) throw new globalThis.Error('insufficient rows for webhook link checks')

    const originalOpen = window.open
    const openedUrls: string[] = []
    window.open = ((url?: string | URL) => {
      openedUrls.push(String(url ?? ''))
      return null
    }) as typeof window.open

    try {
      const firstWebhookButton = rows[0]?.querySelector<HTMLButtonElement>('button[aria-label="Webhook 页面"]')
      if (!firstWebhookButton) throw new globalThis.Error('first row webhook button missing')
      firstWebhookButton.click()

      const secondWebhookButton = rows[1]?.querySelector<HTMLButtonElement>('button[aria-label="Webhook 页面"]')
      if (!secondWebhookButton) throw new globalThis.Error('second row webhook button missing')
      secondWebhookButton.click()

      if (openedUrls[0] !== 'https://github.com/IvanLi-CN/dockrev/settings/hooks/1234567') {
        throw new globalThis.Error(`unexpected hook detail URL: ${openedUrls[0] ?? '<null>'}`)
      }
      if (openedUrls[1] !== 'https://github.com/IvanLi-CN/dockrev-supervisor/settings/hooks') {
        throw new globalThis.Error(`unexpected hook list URL: ${openedUrls[1] ?? '<null>'}`)
      }
    } finally {
      window.open = originalOpen
    }
  },
}
