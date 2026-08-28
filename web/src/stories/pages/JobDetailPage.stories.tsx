import type { Meta, StoryObj } from '@storybook/react'
import type { ReactNode } from 'react'
import { JobDetailPage } from '../../pages/JobDetailPage'
import { PageHarness } from '../mocks/PageHarness'
import { buildQueueHealthRollback, RUNNING_JOB_ID } from '../mocks/dockrevMockApi/fixturesQueues'
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

function getJobDetailCards(root: ParentNode): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>('.jobDetailDataRegion > .card'))
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
    const cards = getJobDetailCards(canvasElement)
    expectStory(cards.length === 2, 'job detail should render summary and logs cards in the data region')
    expectNearlyEqual(
      cards[1]!.getBoundingClientRect().top - cards[0]!.getBoundingClientRect().bottom,
      16,
      1,
      'job detail cards should keep a 16px vertical gap',
    )
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
    const terminalRowCount = canvasElement.querySelectorAll('.logLine-terminal').length
    await sleep(4_500)
    expectStory(
      canvasElement.querySelectorAll('.logLine-terminal').length === terminalRowCount,
      'one running terminal command should replace its snapshot instead of stacking frozen progress rows',
    )
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

export const BackupProgress: Story = {
  parameters: { dockrevApiScenario: 'queue-backup-progress' },
  render: () => renderLongLogsPage('运行中备份'),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('zstd-size') === true)
    await waitForCondition(() => {
      const value = Number(canvasElement.querySelector('[role="progressbar"]')?.getAttribute('aria-valuenow'))
      return Number.isFinite(value) && value > 24
    })
    const terminalRows = canvasElement.querySelectorAll('.logLine-terminal')
    expectStory(terminalRows.length > 0, 'backup progress terminal row missing')
    const initialCount = terminalRows.length
    await sleep(1_200)
    expectStory(
      canvasElement.querySelectorAll('.logLine-terminal').length === initialCount,
      'backup progress should replace the current command snapshot',
    )
  },
}

export const BackupProgressMobileReducedMotion: Story = {
  ...BackupProgress,
  parameters: {
    ...BackupProgress.parameters,
    viewport: { defaultViewport: 'mobile1' },
    reducedMotion: 'reduce',
  },
}

export const CompactSuccessfulPullHistory: Story = {
  parameters: { dockrevApiScenario: 'queue-long-logs' },
  render: () => renderJobDetailSurface(
    <PageHarness
      route={{ name: 'job', jobId: 'job-long' }}
      title="任务详情"
      pageSubtitle="历史成功 pull 只保留退出状态，不补播 transient 下载进度"
    >
      {({ onTopActions }) => <JobDetailPage jobId="job-long" onTopActions={onTopActions} />}
    </PageHarness>,
  ),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => canvasElement.textContent?.includes('status=0 stdout= stderr=') === true)
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
  play: async ({ canvasElement }) => {
    findButton(canvasElement, '刷新')?.click()
    await waitForCondition(() => Boolean(canvasElement.querySelector('.jobProgressBarDual')))
  },
}

