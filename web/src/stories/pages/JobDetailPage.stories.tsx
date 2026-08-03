import type { Meta, StoryObj } from '@storybook/react'
import type { ReactNode } from 'react'
import { JobDetailPage } from '../../pages/JobDetailPage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'
import { expectNearlyEqual, expectStory, findButton, waitForCondition } from './storyAssertions'

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function getLogsSurface(root: ParentNode): HTMLElement | null {
  return root.querySelector<HTMLElement>('[data-job-detail-log-surface="true"]')
}

function getLogsViewport(root: ParentNode): HTMLElement | null {
  return root.querySelector<HTMLElement>('[aria-label="任务日志"]')
}

function getMainViewport(root: ParentNode): HTMLElement | null {
  return root.querySelector<HTMLElement>('[aria-label="主内容"]')
}

function getLogCount(root: ParentNode): number {
  return Number(getLogsSurface(root)?.getAttribute('data-job-detail-log-count') ?? '0')
}

function isNearBottom(element: HTMLElement): boolean {
  return element.scrollHeight - element.scrollTop - element.clientHeight < 48
}

const meta: Meta<typeof JobDetailPage> = {
  title: 'Pages/JobDetailPage',
  component: JobDetailPage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof JobDetailPage>

function renderJobDetailSurface(content: ReactNode) {
  return <div style={{ height: '900px' }}>{content}</div>
}

function renderLongLogsPage(subtitle: string) {
  return renderJobDetailSurface(
    <PageHarness
      route={{ name: 'job', jobId: 'job-live-long' }}
      title="任务详情"
      pageSubtitle={subtitle}
    >
      {({ onTopActions }) => <JobDetailPage jobId="job-live-long" onTopActions={onTopActions} />}
    </PageHarness>
  )
}

export const LongLogs: Story = {
  parameters: { dockrevApiScenario: 'queue-long-logs' },
  render: () => renderLongLogsPage('代表性：长 URL / digest / 多行日志（堆栈/命令输出）应在容器内滚动，且 live tail 默认跟随最新'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => getLogCount(canvasElement) >= 105)
    await waitForCondition(() => Boolean(getMainViewport(canvasElement)))
    await waitForCondition(() => Boolean(getLogsViewport(canvasElement)))

    const mainViewport = getMainViewport(canvasElement)
    const viewport = getLogsViewport(canvasElement)
    expectStory(mainViewport, 'job detail main viewport missing')
    expectStory(viewport, 'job detail logs viewport missing')
    expectStory(
      mainViewport.scrollHeight <= mainViewport.clientHeight + 2,
      'job detail page should fit the main viewport without introducing page-level scroll in the common long-logs case',
    )
    expectStory(viewport.scrollHeight > viewport.clientHeight, 'job detail logs viewport should remain independently scrollable')

    await waitForCondition(() => isNearBottom(viewport))
    expectStory(getLogsSurface(canvasElement)?.getAttribute('data-job-detail-log-follow') === 'true', 'job detail logs should follow by default')

    viewport.scrollTop = Math.max(0, viewport.scrollTop - 240)
    viewport.dispatchEvent(new Event('scroll'))

    await waitForCondition(() => getLogsSurface(canvasElement)?.getAttribute('data-job-detail-log-follow') === 'false')
    await waitForCondition(() => Boolean(findButton(canvasElement, '跳到最新')))

    const pausedScrollTop = viewport.scrollTop
    const pausedCount = getLogCount(canvasElement)

    await waitForCondition(() => getLogCount(canvasElement) > pausedCount, 5_000)
    expectNearlyEqual(viewport.scrollTop, pausedScrollTop, 6, 'paused follow should preserve the reader scroll position when new logs arrive')
    expectStory(!isNearBottom(viewport), 'paused follow should keep the viewport away from the bottom')

    findButton(canvasElement, '跳到最新')?.click()

    await waitForCondition(() => getLogsSurface(canvasElement)?.getAttribute('data-job-detail-log-follow') === 'true')
    await waitForCondition(() => isNearBottom(viewport))
    await waitForCondition(() => !findButton(canvasElement, '跳到最新'))

    const resumedCount = getLogCount(canvasElement)
    await waitForCondition(() => getLogCount(canvasElement) > resumedCount, 5_000)
    expectStory(isNearBottom(viewport), 'resumed follow should keep the viewport pinned to the latest log line')
  },
}

export const LongLogsPausedFollowEvidence: Story = {
  parameters: { dockrevApiScenario: 'queue-long-logs' },
  render: () => renderLongLogsPage('视觉证据：用户上滚查看旧日志时暂停跟随，并提供跳到最新入口'),
}

export const LiveOutputAndEventToggle: Story = {
  parameters: { dockrevApiScenario: 'queue-long-logs' },
  render: () => renderLongLogsPage('实时终端快照替换进度行；EVEN 默认隐藏并可按浏览器偏好打开'),
  beforeEach: () => {
    try {
      window.localStorage.removeItem('dockrev.job-detail.show-events')
    } catch {
      // Storybook should still exercise the default when storage is unavailable.
    }
  },
  play: async ({ canvasElement }) => {
    await waitForCondition(() => getLogCount(canvasElement) >= 105)
    expectStory(!canvasElement.querySelector('.logLine-event'), 'EVEN logs should be hidden by default')
    await waitForCondition(() => Boolean(canvasElement.querySelector('.logLine-terminal')))
    expectStory(
      canvasElement.querySelectorAll('.logLine-terminal .logLvl-warn, .logLine-terminal .logLvl-warning').length === 0,
      'live terminal rows should not be classified as WARN',
    )
    expectStory(
      [...canvasElement.querySelectorAll('.logLine-terminal .logLvl')].every((node) => node.textContent === ''),
      'live terminal rows should keep an empty fixed-width level column',
    )

    const toggle = canvasElement.querySelector<HTMLElement>('[data-job-detail-log-show-events="true"]')
    expectStory(toggle, 'EVEN visibility switch missing')
    toggle?.click()
    await waitForCondition(() => Boolean(canvasElement.querySelector('.logLine-event')))
    expectStory(
      canvasElement.textContent?.includes('event audit: registry snapshot was refreshed') === true,
      'EVEN log should appear after enabling the switch',
    )

    await waitForCondition(
      () => (canvasElement.textContent?.match(/live registry polling continues for the newest digest candidate/g) ?? []).length > 0,
      5_000,
    )
    expectStory(
      (canvasElement.textContent?.match(/status=0 stdout=stream tick/g) ?? []).length === 0,
      'live output should suppress the following persisted command summary on the same connection',
    )
  },
}

