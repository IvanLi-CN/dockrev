import type { Meta, StoryObj } from '@storybook/react'
import { AppShellStatusBanner } from '../../components/AppShellStatusBanner'
import { Button } from '../../ui'
import { RefreshCw } from 'lucide-react'

const meta: Meta<typeof AppShellStatusBanner> = {
  title: 'Components/AppShellStatusBanner',
  component: AppShellStatusBanner,
  parameters: {
    layout: 'padded',
  },
}

export default meta
type Story = StoryObj<typeof AppShellStatusBanner>

export const OfflineMode: Story = {
  args: {
    tone: 'offline',
    title: '当前离线，可继续使用已缓存的主要只读页。',
    detail: '写操作、日志流和部分高时效页面需要恢复联网后才能继续。',
  },
}

export const OfflineReady: Story = {
  args: {
    tone: 'ready',
    title: '离线壳已就绪。',
    detail: '之后断网刷新仍可先启动应用与已缓存的主要只读页。',
    actions: <Button variant="ghost">知道了</Button>,
  },
}

export const ManagementReconnect: Story = {
  args: {
    tone: 'warning',
    title: '管理事件流重连中',
    detail: '心跳超时，第 2 次重试。最近活动：09:10:47。',
    actions: (
      <Button type="button" variant="ghost" size="icon" aria-label="立即重试管理事件流" title="立即重试管理事件流">
        <RefreshCw size={16} aria-hidden="true" />
      </Button>
    ),
  },
}