export const UpdateLayerProgress: Story = {
  parameters: { dockrevApiScenario: 'queue-update-layer-progress' },
  render: () => {
    return renderJobDetailSurface(
      <PageHarness
        route={{ name: 'job', jobId: RUNNING_JOB_ID }}
        title="任务详情"
        pageSubtitle="运行中 update 缺少总字节但有 layers 证据时应显示保守进度"
      >
        {({ onTopActions }) => <JobDetailPage jobId={RUNNING_JOB_ID} onTopActions={onTopActions} />}
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

export const UpdateStopAvailable: Story = {
  parameters: { dockrevApiScenario: 'queue-update-layer-progress' },
  render: () => renderJobDetailSurface(
    <PageHarness route={{ name: 'job', jobId: RUNNING_JOB_ID }} title="任务详情" pageSubtitle="更新实际应用前可立即停止">
      {({ onTopActions }) => <JobDetailPage jobId={RUNNING_JOB_ID} onTopActions={onTopActions} />}
    </PageHarness>,
  ),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => Boolean(canvasElement.querySelector('[aria-label="停止更新"]')))
    const stop = canvasElement.querySelector<HTMLElement>('[aria-label="停止更新"]')
    expectStory(stop, 'stop update button should be visible before apply')
    stop.click()
    await waitForCondition(() => {
      const requested = canvasElement.querySelector<HTMLButtonElement>('[aria-label="正在停止"]')
      return requested?.disabled === true
    })
  },
}

export const UpdateStopAvailableEvidence: Story = {
  parameters: { dockrevApiScenario: 'queue-update-layer-progress' },
  render: () => renderJobDetailSurface(
    <PageHarness route={{ name: 'job', jobId: RUNNING_JOB_ID }} title="任务详情" pageSubtitle="更新实际应用前可立即停止">
      {({ onTopActions }) => <JobDetailPage jobId={RUNNING_JOB_ID} onTopActions={onTopActions} />}
    </PageHarness>,
  ),
}

export const UpdateStopCancelled: Story = {
  parameters: { dockrevApiScenario: 'queue-update-cancelled' },
  render: () => renderJobDetailSurface(
    <PageHarness route={{ name: 'job', jobId: RUNNING_JOB_ID }} title="任务详情" pageSubtitle="停止完成后保留终态，停止按钮保持禁用">
      {({ onTopActions }) => <JobDetailPage jobId={RUNNING_JOB_ID} onTopActions={onTopActions} />}
    </PageHarness>,
  ),
  play: async ({ canvasElement }) => {
    await waitForCondition(() => {
      const stopped = canvasElement.querySelector<HTMLButtonElement>('[aria-label="已停止"]')
      return stopped?.disabled === true
    })
    expectStory(!canvasElement.querySelector('[aria-label="停止更新"]'), 'cancelled update must not offer stop again')
  },
}

export const UpdateDownloadDeterminate: Story = {
  parameters: { dockrevApiScenario: 'queue-update-download-determinate' },
  render: () => {
    return renderJobDetailSurface(
      <PageHarness
        route={{ name: 'job', jobId: RUNNING_JOB_ID }}
        title="任务详情"
        pageSubtitle="运行中 stack update 在 pull 提供 current/total 时应显示真实下载百分比"
      >
        {({ onTopActions }) => <JobDetailPage jobId={RUNNING_JOB_ID} onTopActions={onTopActions} />}
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
  play: async ({ canvasElement }) => {
    findButton(canvasElement, '刷新')?.click()
    await waitForCondition(() => Boolean(canvasElement.querySelector('.jobProgressBarDual')))
  },
}

export const HealthRollback: Story = {
  parameters: {
    dockrevApiScenario: 'queue-health-rollback',
    viewport: { defaultViewport: 'dockrevMobile' },
  },
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
    if (!pageText.includes('归档可用') || !findButton(canvasElement, '下载证据')) {
      throw new globalThis.Error('rollback evidence download affordance missing')
    }
    if (pageText.includes('healthcheck passed for api')) {
      throw new globalThis.Error('healthcheck passed log should not appear after rollback')
    }
  },
}

const incompleteEvidenceFixture = buildQueueHealthRollback()
const incompleteEvidenceJob = incompleteEvidenceFixture.jobById['job-health-rollback']
if (incompleteEvidenceJob && typeof incompleteEvidenceJob.summary === 'object' && incompleteEvidenceJob.summary !== null) {
  const existingSummary = incompleteEvidenceJob.summary as Record<string, unknown>
  incompleteEvidenceJob.summary = {
    ...existingSummary,
    rollbackEvidence: {
      status: 'incomplete',
      failedCandidates: 1,
      archiveFormat: 'tar',
      compression: 'zstd',
      archiveSizeBytes: null,
      services: [],
      errors: ['archive unavailable'],
    },
  }
  incompleteEvidenceFixture.jobs = incompleteEvidenceFixture.jobs.map((job) => job.id === incompleteEvidenceJob.id ? { ...job, summary: incompleteEvidenceJob.summary } : job)
}

export const HealthRollbackEvidenceIncomplete: Story = {
  parameters: {
    dockrevApiScenario: 'queue-health-rollback',
    dockrevInitialFixture: incompleteEvidenceFixture,
  },
  render: () => renderJobDetailSurface(
    <PageHarness route={{ name: 'job', jobId: 'job-health-rollback' }} title="任务详情" pageSubtitle="证据归档未完成时保留状态，但不提供下载入口">
      {({ onTopActions }) => <JobDetailPage jobId="job-health-rollback" onTopActions={onTopActions} />}
    </PageHarness>,
  ),
  play: async ({ canvasElement }) => {
    await sleep(120)
    const pageText = canvasElement.textContent ?? ''
    if (!pageText.includes('归档不完整') || findButton(canvasElement, '下载证据')) {
      throw new globalThis.Error('incomplete rollback evidence state is incorrect')
    }
  },
}

const absentEvidenceFixture = buildQueueHealthRollback()
const absentEvidenceJob = absentEvidenceFixture.jobById['job-health-rollback']
if (absentEvidenceJob && typeof absentEvidenceJob.summary === 'object' && absentEvidenceJob.summary !== null) {
  const summaryWithoutEvidence = { ...(absentEvidenceJob.summary as Record<string, unknown>) }
  delete summaryWithoutEvidence.rollbackEvidence
  absentEvidenceJob.summary = summaryWithoutEvidence
  absentEvidenceFixture.jobs = absentEvidenceFixture.jobs.map((job) => job.id === absentEvidenceJob.id ? { ...job, summary: summaryWithoutEvidence } : job)
}

export const HealthRollbackEvidenceAbsent: Story = {
  parameters: {
    dockrevApiScenario: 'queue-health-rollback',
    dockrevInitialFixture: absentEvidenceFixture,
  },
  render: () => renderJobDetailSurface(
    <PageHarness route={{ name: 'job', jobId: 'job-health-rollback' }} title="任务详情" pageSubtitle="没有候选证据时不显示空的归档入口">
      {({ onTopActions }) => <JobDetailPage jobId="job-health-rollback" onTopActions={onTopActions} />}
    </PageHarness>,
  ),
  play: async ({ canvasElement }) => {
    await sleep(120)
    if (canvasElement.textContent?.includes('回滚证据') || findButton(canvasElement, '下载证据')) {
      throw new globalThis.Error('absent rollback evidence should not render an evidence section')
    }
  },
}
