import { type ReactNode } from 'react'
import { Icon } from '@iconify/react'
import alertCircleOutline from '@iconify-icons/mdi/alert-circle-outline'
import checkCircleOutline from '@iconify-icons/mdi/check-circle-outline'
import closeCircleOutline from '@iconify-icons/mdi/close-circle-outline'
import emailOutline from '@iconify-icons/mdi/email-outline'
import linkVariant from '@iconify-icons/mdi/link-variant'
import progressClock from '@iconify-icons/mdi/progress-clock'
import sendCircleOutline from '@iconify-icons/mdi/send-circle-outline'
import telegram from '@iconify-icons/mdi/telegram'
import bellOutline from '@iconify-icons/mdi/bell-outline'
import type { NotificationTestChannel } from '../api'
import { Switch } from '../ui'

export type NotificationTestBubbleStepTone = 'running' | 'success' | 'error' | 'info'

export type NotificationTestBubbleStep = {
  tone: NotificationTestBubbleStepTone
  text: string
}

export type NotificationChannelTestState = {
  phase: 'running' | 'success' | 'error'
  steps: NotificationTestBubbleStep[]
  updatedAt: string
  errorDetail?: string
}

const NOTIFICATION_CHANNEL_LABEL: Record<NotificationTestChannel, string> = {
  email: 'Email',
  webhook: 'Webhook',
  telegram: 'Telegram',
  webPush: 'Web Push',
}

const NOTIFICATION_CHANNEL_ICON = {
  email: emailOutline,
  webhook: linkVariant,
  telegram,
  webPush: bellOutline,
} as const

export function NotificationChannelTestControl(props: {
  channel: NotificationTestChannel
  state: NotificationChannelTestState | undefined
  running: boolean
  onRun: (channel: NotificationTestChannel) => void
}) {
  const label = NOTIFICATION_CHANNEL_LABEL[props.channel]

  return (
    <div className="notificationTestActionWrap" data-notification-test-wrap={props.channel}>
      <button
        type="button"
        className={`btn btnGhost notificationTestBtn${props.running ? ' notificationTestBtnRunning' : ''}`}
        onClick={() => props.onRun(props.channel)}
        disabled={props.running}
        aria-label={`测试 ${label} 通道`}
        title={`测试 ${label} 通道`}
        data-notification-test-channel={props.channel}
      >
        <span className="notificationTestBtnInner">
          <Icon icon={NOTIFICATION_CHANNEL_ICON[props.channel]} className="notificationTestBtnIcon" aria-hidden="true" />
          <span>测试</span>
        </span>
      </button>

      {props.state ? (
        <div
          className={`notificationTestBubble notificationTestBubble${props.state.phase === 'error' ? 'Bad' : props.state.phase === 'success' ? 'Ok' : 'Info'}`}
          role="status"
          aria-live="polite"
          data-notification-test-bubble={props.channel}
        >
          <div className="notificationTestBubbleTitle">
            <Icon icon={sendCircleOutline} aria-hidden="true" />
            <span>{label} 测试</span>
          </div>
          <div className="notificationTestBubbleSteps">
            {props.state.steps.map((step, index) => {
              const icon =
                step.tone === 'success'
                  ? checkCircleOutline
                  : step.tone === 'error'
                    ? closeCircleOutline
                    : step.tone === 'running'
                      ? progressClock
                      : alertCircleOutline
              return (
                <div
                  key={`${props.channel}-${index}-${step.text}`}
                  className={`notificationTestBubbleStep notificationTestBubbleStep${step.tone === 'running' ? 'Running' : step.tone === 'success' ? 'Ok' : step.tone === 'error' ? 'Bad' : 'Info'}`}
                >
                  <Icon
                    icon={icon}
                    className={step.tone === 'running' ? 'notificationTestBubbleStepIconSpin' : undefined}
                    aria-hidden="true"
                  />
                  <span>{step.text}</span>
                </div>
              )
            })}
          </div>
          {props.state.errorDetail ? <div className="notificationTestBubbleError">{props.state.errorDetail}</div> : null}
        </div>
      ) : null}
    </div>
  )
}

export type NotificationChannelCardProps = {
  channel: NotificationTestChannel
  title: string
  enabled: boolean
  busy?: boolean
  testState: NotificationChannelTestState | undefined
  testRunning: boolean
  onToggleEnabled: (next: boolean) => void
  onRunTest: (channel: NotificationTestChannel) => void
  children: ReactNode
}

export function NotificationChannelCard(props: NotificationChannelCardProps) {
  return (
    <div className="settingsSection" data-notification-channel-card={props.channel}>
      <div className="settingHead">
        <div className="sectionTitle">{props.title}</div>
        <div className="notificationChannelHeadActions">
          <NotificationChannelTestControl
            channel={props.channel}
            state={props.testState}
            running={props.testRunning}
            onRun={props.onRunTest}
          />
          <Switch checked={props.enabled} disabled={props.busy} onChange={props.onToggleEnabled} />
        </div>
      </div>
      <div className="kv">{props.children}</div>
    </div>
  )
}
