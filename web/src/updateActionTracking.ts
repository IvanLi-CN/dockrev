import { useCallback, useEffect, useRef, useState } from 'react'
import { getJob } from './api'

const UPDATE_JOB_POLL_INTERVAL_MS = 1200
const UPDATE_JOB_MAX_ERRORS = 3

export type UpdateActionTargetKey = 'all' | `stack:${string}` | `service:${string}`

export function resolveUpdateActionTargetKey(
  scope: string,
  stackId?: string | null,
  serviceId?: string | null,
): UpdateActionTargetKey | null {
  if (scope === 'all') return 'all'
  if (scope === 'stack') {
    const id = (stackId ?? '').trim()
    return id ? `stack:${id}` : null
  }
  if (scope === 'service') {
    const id = (serviceId ?? '').trim()
    return id ? `service:${id}` : null
  }
  return null
}

export function isUpdateJobActiveStatus(status: string): boolean {
  return status === 'queued' || status === 'running'
}

export function useUpdateActionTracker() {
  const [submittingCounts, setSubmittingCounts] = useState<Record<string, number>>({})
  const [runningByTarget, setRunningByTarget] = useState<Record<string, string>>({})
  const runningByTargetRef = useRef(new Map<UpdateActionTargetKey, string>())
  const pollJobRef = useRef<(target: UpdateActionTargetKey, jobId: string) => Promise<void> | void>(() => {})
  const timersRef = useRef(new Map<string, number>())
  const errorCountsRef = useRef(new Map<string, number>())
  const unmountedRef = useRef(false)

  const publishRunning = useCallback(() => {
    if (unmountedRef.current) return
    const next: Record<string, string> = {}
    for (const [target, jobId] of runningByTargetRef.current.entries()) next[target] = jobId
    setRunningByTarget(next)
  }, [])

  const clearJobTimer = useCallback((jobId: string) => {
    const existing = timersRef.current.get(jobId)
    if (existing == null) return
    window.clearTimeout(existing)
    timersRef.current.delete(jobId)
  }, [])

  const clearRunningJob = useCallback(
    (target: UpdateActionTargetKey, jobId: string) => {
      const current = runningByTargetRef.current.get(target)
      if (current !== jobId) return
      runningByTargetRef.current.delete(target)
      errorCountsRef.current.delete(jobId)
      clearJobTimer(jobId)
      publishRunning()
    },
    [clearJobTimer, publishRunning],
  )

  const pollJob = useCallback(
    async (target: UpdateActionTargetKey, jobId: string) => {
      if (unmountedRef.current) return
      if (runningByTargetRef.current.get(target) !== jobId) {
        clearJobTimer(jobId)
        return
      }

      try {
        const job = await getJob(jobId)
        errorCountsRef.current.delete(jobId)
        if (!isUpdateJobActiveStatus(job.status)) {
          clearRunningJob(target, jobId)
          return
        }
      } catch {
        const errors = (errorCountsRef.current.get(jobId) ?? 0) + 1
        errorCountsRef.current.set(jobId, errors)
        if (errors >= UPDATE_JOB_MAX_ERRORS) {
          clearRunningJob(target, jobId)
          return
        }
      }

      const timer = window.setTimeout(() => {
        void pollJobRef.current(target, jobId)
      }, UPDATE_JOB_POLL_INTERVAL_MS)
      timersRef.current.set(jobId, timer)
    },
    [clearJobTimer, clearRunningJob],
  )

  useEffect(() => {
    pollJobRef.current = pollJob
  }, [pollJob])

  useEffect(() => {
    const timers = timersRef.current
    return () => {
      unmountedRef.current = true
      for (const timer of timers.values()) window.clearTimeout(timer)
      timers.clear()
    }
  }, [])

  const beginSubmitting = useCallback((target: UpdateActionTargetKey) => {
    setSubmittingCounts((prev) => ({
      ...prev,
      [target]: (prev[target] ?? 0) + 1,
    }))
  }, [])

  const endSubmitting = useCallback((target: UpdateActionTargetKey) => {
    setSubmittingCounts((prev) => {
      const nextCount = (prev[target] ?? 0) - 1
      if (nextCount > 0) return { ...prev, [target]: nextCount }
      if (!(target in prev)) return prev
      const next = { ...prev }
      delete next[target]
      return next
    })
  }, [])

  const trackJob = useCallback(
    (target: UpdateActionTargetKey, jobId: string) => {
      const previousJobId = runningByTargetRef.current.get(target)
      if (previousJobId && previousJobId !== jobId) {
        clearJobTimer(previousJobId)
        errorCountsRef.current.delete(previousJobId)
      }
      runningByTargetRef.current.set(target, jobId)
      errorCountsRef.current.delete(jobId)
      clearJobTimer(jobId)
      publishRunning()
      const timer = window.setTimeout(() => {
        void pollJobRef.current(target, jobId)
      }, 0)
      timersRef.current.set(jobId, timer)
    },
    [clearJobTimer, publishRunning],
  )

  const isTargetBusy = useCallback(
    (target: UpdateActionTargetKey): boolean => {
      return Boolean((submittingCounts[target] ?? 0) > 0 || runningByTarget[target] != null)
    },
    [runningByTarget, submittingCounts],
  )

  return {
    beginSubmitting,
    endSubmitting,
    trackJob,
    isTargetBusy,
  }
}
