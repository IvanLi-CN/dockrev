import type { Meta, StoryObj } from '@storybook/react'
import { VersionInferencePage } from '../../pages/VersionInferencePage'
import { PageHarness } from '../mocks/PageHarness'
import { withDockrevMockApi } from '../mocks/withDockrevMockApi'

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

const meta: Meta<typeof VersionInferencePage> = {
  title: 'Pages/VersionInferencePage',
  component: VersionInferencePage,
  decorators: [withDockrevMockApi],
}

export default meta
type Story = StoryObj<typeof VersionInferencePage>

export const Idle: Story = {
  parameters: { dockrevApiScenario: 'version-inference-idle' },
  render: () => {
    return (
      <PageHarness route={{ name: 'version-inference' }} title="版本推测" pageSubtitle="空闲态：仅缓存快照，无 in-flight 任务">
        {({ onTopActions }) => <VersionInferencePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const ColdLoading: Story = {
  parameters: {
    dockrevApiScenario: 'version-inference-running',
    dockrevApiBehaviorByRoute: {
      'GET /api/version-inference/overview': { delayMs: 900 },
    },
  },
  render: () => (
    <PageHarness route={{ name: 'version-inference' }} title="版本推测">
      {({ onTopActions }) => <VersionInferencePage onTopActions={onTopActions} />}
    </PageHarness>
  ),
  play: async ({ canvasElement }) => {
    await sleep(100)
    if (!canvasElement.querySelector('[data-async-data-phase="initial-loading"] .skeleton')) {
      throw new Error('cold version inference must render a skeleton')
    }
    if (canvasElement.textContent?.includes('并发上限0')) {
      throw new Error('cold version inference must not report zero metrics')
    }
  },
}

export const InitialErrorAndRetry: Story = {
  parameters: {
    dockrevApiScenario: 'version-inference-running',
    dockrevApiBehaviorByRoute: {
      'GET /api/version-inference/overview': {
        delayMs: 100,
        failTimes: 1,
        failureStatus: 503,
        failureBody: { error: 'mock inference unavailable' },
      },
    },
  },
  render: () => (
    <PageHarness route={{ name: 'version-inference' }} title="版本推测">
      {({ onTopActions }) => <VersionInferencePage onTopActions={onTopActions} />}
    </PageHarness>
  ),
  play: async ({ canvasElement }) => {
    await sleep(180)
    const retry = canvasElement.querySelector<HTMLButtonElement>('[aria-label="重试加载"]')
    if (!retry || !canvasElement.querySelector('[role="alert"]')) {
      throw new Error('initial inference failure must expose an error overlay and retry')
    }
    retry.click()
    await sleep(160)
    if (!canvasElement.textContent?.includes('统一状态列表')) {
      throw new Error('retry should recover the version inference data region')
    }
  },
}

export const Running: Story = {
  parameters: { dockrevApiScenario: 'version-inference-running' },
  render: () => {
    return (
      <PageHarness route={{ name: 'version-inference' }} title="版本推测" pageSubtitle="运行态：展示镜像级实时进度">
        {({ onTopActions }) => <VersionInferencePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const QueueBacklog: Story = {
  parameters: { dockrevApiScenario: 'version-inference-queue-backlog' },
  render: () => {
    return (
      <PageHarness route={{ name: 'version-inference' }} title="版本推测" pageSubtitle="队列堆积：queued 主导，便于观察排队压力">
        {({ onTopActions }) => <VersionInferencePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}

export const StaleAndAllFailed: Story = {
  parameters: { dockrevApiScenario: 'version-inference-stale-all-failed' },
  render: () => {
    return (
      <PageHarness route={{ name: 'version-inference' }} title="版本推测" pageSubtitle="异常态：需处理缓存与失败任务混合展示">
        {({ onTopActions }) => <VersionInferencePage onTopActions={onTopActions} />}
      </PageHarness>
    )
  },
}
