import { RefreshCw } from 'lucide-react'

import { Alert, AlertDescription, AlertTitle } from './ui/alert'
import './ReleaseNotesStaleAlert.css'

type ReleaseNotesStaleAlertProps = {
  className?: string
  dataAttribute: 'data-release-drawer-banner' | 'data-service-versions-banner'
  message: string
}

export function ReleaseNotesStaleAlert(props: ReleaseNotesStaleAlertProps) {
  return (
    <Alert
      aria-live="polite"
      className={props.className}
      role="status"
      variant="warning"
      {...{ [props.dataAttribute]: 'stale' }}
    >
      <RefreshCw aria-hidden="true" className="releaseNotesStaleAlertIcon" />
      <div className="releaseNotesStaleAlertCopy">
        <AlertTitle className="releaseNotesStaleAlertTitle">发布记录暂未更新</AlertTitle>
        <AlertDescription className="releaseNotesStaleAlertDescription">{props.message}</AlertDescription>
      </div>
    </Alert>
  )
}
