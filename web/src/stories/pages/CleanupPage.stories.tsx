import type { Meta, StoryObj } from '@storybook/react'
import { CleanupPage } from '../../pages/CleanupPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

const meta: Meta<typeof CleanupPage> = {
  title: 'Pages/CleanupPage',
  component: CleanupPage,
  decorators: [withDockrevMockApi],
  tags: ['autodocs'],
  parameters: {
    docs: {
      description: {
        component: 'Cleanup 页默认走 autodocs；这里补充顶部动作图标、扫描态、未知大小文案与确认弹窗行为的稳定回归覆盖。',
      },
    },
  },
}

export default meta
type Story = StoryObj<typeof CleanupPage>

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

async function waitForCondition(check: () => boolean, timeoutMs = 3000): Promise<void> {
  const started = Date.now()
  while (!check()) {
    if (Date.now() - started > timeoutMs) throw new globalThis.Error('condition timeout')
    await sleep(60)
  }
}

function assertStory(condition: unknown, message: string): asserts condition {
  if (!condition) throw new globalThis.Error(message)
}

function findButton(root: ParentNode, text: string): HTMLButtonElement | null {
  return (
    Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find(
      (button) => button.textContent?.replace(/\s+/g, ' ').trim() === text,
    ) ?? null
  )
}

function assertButtonHasIcon(root: ParentNode, text: string) {
  const button = findButton(root, text)
  assertStory(button, `${text} button missing`)
  assertStory(button.querySelector('svg'), `${text} button should render a leading icon`)
}

function renderPage() {
  return (
    <PageHarness
      route={{ name: 'cleanup' }}
      title="清理"
      topbarHint="Docker 清理控制台"
      pageSubtitle="按规则预览 docker prune 候选，支持全局 / Stack / 服务三级清理"
    >
      {({ onLastScanHint, onTopActions }) => (
        <CleanupPage onLastScanHint={onLastScanHint} onTopActions={onTopActions} />
      )}
    </PageHarness>
  )
}

export const Default: Story = {
  parameters: { dockrevApiScenario: 'cleanup-console' },
  render: renderPage,
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => canvasElement.textContent?.includes('均衡') ?? false)

    const activeNav = Array.from(doc.querySelectorAll<HTMLElement>('.navItemActive')).find((node) =>
      node.textContent?.includes('清理'),
    )
    assertStory(activeNav, 'cleanup nav item should be active')
    assertStory(canvasElement.textContent?.includes('旧镜像未被任何容器使用'), 'resource reason should be visible')
    assertStory(!(canvasElement.textContent?.includes('服务直属候选') ?? false), 'generic candidate copy should be removed')
    assertStory(canvasElement.textContent?.includes('空间概览'), 'cleanup summary section should be visible')
    assertStory(canvasElement.textContent?.includes('容器'), 'cleanup container card should be visible')
    assertStory(canvasElement.textContent?.includes('镜像'), 'cleanup image card should be visible')
    assertStory(canvasElement.textContent?.includes('卷'), 'cleanup volume card should be visible')
    assertStory(!(canvasElement.textContent?.includes('待估') ?? false), 'cleanup copy should avoid the old pending-estimate wording')
    assertButtonHasIcon(doc, '全部')
    assertButtonHasIcon(doc, '重扫')
  },
}

export const Empty: Story = {
  parameters: { dockrevApiScenario: 'cleanup-console-empty' },
  render: renderPage,
}

export const ScanningState: Story = {
  parameters: { dockrevApiScenario: 'cleanup-console-scan-pending' },
  render: renderPage,
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => canvasElement.textContent?.includes('正在扫描可清理资源…') ?? false)

    const allButton = findButton(doc, '全部')
    const refreshButton = findButton(doc, '重扫')
    assertStory(allButton, 'topbar cleanup action should stay visible while scanning')
    assertStory(refreshButton, 'topbar rescan action should stay visible while scanning')
    assertStory(allButton.disabled, 'cleanup action should be disabled during initial scan')
    assertStory(refreshButton.disabled, 'rescan action should be disabled during initial scan')
    assertStory(refreshButton.getAttribute('aria-busy') === 'true', 'rescan action should expose busy state during initial scan')
  },
}

export const RescanningState: Story = {
  parameters: { dockrevApiScenario: 'cleanup-console-scan-slow' },
  render: renderPage,
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => canvasElement.textContent?.includes('空间概览') ?? false)

    const refreshButton = findButton(doc, '重扫')
    assertStory(refreshButton, 'rescan action missing')
    refreshButton.click()

    await waitForCondition(() => findButton(doc, '重扫')?.getAttribute('aria-busy') === 'true')
    assertStory(findButton(doc, '全部')?.disabled === true, 'cleanup action should be disabled while rescanning')
    assertStory(findButton(doc, '重扫')?.disabled === true, 'rescan action should be disabled while rescanning')
  },
}

export const AggressiveUnowned: Story = {
  parameters: { dockrevApiScenario: 'cleanup-console-aggressive-unowned' },
  render: renderPage,
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => canvasElement.textContent?.includes('均衡') ?? false)

    const aggressiveTab = findButton(doc, '激进')
    assertStory(aggressiveTab, 'aggressive tab missing')
    aggressiveTab.click()

    await waitForCondition(() => canvasElement.textContent?.includes('未归属资源') ?? false)
    assertStory(canvasElement.textContent?.includes('仅全部'), 'unowned group badge missing')
  },
}

export const ConfirmDialogLatestScan: Story = {
  parameters: { dockrevApiScenario: 'cleanup-console' },
  render: renderPage,
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '全部') != null)

    const trigger = findButton(doc, '全部')
    assertStory(trigger, 'topbar cleanup action missing')
    trigger.click()

    await waitForCondition(() => doc.body.textContent?.includes('确认清理全部') ?? false)
    assertStory(doc.body.textContent?.includes('最新扫描'), 'confirm dialog should show latest scan timestamp')
    assertStory(doc.body.textContent?.includes('预计释放'), 'confirm dialog should show reclaim estimate')
  },
}

export const StaleFingerprintRetry: Story = {
  parameters: { dockrevApiScenario: 'cleanup-console-stale' },
  render: renderPage,
  play: async ({ canvasElement }) => {
    const doc = canvasElement.ownerDocument
    await waitForCondition(() => findButton(doc, '全部') != null)

    const trigger = findButton(doc, '全部')
    assertStory(trigger, 'topbar cleanup action missing')
    trigger.click()

    await waitForCondition(() => doc.body.textContent?.includes('确认清理全部') ?? false)
    const firstConfirm = findButton(doc, '确认清理')
    assertStory(firstConfirm, 'confirm dialog action missing')
    firstConfirm.click()

    await waitForCondition(() => doc.body.textContent?.includes('候选已变化') ?? false)
    const secondConfirm = findButton(doc, '确认清理')
    assertStory(secondConfirm, 'stale confirm action missing')
    secondConfirm.click()

    await waitForCondition(() => window.location.hash.includes('/queue/job-cleanup-1'))
  },
}

export const UsageOverviewFocus: Story = {
  parameters: { dockrevApiScenario: 'cleanup-console-aggressive-unowned' },
  render: renderPage,
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('空间概览') ?? false)
    assertStory(canvasElement.textContent?.includes('其他'), 'cleanup other card should be visible in focus story')
    assertStory(canvasElement.textContent?.includes('Docker 清理候选'), 'cleanup summary pill should be visible')
    assertStory(canvasElement.textContent?.includes('大小未知'), 'cleanup focus story should expose the refined unknown-size copy')
  },
}
