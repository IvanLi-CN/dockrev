import { Icon } from '@iconify/react'
import type { Meta, StoryObj } from '@storybook/react'
import { webhookStateDotClass, webhookStateIcon } from '../../webhookStatus'

const meta: Meta = {
  title: 'Components/WebhookStateDot',
}

export default meta
type Story = StoryObj

const states = ['ok', 'missing', 'queued', 'running', 'error', 'conflict', 'unknown'] as const

export const AllStates: Story = {
  render: () => {
    return (
      <div className="card" style={{ width: 520 }}>
        <div className="title">Webhook 状态点</div>
        <div className="muted" style={{ marginTop: 6 }}>
          使用 SettingsPage 同一映射（Iconify + 状态色）
        </div>
        <div style={{ marginTop: 12, display: 'grid', rowGap: 10 }}>
          {states.map((state) => (
            <div key={state} style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <Icon icon={webhookStateIcon(state)} className={webhookStateDotClass(state)} aria-hidden="true" />
              <span className="mono">{state}</span>
            </div>
          ))}
        </div>
      </div>
    )
  },
}
