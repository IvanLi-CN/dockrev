import type { Meta, StoryObj } from '@storybook/react'
import { AppShellStatusBanner } from '../../components/AppShellStatusBanner'
import { Button } from '../../ui'

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
