import { useEffect, useState } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import { AsyncDataRegion, AsyncDataSkeleton } from '../../components/AsyncDataRegion'
import type { AsyncDataPhase, AsyncDataSource } from '../../asyncData'

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function RegionContents() {
  return (
    <div className="card" style={{ minHeight: 180 }}>
      <div className="sectionRow">
        <div>
          <div className="title">服务运行态</div>
          <div className="muted">最近一次成功读取的数据仍可操作</div>
        </div>
      </div>
      <div className="chipRow" style={{ marginTop: 16 }}>
        <div className="chipStatic">运行中: 3</div>
        <div className="chipStatic">失败: 0</div>
      </div>
    </div>
  )
}

function RegionPreview(props: {
  phase: AsyncDataPhase
  source?: AsyncDataSource
  hasData?: boolean
  error?: string
}) {
  return (
    <AsyncDataRegion
      error={props.error}
      hasData={props.hasData}
      label="正在同步服务运行态"
      onRetry={() => undefined}
      phase={props.phase}
      skeleton={<AsyncDataSkeleton lines={5} />}
      source={props.source}
    >
      <RegionContents />
    </AsyncDataRegion>
  )
}

function TimedRefreshPreview(props: { source: AsyncDataSource }) {
  const [phase, setPhase] = useState<AsyncDataPhase>('ready-data')

  useEffect(() => {
    const timer = window.setTimeout(() => setPhase('refreshing'), 24)
    return () => window.clearTimeout(timer)
  }, [])

  return <RegionPreview hasData phase={phase} source={props.source} />
}

const meta = {
  title: 'Components/AsyncDataRegion',
  component: AsyncDataRegion,
  args: { phase: 'initial-loading' },
  parameters: { layout: 'padded' },
  tags: ['autodocs'],
} satisfies Meta<typeof AsyncDataRegion>

export default meta
type Story = StoryObj<typeof meta>

export const InitialLoading: Story = {
  render: () => <RegionPreview phase="initial-loading" />,
  play: async ({ canvasElement }) => {
    if (!canvasElement.querySelector('[data-async-data-phase="initial-loading"] .skeleton')) {
      throw new Error('initial loading must render a skeleton')
    }
  },
}

export const UserActionRefresh: Story = {
  render: () => <TimedRefreshPreview source="memory" />,
  play: async ({ canvasElement }) => {
    await sleep(250)
    if (!canvasElement.querySelector('[role="status"]')) {
      throw new Error('user refresh should show its delayed overlay after 200ms')
    }
  },
}

export const FreshSnapshotRefresh: Story = {
  render: () => <TimedRefreshPreview source="fresh-snapshot" />,
  play: async ({ canvasElement }) => {
    await sleep(840)
    if (!canvasElement.querySelector('[role="status"]')) {
      throw new Error('snapshot refresh should show its delayed overlay after 800ms')
    }
  },
}

export const ErrorWithLastGoodData: Story = {
  render: () => <RegionPreview error="服务运行态暂时不可用" hasData phase="error" source="memory" />,
  play: async ({ canvasElement }) => {
    if (!canvasElement.querySelector('[role="alert"]') || !canvasElement.textContent?.includes('运行中: 3')) {
      throw new Error('error state must retain the last good data and expose a retry alert')
    }
  },
}

export const InitialError: Story = {
  render: () => <RegionPreview error="无法连接服务，请重试。" phase="error" />,
}
