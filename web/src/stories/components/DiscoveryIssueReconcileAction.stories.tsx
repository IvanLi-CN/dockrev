import type { Meta, StoryObj } from '@storybook/react'
import { DiscoveryIssueReconcileAction } from '../../pages/overviewHelpers'

const meta = {
  title: 'Components/DiscoveryIssueReconcileAction',
  component: DiscoveryIssueReconcileAction,
  tags: ['autodocs'],
  parameters: {
    layout: 'centered',
    docs: {
      description: {
        component: '仅对历史 Dockrev 临时 override 告警显示修复入口；确认弹窗和重建任务由页面状态负责。',
      },
    },
  },
} satisfies Meta<typeof DiscoveryIssueReconcileAction>

export default meta
type Story = StoryObj<typeof meta>

export const EligibleWarning: Story = {
  args: {
    eligible: true,
    stackId: 'stk_01KSTORY',
    onReconcile: () => undefined,
  },
  render: (args) => (
    <div
      data-reconcile-action-state="eligible"
      style={{
        padding: 16,
      }}
    >
      <div
        style={{
          padding: 24,
          background: 'var(--panel)',
          border: '1px solid var(--borderColor)',
          borderRadius: 8,
        }}
      >
        <DiscoveryIssueReconcileAction {...args} />
      </div>
    </div>
  ),
  play: async ({ canvasElement }) => {
    const button = canvasElement.querySelector<HTMLButtonElement>('button')
    if (!button || button.textContent?.trim() !== '修复标签') {
      throw new Error('eligible stale warning should expose the repair action')
    }
  },
}

export const NotEligible: Story = {
  args: {
    eligible: false,
    stackId: 'stk_01KSTORY',
    onReconcile: () => undefined,
  },
  render: (args) => (
    <div data-reconcile-action-state="not-eligible" style={{ minHeight: 24, minWidth: 120 }}>
      <DiscoveryIssueReconcileAction {...args} />
    </div>
  ),
}
