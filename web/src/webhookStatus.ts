import type { IconifyIcon } from '@iconify/types'
import alertCircleOutline from '@iconify-icons/mdi/alert-circle-outline'
import checkCircle from '@iconify-icons/mdi/check-circle'
import closeCircleOutline from '@iconify-icons/mdi/close-circle-outline'
import helpCircleOutline from '@iconify-icons/mdi/help-circle-outline'
import progressClock from '@iconify-icons/mdi/progress-clock'

export function webhookStateDotClass(state: string): string {
  if (state === 'ok') return 'statusCircleIcon statusCircleIconOk'
  if (state === 'missing' || state === 'queued' || state === 'running')
    return 'statusCircleIcon statusCircleIconWarn'
  if (state === 'error' || state === 'conflict') return 'statusCircleIcon statusCircleIconBad'
  return 'statusCircleIcon statusCircleIconWarn'
}

export function webhookStateIcon(state: string): IconifyIcon {
  if (state === 'ok') return checkCircle
  if (state === 'missing') return alertCircleOutline
  if (state === 'queued' || state === 'running') return progressClock
  if (state === 'error' || state === 'conflict') return closeCircleOutline
  return helpCircleOutline
}
