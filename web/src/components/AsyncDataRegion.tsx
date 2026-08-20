import { useEffect, useState, type ReactNode } from 'react'
import { CircleAlert, LoaderCircle, RotateCw } from 'lucide-react'
import {
  asyncOverlayDelay,
  formatAsyncDataError,
  isAsyncDataBusy,
  type AsyncDataPhase,
  type AsyncDataSource,
  type AsyncDataTrigger,
} from '../asyncData'
import { Button } from '../ui'

export type { AsyncDataPhase, AsyncDataSource, AsyncDataTrigger } from '../asyncData'

export function Skeleton(props: {
  className?: string
  shape?: 'line' | 'pill' | 'block'
}) {
  const shape = props.shape ?? 'line'
  return <span aria-hidden="true" className={`skeleton skeleton${shape[0]!.toUpperCase()}${shape.slice(1)} ${props.className ?? ''}`} />
}

export function AsyncDataSkeleton(props: { className?: string; lines?: number }) {
  const lines = Math.max(1, props.lines ?? 3)
  return (
    <div aria-hidden="true" className={`asyncDataSkeleton ${props.className ?? ''}`}>
      {Array.from({ length: lines }, (_, index) => (
        <Skeleton className={index === lines - 1 ? 'asyncDataSkeletonShort' : undefined} key={index} />
      ))}
    </div>
  )
}

export function AsyncDataRegion(props: {
  phase: AsyncDataPhase
  source?: AsyncDataSource
  trigger?: AsyncDataTrigger
  hasData?: boolean
  className?: string
  label?: string
  error?: string | null
  onRetry?: () => void
  retryDisabled?: boolean
  skeleton?: ReactNode
  children?: ReactNode
}) {
  const {
    phase,
    source = 'none',
    trigger = 'background',
    hasData = false,
    className,
    label = '正在加载数据',
    error,
    onRetry,
    retryDisabled = false,
    skeleton = <AsyncDataSkeleton />,
    children,
  } = props
  const [showDelayedOverlay, setShowDelayedOverlay] = useState(false)
  const [loadingOverlayMounted, setLoadingOverlayMounted] = useState(false)
  const [loadingOverlayLeaving, setLoadingOverlayLeaving] = useState(false)
  const busy = isAsyncDataBusy(phase)
  const initialSkeleton = phase === 'initial-loading' || (phase === 'error' && !hasData)

  useEffect(() => {
    if (phase !== 'refreshing') {
      setShowDelayedOverlay(false)
      return
    }
    const timer = window.setTimeout(() => setShowDelayedOverlay(true), asyncOverlayDelay(trigger))
    return () => window.clearTimeout(timer)
  }, [phase, trigger])

  const showLoadingOverlay = phase === 'refreshing' && showDelayedOverlay
  const showErrorOverlay = phase === 'error'

  useEffect(() => {
    if (showErrorOverlay) {
      setLoadingOverlayMounted(false)
      setLoadingOverlayLeaving(false)
      return
    }
    if (showLoadingOverlay) {
      setLoadingOverlayMounted(true)
      setLoadingOverlayLeaving(false)
      return
    }
    if (!loadingOverlayMounted) return
    setLoadingOverlayLeaving(true)
    const timer = window.setTimeout(() => {
      setLoadingOverlayMounted(false)
      setLoadingOverlayLeaving(false)
    }, 160)
    return () => window.clearTimeout(timer)
  }, [loadingOverlayMounted, showErrorOverlay, showLoadingOverlay])

  return (
    <section
      aria-busy={busy || undefined}
      className={`asyncDataRegion ${className ?? ''}`}
      data-async-data-phase={phase}
      data-async-data-source={source}
      data-async-data-trigger={trigger}
    >
      {initialSkeleton ? skeleton : children}
      {loadingOverlayMounted && !showErrorOverlay ? (
        <div
          className={`asyncDataOverlay asyncDataLoadingOverlay ${loadingOverlayLeaving ? 'asyncDataOverlayLeaving' : ''}`}
          role="status"
          aria-live="polite"
        >
          <LoaderCircle aria-hidden="true" className="asyncDataSpinner" size={18} strokeWidth={2} />
          <span>{label}</span>
        </div>
      ) : null}
      {showErrorOverlay ? (
        <div className="asyncDataOverlay asyncDataErrorOverlay" role="alert">
          <CircleAlert aria-hidden="true" size={18} strokeWidth={2} />
          <div className="asyncDataErrorCopy">
            <span>{formatAsyncDataError(error)}</span>
          </div>
          {onRetry ? (
            <Button
              ariaLabel="重试加载"
              className="asyncDataRetry"
              disabled={retryDisabled}
              onClick={onRetry}
              variant="ghost"
            >
              <RotateCw aria-hidden="true" size={14} strokeWidth={2} />
              重试
            </Button>
          ) : null}
        </div>
      ) : null}
    </section>
  )
}
