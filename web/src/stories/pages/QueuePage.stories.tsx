import type { Meta, StoryObj } from '@storybook/react'
import { QueuePage } from '../../pages/QueuePage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

const meta: Meta<typeof QueuePage> = {
  title: 'Pages/QueuePage',
  component: QueuePage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof QueuePage>

export const Default: Story = {
  parameters: { dockrevApiScenario: 'queue-mixed' },
  render: () => {
    return (
      <PageHarness route={{ name: 'queue' }} title="任务队列">
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const DashboardDemo: Story = {
  parameters: { dockrevApiScenario: 'dashboard-demo' },
  render: () => {
    return (
      <PageHarness route={{ name: 'queue' }} title="任务队列" pageSubtitle="代表性：任务列表（点击进入任务详情页查看日志）">
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const LegacyProgressFallback: Story = {
  parameters: { dockrevApiScenario: 'queue-legacy-progress' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'queue' }}
        title="任务队列"
        pageSubtitle="兼容场景：旧任务仅有 completed 进度字段，UI 自动回退 planned=completed"
      >
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const UpdateIndeterminate: Story = {
  parameters: { dockrevApiScenario: 'queue-update-indeterminate' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'queue' }}
        title="任务队列"
        pageSubtitle="运行中 update 在缺少可解析 pull 证据时应保持 indeterminate"
      >
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(120)

    const progressbar = canvasElement.querySelector('[role="progressbar"]') as HTMLElement | null
    if (!progressbar) {
      throw new globalThis.Error('queue progress bar missing')
    }
    if (!progressbar.className.includes('queueProgressBarIndeterminate')) {
      throw new globalThis.Error('queue progress bar should be indeterminate')
    }
    if (progressbar.getAttribute('aria-valuetext') !== '安排 running · 完成 40%') {
      throw new globalThis.Error('queue progress aria text should preserve indeterminate planned state')
    }
    const text = canvasElement.textContent ?? ''
    if (!text.includes('下载 已下载 4.2MB · layers 2/6')) {
      throw new globalThis.Error('queue page should render unknown-total download status')
    }
  },
}

export const UpdateDownloadDeterminate: Story = {
  parameters: { dockrevApiScenario: 'queue-update-download-determinate' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'queue' }}
        title="任务队列"
        pageSubtitle="运行中 stack update 在 pull 提供 current/total 时应显示真实下载百分比"
      >
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(120)

    const progressbar = canvasElement.querySelector('[role="progressbar"]') as HTMLElement | null
    if (!progressbar) {
      throw new globalThis.Error('queue progress bar missing')
    }
    if (progressbar.className.includes('queueProgressBarIndeterminate')) {
      throw new globalThis.Error('queue progress bar should be determinate when pull total is known')
    }
    if (progressbar.getAttribute('aria-valuetext') !== '安排 40% · 完成 40%') {
      throw new globalThis.Error('queue progress aria text should expose determinate planned state')
    }
    const text = canvasElement.textContent ?? ''
    if (!text.includes('下载 3.1MB / 5.9MB · layers 1/3')) {
      throw new globalThis.Error('queue page should render determinate download status')
    }
  },
}

export const ProgressSmoothing: Story = {
  parameters: { dockrevApiScenario: 'queue-progress-smoothing' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'queue' }}
        title="任务队列"
        pageSubtitle="演示：running 任务会自动推送进度，观察 420ms 宽度平滑过渡"
      >
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const ResultReasonRollback: Story = {
  parameters: { dockrevApiScenario: 'queue-health-rollback' },
  render: () => {
    return (
      <PageHarness
        route={{ name: 'queue' }}
        title="任务队列"
        pageSubtitle="终态任务在元信息下方展示结果原因摘要，并支持展开完整详情"
      >
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
  play: async ({ canvasElement }) => {
    await sleep(120)
    const text = canvasElement.textContent ?? ''
    if (!text.includes('健康检查失败，已回滚')) {
      throw new globalThis.Error('queue page should render rollback result reason summary')
    }
  },
}

export const Empty: Story = {
  parameters: { dockrevApiScenario: 'empty' },
  render: () => {
    return (
      <PageHarness route={{ name: 'queue' }} title="任务队列">
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const Error: Story = {
  parameters: { dockrevApiScenario: 'error' },
  render: () => {
    return (
      <PageHarness route={{ name: 'queue' }} title="任务队列">
        {({ onTopActions }) => <QueuePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}
