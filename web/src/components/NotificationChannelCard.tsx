import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react'
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

const NOTIFICATION_TEST_BUBBLE_MIN_VISIBLE_MS = 3000

export function NotificationChannelTestControl(props: {
  channel: NotificationTestChannel
  state: NotificationChannelTestState | undefined
  running: boolean
  onRun: (channel: NotificationTestChannel) => void
}) {
  const label = NOTIFICATION_CHANNEL_LABEL[props.channel]
  const [dismissedKey, setDismissedKey] = useState<string | null>(null)
  const bubbleRef = useRef<HTMLDivElement | null>(null)
  const dismissTimerRef = useRef<number | null>(null)
  const outsideClickRequestedRef = useRef(false)
  const shownAtMsRef = useRef<number | null>(null)
  const stateKeyRef = useRef<string | null>(null)

  const clearDismissTimer = useCallback(() => {
    const handle = dismissTimerRef.current
    if (handle == null) return
    window.clearTimeout(handle)
    dismissTimerRef.current = null
  }, [])

  const stateKey = useMemo(() => {
    if (!props.state) return null
    // Use a delimiter not present in ISO timestamps.
    return `${props.channel}|${props.state.phase}|${props.state.updatedAt}`
  }, [props.channel, props.state])

  useEffect(() => {
    stateKeyRef.current = stateKey
  }, [stateKey])

  const updatedAt = props.state?.updatedAt ?? null

  useEffect(() => {
    clearDismissTimer()
    outsideClickRequestedRef.current = false
    shownAtMsRef.current = null

    if (!updatedAt) return

    const parsed = Date.parse(updatedAt)
    shownAtMsRef.current = Number.isFinite(parsed) ? parsed : Date.now()
  }, [clearDismissTimer, updatedAt])

  const bubbleVisible = Boolean(props.state && stateKey && dismissedKey !== stateKey)

  const canOutsideDismiss =
    props.state != null &&
    bubbleVisible &&
    (props.state.phase === 'success' || props.state.phase === 'error')

  useEffect(() => {
    if (!canOutsideDismiss) {
      clearDismissTimer()
      outsideClickRequestedRef.current = false
      return
    }

    const onPointerDown = (e: PointerEvent) => {
      const el = e.target instanceof Element ? e.target : null
      if (!el) return

      // Only dismiss when the click is outside of the bubble itself.
      if (bubbleRef.current?.contains(el)) return

      const shownAtMs = shownAtMsRef.current ?? Date.now()
      const earliestCloseAt = shownAtMs + NOTIFICATION_TEST_BUBBLE_MIN_VISIBLE_MS
      const now = Date.now()

      if (now >= earliestCloseAt) {
        clearDismissTimer()
        outsideClickRequestedRef.current = false
        const keyNow = stateKeyRef.current
        if (keyNow != null) setDismissedKey(keyNow)
        return
      }

      outsideClickRequestedRef.current = true
      if (dismissTimerRef.current != null) return

      const waitMs = Math.max(0, earliestCloseAt - now)
      const keyAtSchedule = stateKeyRef.current

      dismissTimerRef.current = window.setTimeout(() => {
        dismissTimerRef.current = null

        // State changed (new test) while waiting: do not dismiss the new bubble.
        if (stateKeyRef.current !== keyAtSchedule) return
        if (!outsideClickRequestedRef.current) return

        outsideClickRequestedRef.current = false
        if (keyAtSchedule != null) setDismissedKey(keyAtSchedule)
      }, waitMs)
    }

    document.addEventListener('pointerdown', onPointerDown)
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      clearDismissTimer()
      outsideClickRequestedRef.current = false
    }
  }, [canOutsideDismiss, clearDismissTimer])

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

      {props.state && bubbleVisible ? (
        <div
          ref={bubbleRef}
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
