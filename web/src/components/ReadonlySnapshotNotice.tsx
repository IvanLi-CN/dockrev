import { WifiOff, RefreshCw, ShieldAlert, DatabaseZap } from 'lucide-react'
import { Button } from '../ui'

export type ReadonlySnapshotNoticeTone = 'info' | 'warn' | 'bad'

export function ReadonlySnapshotNotice(props: {
  tone?: ReadonlySnapshotNoticeTone
  title: string
  detail: string
  fetchedAt?: string | null
  actionLabel?: string
  actionDisabled?: boolean
  onAction?: () => void
}) {
  const tone = props.tone ?? 'info'
  const icon =
    tone === 'bad' ? (
      <ShieldAlert size={18} strokeWidth={2.1} aria-hidden="true" />
    ) : tone === 'warn' ? (
      <WifiOff size={18} strokeWidth={2.1} aria-hidden="true" />
    ) : (
      <DatabaseZap size={18} strokeWidth={2.1} aria-hidden="true" />
    )

  return (
    <div className={`readonlySnapshotNotice readonlySnapshotNotice-${tone}`} role="status">
      <div className="readonlySnapshotNoticeIcon">{icon}</div>
      <div className="readonlySnapshotNoticeBody">
        <div className="readonlySnapshotNoticeTitle">{props.title}</div>
        <div className="readonlySnapshotNoticeDetail">{props.detail}</div>
      </div>
      {props.actionLabel && props.onAction ? (
        <Button
          className="readonlySnapshotNoticeAction"
          disabled={props.actionDisabled}
          onClick={props.onAction}
          variant="ghost"
        >
          <RefreshCw size={15} strokeWidth={2} aria-hidden="true" />
          {props.actionLabel}
        </Button>
      ) : null}
    </div>
  )
}
