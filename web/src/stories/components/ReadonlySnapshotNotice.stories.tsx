import type { Meta, StoryObj } from '@storybook/react'
import { ReadonlySnapshotNotice } from '../../components/ReadonlySnapshotNotice'

const meta: Meta<typeof ReadonlySnapshotNotice> = {
  title: 'Components/ReadonlySnapshotNotice',
  component: ReadonlySnapshotNotice,
  parameters: {
    layout: 'padded',
  },
}

export default meta
type Story = StoryObj<typeof ReadonlySnapshotNotice>

export const OfflineSnapshot: Story = {
  args: {
    tone: 'warn',
    title: '当前离线，显示已缓存的版本推测数据。',
    detail: 'SSE 连接、实时推测进度和 GC 最新结果仍以联网后的服务端状态为准。',
    fetchedAt: '2026-07-08T09:12:00.000Z',
    actionLabel: '重试刷新',
  },
}

export const NoOfflineData: Story = {
  args: {
    tone: 'bad',
    title: '当前没有可用的离线任务队列数据。',
    detail: '请恢复联网后重新加载该页面。',
  },
}