export const RunningDualProgress: Story = {
  parameters: { dockrevApiScenario: 'queue-long-logs' },
  render: () => {
    return renderJobDetailSurface(
      <PageHarness
        route={{ name: 'job', jobId: 'job-short' }}
        title="任务详情"
        pageSubtitle="运行中：安排进度与完成进度同时显示"
      >
        {({ onTopActions }) => <JobDetailPage jobId="job-short" onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const UpdateLayerProgress: Story = {
  parameters: { dockrevApiScenario: 'queue-update-layer-progress' },
  render: () => {
    return renderJobDetailSurface(
      <PageHarness
        route={{ name: 'job', jobId: 'job-running' }}
        title="任务详情"
        pageSubtitle="运行中 update 缺少总字节但有 layers 证据时应显示保守进度"
      >
        {({ onTopActions }) => <JobDetailPage jobId="job-running" onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(120)

    const progressbar = canvasElement.querySelector('[role="progressbar"]') as HTMLElement | null
    if (!progressbar) {
      throw new globalThis.Error('progress bar missing')
    }
    if (progressbar.className.includes('jobProgressBarIndeterminate')) {
      throw new globalThis.Error('progress bar should be determinate when layer progress is available')
    }
    if (progressbar.getAttribute('aria-valuetext') !== '安排 40% · 完成 40%') {
      throw new globalThis.Error('progress aria text should expose layer-derived determinate state')
    }
    const pageText = canvasElement.textContent ?? ''
    if (!pageText.includes('下载')) {
      throw new globalThis.Error('download label missing')
    }
    if (!pageText.includes('已下载 4.2MB · layers 2/6')) {
      throw new globalThis.Error('job detail should render unknown-total download status')
    }
  },
}

export const UpdateDownloadDeterminate: Story = {
  parameters: { dockrevApiScenario: 'queue-update-download-determinate' },
  render: () => {
    return renderJobDetailSurface(
      <PageHarness
        route={{ name: 'job', jobId: 'job-running' }}
        title="任务详情"
        pageSubtitle="运行中 stack update 在 pull 提供 current/total 时应显示真实下载百分比"
      >
        {({ onTopActions }) => <JobDetailPage jobId="job-running" onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(120)

    const progressbar = canvasElement.querySelector('[role="progressbar"]') as HTMLElement | null
    if (!progressbar) {
      throw new globalThis.Error('progress bar missing')
    }
    if (progressbar.className.includes('jobProgressBarIndeterminate')) {
      throw new globalThis.Error('progress bar should be determinate when pull total is known')
    }
    if (progressbar.getAttribute('aria-valuetext') !== '安排 40% · 完成 40%') {
      throw new globalThis.Error('progress aria text should expose determinate planned state')
    }
    const pageText = canvasElement.textContent ?? ''
    if (!pageText.includes('3.1MB / 5.9MB · layers 1/3')) {
      throw new globalThis.Error('job detail should render determinate download status')
    }
  },
}

export const LegacyProgressFallback: Story = {
  parameters: { dockrevApiScenario: 'queue-legacy-progress' },
  render: () => {
    return renderJobDetailSurface(
      <PageHarness
        route={{ name: 'job', jobId: 'job-legacy-running' }}
        title="任务详情"
        pageSubtitle="兼容场景：旧任务缺失 planned* 字段时，UI 自动回退 planned=completed"
      >
        {({ onTopActions }) => <JobDetailPage jobId="job-legacy-running" onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const HealthRollback: Story = {
  parameters: { dockrevApiScenario: 'queue-health-rollback' },
  render: () => {
    return renderJobDetailSurface(
      <PageHarness
        route={{ name: 'job', jobId: 'job-health-rollback' }}
        title="任务详情"
        pageSubtitle="健康检查失败后已回滚：进度与日志都应明确表达 rollback，而不是误报 passed"
      >
        {({ onTopActions }) => <JobDetailPage jobId="job-health-rollback" onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(120)

    const pageText = canvasElement.textContent ?? ''
    if (!pageText.includes('rolled_back')) {
      throw new globalThis.Error('rolled_back status pill missing')
    }
    if (!pageText.includes('update rolled back after healthcheck failure')) {
      throw new globalThis.Error('final rollback progress message missing')
    }
    if (!pageText.includes('healthcheck failed for api; rolling back')) {
      throw new globalThis.Error('healthcheck failure log missing')
    }
    if (!pageText.includes('结果原因')) {
      throw new globalThis.Error('result reason section missing')
    }
    if (!pageText.includes('健康检查失败，已回滚')) {
      throw new globalThis.Error('friendly rollback reason missing')
    }
    if (pageText.includes('healthcheck passed for api')) {
      throw new globalThis.Error('healthcheck passed log should not appear after rollback')
    }
  },
}
