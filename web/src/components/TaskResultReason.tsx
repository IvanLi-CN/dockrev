import type { ReactNode } from 'react'
import type { JobResultReason } from '../api'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { useHoverPinnedPopover } from './HoverPinnedPopover'

function normalizeReasonText(value: string | null | undefined): string {
  return (value ?? '').trim()
}

type TaskResultReasonProps = {
  reason?: JobResultReason | null
  lines?: 1 | 2
  label?: ReactNode
  className?: string
  detailClassName?: string
}

export function TaskResultReason(props: TaskResultReasonProps) {
  const summary = normalizeReasonText(props.reason?.summary)
  const detail = normalizeReasonText(props.reason?.detail)
  const raw = normalizeReasonText(props.reason?.raw)
  const fullDetail = detail || summary
  const rootClassName = ['taskResultReason', props.className].filter(Boolean).join(' ')
  const popoverClassName = ['taskResultReasonPopover', props.detailClassName].filter(Boolean).join(' ')

  const {
    contentProps,
    popoverProps,
    triggerProps,
  } = useHoverPinnedPopover()

  if (!summary || !fullDetail) return null

  return (
    <div className={rootClassName}>
      {props.label ? <span className="taskResultReasonLabel">{props.label}</span> : null}
      <Popover {...popoverProps}>
        <PopoverTrigger asChild>
          <button
            type="button"
            className={`taskResultReasonTrigger taskResultReasonTrigger-${props.lines ?? 1}`}
            aria-label={`查看任务结果原因：${summary}`}
            aria-expanded={triggerProps['aria-expanded']}
            data-state={triggerProps['data-state']}
            onClick={(event) => {
              event.preventDefault()
              event.stopPropagation()
              triggerProps.onClick()
            }}
            onPointerEnter={(event) => {
              event.stopPropagation()
              triggerProps.onPointerEnter()
            }}
            onPointerLeave={(event) => {
              event.stopPropagation()
              triggerProps.onPointerLeave()
            }}
          >
            <span className={`taskResultReasonSummary taskResultReasonSummary-${props.lines ?? 1}`}>
              {summary}
            </span>
          </button>
        </PopoverTrigger>
        <PopoverContent
          className={popoverClassName}
          align="start"
          sideOffset={10}
          {...contentProps}
        >
          <div className="taskResultReasonPopoverSection">
            <div className="taskResultReasonPopoverTitle">结果原因</div>
            <div className="taskResultReasonPopoverBody">{fullDetail}</div>
          </div>
          {raw && raw !== fullDetail ? (
            <div className="taskResultReasonPopoverSection taskResultReasonPopoverSection-raw">
              <div className="taskResultReasonPopoverSubtitle">原始详情</div>
              <pre className="taskResultReasonPopoverRaw">{raw}</pre>
            </div>
          ) : null}
        </PopoverContent>
      </Popover>
    </div>
  )
}
