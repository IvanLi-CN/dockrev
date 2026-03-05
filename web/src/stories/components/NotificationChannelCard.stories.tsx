import { useState, type ReactNode } from 'react'
import type { Meta, StoryObj } from '@storybook/react'
import {
  NotificationChannelCard,
  type NotificationChannelCardProps,
  type NotificationChannelTestState,
  type NotificationTestBubbleStep,
} from '../../components/NotificationChannelCard'

function state(phase: NotificationChannelTestState['phase'], steps: NotificationTestBubbleStep[], errorDetail?: string) {
  return {
    phase,
    steps,
    updatedAt: new Date('2026-03-05T00:00:00+08:00').toISOString(),
    errorDetail,
  } satisfies NotificationChannelTestState
}

function notificationFields(channel: 'email' | 'webhook' | 'telegram' | 'webPush'): ReactNode {
  if (channel === 'email') {
    return (
      <div className="kvRow">
        <div className="label">SMTP URL</div>
        <input className="input" defaultValue="smtp://user:pass@smtp.example.com:587" />
      </div>
    )
  }

  if (channel === 'webhook') {
    return (
      <div className="kvRow">
        <div className="label">URL</div>
        <input className="input" defaultValue="https://hooks.example.com/dockrev" />
      </div>
    )
  }

  if (channel === 'telegram') {
    return (
      <>
        <div className="kvRow">
          <div className="label">Bot token</div>
          <input className="input" defaultValue="123456:AAAbbbCCCDDD" />
        </div>
        <div className="kvRow">
          <div className="label">Chat id</div>
          <input className="input" defaultValue="-1001234567890" />
        </div>
      </>
    )
  }

  return (
    <>
      <div className="kvRow">
        <div className="label">Public Key</div>
        <input className="input" defaultValue="BAnExamplePublicKey..." />
      </div>
      <div className="kvRow">
        <div className="label">Subject</div>
        <input className="input" defaultValue="mailto:ops@example.com" />
      </div>
    </>
  )
}

function NotificationChannelCardPreview(args: Partial<NotificationChannelCardProps>) {
  const channel = args.channel ?? 'email'
  const title = args.title ?? 'Email'
  const busy = args.busy ?? false
  const testRunning = args.testRunning ?? false
  const [enabled, setEnabled] = useState(args.enabled ?? true)

  return (
    <div className="card" style={{ maxWidth: 760 }}>
      <div className="title">通知</div>
      <div className="muted">独立测试按钮 + 气泡步骤 + 最近一次结果常驻</div>
      <NotificationChannelCard
        channel={channel}
        title={title}
        enabled={enabled}
        busy={busy}
        testRunning={testRunning}
        testState={args.testState}
        onRunTest={(testChannel) => args.onRunTest?.(testChannel)}
        onToggleEnabled={(next) => {
          setEnabled(next)
          args.onToggleEnabled?.(next)
        }}
      >
        {notificationFields(channel)}
      </NotificationChannelCard>
    </div>
  )
}

const meta: Meta<typeof NotificationChannelCard> = {
  title: 'Components/NotificationChannelCard',
  component: NotificationChannelCard,
  argTypes: {
    children: { control: false },
    onRunTest: { action: 'run-test' },
    onToggleEnabled: { action: 'toggle-enabled' },
  },
  render: (args) => <NotificationChannelCardPreview {...args} />,
}

export default meta
type Story = StoryObj<typeof NotificationChannelCard>

export const Idle: Story = {
  args: {
    channel: 'email',
    title: 'Email',
    enabled: true,
    busy: false,
    testRunning: false,
    testState: undefined,
  },
}

export const Running: Story = {
  args: {
    channel: 'webhook',
    title: 'Webhook',
    enabled: true,
    busy: false,
    testRunning: true,
    testState: state('running', [
      { tone: 'running', text: '正在发送 Webhook 测试消息' },
      { tone: 'info', text: '等待渠道响应' },
    ]),
  },
}

export const Success: Story = {
  args: {
    channel: 'email',
    title: 'Email',
    enabled: true,
    busy: false,
    testRunning: false,
    testState: state('success', [
      { tone: 'success', text: 'Email 测试请求已发送' },
      { tone: 'success', text: 'Email 渠道返回成功' },
    ]),
  },
}

export const ErrorWithDetail: Story = {
  args: {
    channel: 'webPush',
    title: 'Web Push（Chrome / VAPID）',
    enabled: false,
    busy: false,
    testRunning: false,
    testState: state(
      'error',
      [
        { tone: 'success', text: 'Web Push 测试请求已发出' },
        { tone: 'error', text: 'Web Push 渠道测试失败' },
        { tone: 'error', text: '查看详细错误信息' },
      ],
      'webPush.vapidPublicKey missing',
    ),
  },
}

export const DisabledButStillTestable: Story = {
  args: {
    channel: 'telegram',
    title: 'Telegram',
    enabled: false,
    busy: false,
    testRunning: false,
    testState: state(
      'error',
      [
        { tone: 'success', text: 'Telegram 测试请求已发出' },
        { tone: 'error', text: 'Telegram 渠道测试失败' },
        { tone: 'error', text: '查看详细错误信息' },
      ],
      'telegram.botToken missing',
    ),
  },
}
