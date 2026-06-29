import type { Meta, StoryObj } from '@storybook/react'
import { TaskResultReason } from '../../components/TaskResultReason'

const meta: Meta<typeof TaskResultReason> = {
  title: 'Components/TaskResultReason',
  component: TaskResultReason,
  tags: ['autodocs'],
}

export default meta
type Story = StoryObj<typeof TaskResultReason>

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

export const QueueSingleLine: Story = {
  args: {
    lines: 1,
    reason: {
      summary: '镜像拉取失败（Registry / Docker Hub 限流），已回滚',
      detail: '镜像拉取命中 Registry / Docker Hub 限流，Dockrev 已终止更新并自动回滚到升级前版本。',
      raw: 'toomanyrequests: You have reached your pull rate limit on Docker Hub.',
    },
  },
  render: (args) => (
    <div className="card" style={{ width: 560 }}>
      <TaskResultReason {...args} />
    </div>
  ),
  play: async ({ canvasElement }) => {
    const trigger = canvasElement.querySelector<HTMLButtonElement>('.taskResultReasonTrigger')
    if (!trigger) throw new Error('missing result reason trigger')
    trigger.click()
    await sleep(160)
    const doc = canvasElement.ownerDocument
    const popover = doc.querySelector('.taskResultReasonPopover')
    if (!popover) throw new Error('result reason popover should open on click')
    const text = popover.textContent ?? ''
    if (!text.includes('结果原因')) throw new Error('popover title missing')
    if (!text.includes('原始详情')) throw new Error('raw detail title missing')
    if (!text.includes('Docker Hub')) throw new Error('raw detail content missing')
  },
}

export const DetailTwoLines: Story = {
  args: {
    lines: 2,
    label: '结果原因',
    reason: {
      summary: '健康检查失败，已回滚',
      detail: '健康检查未通过，已停止本次变更并恢复到回滚前状态。',
    },
  },
  render: (args) => (
    <div className="card" style={{ width: 640 }}>
      <TaskResultReason {...args} />
    </div>
  ),
}
