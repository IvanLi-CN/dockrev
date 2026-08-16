import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Square } from 'lucide-react'
import { getJob, newJobEventsSource, stopJob, type JobDetail, type JobLogLine, type JobProgress } from '../api'
import { useManagementEventBatch } from '../managementEvents'
import { formatJobMachineName, formatJobReadableDisplay } from '../jobDisplay'
import { formatJobProgressDownload, parseJobProgressDownload } from '../jobProgressDownload'
import { TaskResultReason } from '../components/TaskResultReason'
import { navigate } from '../routes'
import { Button, Chip, IconButton, Mono, OverlayScrollArea, Pill, Switch } from '../ui'

function statusTone(status: string): 'ok' | 'warn' | 'bad' | 'muted' | 'info' {
  if (status === 'success') return 'ok'
  if (status === 'rolled_back') return 'warn'
  if (status === 'cancelled') return 'muted'
  if (status === 'failed') return 'bad'
  if (status === 'running') return 'info'
  return 'muted'
}

function formatShort(ts?: string | null) {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

type LogTimeZone = 'local' | 'utc'
const LOG_FOLLOW_BOTTOM_THRESHOLD_PX = 48
const SHOW_EVENTS_STORAGE_KEY = 'dockrev.job-detail.show-events'

const LOCAL_TZ = (() => {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || 'local'
  } catch {
    return 'local'
  }
})()

function pad2(n: number): string {
  return String(n).padStart(2, '0')
}

function pad3(n: number): string {
  return String(n).padStart(3, '0')
}

function formatLogTs(ts: string, tz: LogTimeZone): string {
  const s = (ts ?? '').trim()
  if (!s) return '-'

  const d = new Date(s)
  if (!Number.isNaN(d.valueOf())) {
    const h = tz === 'utc' ? d.getUTCHours() : d.getHours()
    const min = tz === 'utc' ? d.getUTCMinutes() : d.getMinutes()
    const sec = tz === 'utc' ? d.getUTCSeconds() : d.getSeconds()
    const ms = tz === 'utc' ? d.getUTCMilliseconds() : d.getMilliseconds()
    // Monospace-friendly, stable width (works well with the 14ch grid column).
    return `${pad2(h)}:${pad2(min)}:${pad2(sec)}.${pad3(ms)}`
  }

  // Common "YYYY-MM-DD HH:mm:ss" -> show time part.
  const m = s.match(/^\d{4}-\d{2}-\d{2}[ T](.+)$/)
  if (m) return m[1]

  return s
}

function formatLogTitle(ts: string): string {
  const s = (ts ?? '').trim()
  if (!s) return '-'
  const d = new Date(s)
  if (Number.isNaN(d.valueOf())) return s
  return `${LOCAL_TZ}: ${d.toLocaleString()} · UTC: ${d.toISOString()}`
}

function formatLogLevel(level: string): string {
  const s = (level ?? '').trim().toLowerCase()
  if (!s) return '-'
  if (s === 'info') return 'INFO'
  if (s === 'warn' || s === 'warning') return 'WARN'
  if (s === 'error' || s === 'err') return 'ERR'
  if (s === 'debug') return 'DBG'
  if (s === 'trace') return 'TRC'
  return s.slice(0, 4).toUpperCase()
}

function isLogViewportNearBottom(element: HTMLElement): boolean {
  return element.scrollHeight - element.scrollTop - element.clientHeight < LOG_FOLLOW_BOTTOM_THRESHOLD_PX
}

function scrollLogViewportToBottom(element: HTMLElement): void {
  element.scrollTop = element.scrollHeight
}

function readShowEventsPreference(): boolean {
  try {
    return window.localStorage.getItem(SHOW_EVENTS_STORAGE_KEY) === 'true'
  } catch {
    return false
  }
}

function writeShowEventsPreference(value: boolean): void {
  try {
    window.localStorage.setItem(SHOW_EVENTS_STORAGE_KEY, String(value))
  } catch {
    // Private browsing and disabled storage should leave the default off state intact.
  }
}

type TerminalSegment = {
  text: string
  fg?: string
  bg?: string
  bold?: boolean
  dim?: boolean
  underline?: boolean
}

type DisplayLogLine = JobLogLine & {
  durableId?: string
  transient?: boolean
  terminalCommandSeq?: number
  terminalSegments?: TerminalSegment[]
  terminalFrozen?: boolean
}

function parseTerminalSegments(value: unknown): TerminalSegment[] | null {
  if (!Array.isArray(value)) return null
  const segments = value.flatMap((item) => {
    if (!item || typeof item !== 'object') return []
    const record = item as Record<string, unknown>
    if (typeof record.text !== 'string' || !record.text) return []
    return [
      {
        text: record.text,
        ...(typeof record.fg === 'string' ? { fg: record.fg } : {}),
        ...(typeof record.bg === 'string' ? { bg: record.bg } : {}),
        ...(record.bold === true ? { bold: true } : {}),
        ...(record.dim === true ? { dim: true } : {}),
        ...(record.underline === true ? { underline: true } : {}),
      },
    ]
  })
  return segments.length > 0 ? segments : []
}

function parseTerminalLines(value: unknown): TerminalSegment[][] {
  if (!Array.isArray(value)) return []
  return value.map((line) => {
    if (!line || typeof line !== 'object') return []
    return parseTerminalSegments((line as Record<string, unknown>).segments) ?? []
  })
}

function safeTerminalColor(value: string | undefined): string | undefined {
  if (!value) return undefined
  if (/^rgb\((?:\s*\d{1,3}\s*,){2}\s*\d{1,3}\s*\)$/.test(value)) return value
  return undefined
}

function normalizeProgress(input: JobProgress | null | undefined): JobProgress | null {
  if (!input) return null
  const total = Number.isFinite(input.total) ? Math.max(0, input.total) : 0
  const current = Number.isFinite(input.current) ? Math.min(Math.max(0, input.current), total || input.current) : 0
  // Frontend only sanitizes backend values; percent is never derived from current/total.
  const percentRaw = Number.isFinite(input.percent) ? input.percent : 0
  const percent = Math.max(0, Math.min(100, Math.round(percentRaw)))
  const hasPlannedPercent = Object.prototype.hasOwnProperty.call(input, 'plannedPercent')
  const plannedTotalRaw = Number.isFinite(input.plannedTotal) ? Math.max(0, input.plannedTotal ?? 0) : total
  const plannedCurrentRaw = Number.isFinite(input.plannedCurrent) ? Math.max(0, input.plannedCurrent ?? 0) : current
  const plannedCurrent = Math.min(plannedCurrentRaw, plannedTotalRaw || plannedCurrentRaw)
  const plannedPercent =
    input.plannedPercent === null
      ? null
      : Number.isFinite(input.plannedPercent)
        ? Math.max(0, Math.min(100, Math.round(input.plannedPercent ?? 0)))
        : hasPlannedPercent
          ? null
          : percent
  return { ...input, current, total, percent, plannedCurrent, plannedTotal: plannedTotalRaw, plannedPercent }
}

function getKnownProgressPercent(progress: JobProgress): number | null {
  const total = Number.isFinite(progress.total) ? Math.max(0, progress.total) : 0
  if (total <= 0) return null
  if (!Number.isFinite(progress.percent)) return null
  return Math.max(0, Math.min(100, Math.round(progress.percent)))
}

function getKnownPlannedProgressPercent(progress: JobProgress): number | null {
  const total = Number.isFinite(progress.plannedTotal) ? Math.max(0, progress.plannedTotal ?? 0) : 0
  if (total <= 0) return null
  if (!Number.isFinite(progress.plannedPercent)) return null
  return Math.max(0, Math.min(100, Math.round(progress.plannedPercent ?? 0)))
}

export function JobDetailPage(props: { jobId: string; onTopActions: (node: React.ReactNode) => void }) {
  const { jobId, onTopActions } = props
  const [job, setJob] = useState<JobDetail | null>(null)
  const [logs, setLogs] = useState<DisplayLogLine[]>([])
  const [progress, setProgress] = useState<JobProgress | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [logTz, setLogTz] = useState<LogTimeZone>('local')
  const [logViewport, setLogViewport] = useState<HTMLElement | null>(null)
  const [logFollow, setLogFollow] = useState(true)
  const [logIsAtBottom, setLogIsAtBottom] = useState(true)
  const [showEvents, setShowEvents] = useState(readShowEventsPreference)
  const [manualRefreshVersion, setManualRefreshVersion] = useState(0)
  const liveCommandOutputSeqsRef = useRef(new Set<number>())
  const pendingCommandSummarySeqsRef = useRef(new Set<number>())
  const visibleLogs = useMemo(
    () => logs.filter((log) => showEvents || log.level.trim().toLowerCase() !== 'event'),
    [logs, showEvents],
  )

  const refresh = useCallback(async () => {
    setError(null)
    const j = await getJob(jobId)
    setJob(j)
    setLogs(j.logs)
    setProgress(normalizeProgress(j.progress))
    liveCommandOutputSeqsRef.current.clear()
    pendingCommandSummarySeqsRef.current.clear()
    return j
  }, [jobId])

  const requestStop = useCallback(async () => {
    setBusy(true)
    setError(null)
    try {
      await stopJob(jobId)
      await refresh()
    } catch (error: unknown) {
      setError(errorMessage(error))
      await refresh().catch(() => undefined)
    } finally {
      setBusy(false)
    }
  }, [jobId, refresh])

  useEffect(() => {
    writeShowEventsPreference(showEvents)
  }, [showEvents])

  useEffect(() => {
    let closed = false
    let es: EventSource | null = null
    let refreshTimer: number | null = null
    let errorStreak = 0
    let hasOpenedOnce = false
    let restarting = false

    const scheduleRefresh = (delayMs: number) => {
      if (refreshTimer != null) return
      refreshTimer = window.setTimeout(() => {
        refreshTimer = null
        void (async () => {
          try {
            const j = await refresh()
            if (closed) return
            if (j.status !== 'running' && j.status !== 'queued') {
              es?.close()
            }
          } catch {
            // ignore refresh failures; user can still use the manual refresh button
          }
        })()
      }, delayMs)
    }

    const start = async () => {
      try {
        const j = await refresh()
        if (closed) return

        // Nothing to stream for terminal jobs.
        if (j.status !== 'running' && j.status !== 'queued') return

        try {
          es = newJobEventsSource(jobId, { afterId: j.logsLastId })
        } catch {
          setError('无法建立实时日志连接')
          return
        }

        es.addEventListener('open', () => {
          errorStreak = 0
          if (!hasOpenedOnce) {
            hasOpenedOnce = true
            return
          }
          // Reconcile durable history before reconnecting so the old source cannot
          // replay rows concurrently with a snapshot that already contains them.
          if (restarting) return
          restarting = true
          es?.close()
          es = null
          hasOpenedOnce = false
          void start().finally(() => {
            restarting = false
          })
        })

        es.addEventListener('job_log', (evt: Event) => {
          const data = (evt as MessageEvent).data
          if (typeof data !== 'string' || !data) return
          try {
            const parsed = JSON.parse(data) as unknown
            if (!parsed || typeof parsed !== 'object') return
            const p = parsed as Record<string, unknown>
            if (p.type !== 'job_log') return
            const ts = typeof p.ts === 'string' ? p.ts : ''
            const level = typeof p.level === 'string' ? p.level : ''
            const msg = typeof p.msg === 'string' ? p.msg : ''
            const commandSeq = typeof p.commandSeq === 'number' && Number.isSafeInteger(p.commandSeq) ? p.commandSeq : null
            const durableId = typeof p.id === 'number' || typeof p.id === 'string' ? String(p.id) : ''
            if (commandSeq !== null && pendingCommandSummarySeqsRef.current.delete(commandSeq)) return
            setLogs((prev) => {
              if (
                (durableId && prev.some((log) => log.durableId === durableId)) ||
                (level.trim().toLowerCase() === 'event' &&
                  prev.some(
                    (log) =>
                      !log.transient &&
                      log.level.trim().toLowerCase() === 'event' &&
                      log.ts === ts &&
                      log.msg === msg,
                  ))
              ) {
                return prev
              }
              const next = [...prev, { ts, level, msg, durableId: durableId || undefined }]
              return next.length > 500 ? next.slice(-500) : next
            })
          } catch {
            // ignore invalid events
          }
        })

        es.addEventListener('job_live_log', (evt: Event) => {
          const data = (evt as MessageEvent).data
          if (typeof data !== 'string' || !data) return
          try {
            const parsed = JSON.parse(data) as unknown
            if (!parsed || typeof parsed !== 'object') return
            const p = parsed as Record<string, unknown>
            if (p.type !== 'job_live_log') return
            const ts = typeof p.ts === 'string' ? p.ts : new Date().toISOString()
            const msg = typeof p.msg === 'string' ? p.msg : ''
            setLogs((prev) => {
              const next = [...prev, { ts, level: '', msg, transient: true }]
              return next.length > 500 ? next.slice(-500) : next
            })
          } catch {
            // ignore invalid events
          }
        })

        es.addEventListener('job_live_terminal', (evt: Event) => {
          const data = (evt as MessageEvent).data
          if (typeof data !== 'string' || !data) return
          try {
            const parsed = JSON.parse(data) as unknown
            if (!parsed || typeof parsed !== 'object') return
            const p = parsed as Record<string, unknown>
            if (p.type !== 'job_live_terminal') return
            const commandSeq = typeof p.commandSeq === 'number' && Number.isSafeInteger(p.commandSeq) ? p.commandSeq : null
            if (commandSeq === null) return
            const ts = typeof p.ts === 'string' ? p.ts : new Date().toISOString()
            const terminalLines = parseTerminalLines(p.lines)
            liveCommandOutputSeqsRef.current.add(commandSeq)
            setLogs((prev) => {
              const retained = prev.filter((log) => log.terminalCommandSeq !== commandSeq || log.terminalFrozen)
              const next = terminalLines.map((segments) => ({
                ts,
                level: '',
                msg: '',
                transient: true,
                terminalCommandSeq: commandSeq,
                terminalSegments: segments,
              }))
              const combined = [...retained, ...next]
              return combined.length > 500 ? combined.slice(-500) : combined
            })
          } catch {
            // ignore invalid events
          }
        })

        es.addEventListener('job_live_command_complete', (evt: Event) => {
          const data = (evt as MessageEvent).data
          if (typeof data !== 'string' || !data) return
          try {
            const parsed = JSON.parse(data) as unknown
            if (!parsed || typeof parsed !== 'object') return
            const p = parsed as Record<string, unknown>
            if (p.type !== 'job_live_command_complete') return
            const commandSeq = typeof p.commandSeq === 'number' && Number.isSafeInteger(p.commandSeq) ? p.commandSeq : null
            const summaryPersisted = p.summaryPersisted !== false
            if (commandSeq !== null) {
              if (summaryPersisted && liveCommandOutputSeqsRef.current.has(commandSeq)) {
                pendingCommandSummarySeqsRef.current.add(commandSeq)
              }
              setLogs((prev) => {
                if (summaryPersisted) {
                  return prev.filter((log) => log.terminalCommandSeq !== commandSeq)
                }
                return prev.map((log) =>
                  log.terminalCommandSeq === commandSeq ? { ...log, terminalFrozen: true } : log,
                )
              })
            }
            if (commandSeq !== null) liveCommandOutputSeqsRef.current.delete(commandSeq)
          } catch {
            // ignore invalid events
          }
        })

        es.addEventListener('job_progress', (evt: Event) => {
          const data = (evt as MessageEvent).data
          if (typeof data !== 'string' || !data) return
          try {
            const parsed = JSON.parse(data) as unknown
            if (!parsed || typeof parsed !== 'object') return
            const envelope = parsed as Record<string, unknown>
            if (envelope.type !== 'job_progress') return
            const p = envelope.progress && typeof envelope.progress === 'object'
              ? envelope.progress as Record<string, unknown>
              : envelope
            const plannedPercent = Object.prototype.hasOwnProperty.call(p, 'plannedPercent')
              ? typeof p.plannedPercent === 'number'
                ? p.plannedPercent
                : null
              : undefined
            const next = normalizeProgress({
              phase: typeof p.phase === 'string' ? p.phase : 'running',
              message: typeof p.message === 'string' ? p.message : '',
              current: typeof p.current === 'number' ? p.current : 0,
              total: typeof p.total === 'number' ? p.total : 0,
              percent: typeof p.percent === 'number' ? p.percent : 0,
              plannedCurrent: typeof p.plannedCurrent === 'number' ? p.plannedCurrent : null,
              plannedTotal: typeof p.plannedTotal === 'number' ? p.plannedTotal : null,
              ...(plannedPercent === undefined ? {} : { plannedPercent }),
              currentTarget: typeof p.currentTarget === 'string' ? p.currentTarget : null,
              download: parseJobProgressDownload(p.download),
              backup: p.backup && typeof p.backup === 'object'
                ? p.backup as JobProgress['backup']
                : null,
              updatedAt: typeof p.updatedAt === 'string' ? p.updatedAt : new Date().toISOString(),
            })
            if (next) setProgress(next)
          } catch {
            // ignore invalid events
          }
        })

        es.onerror = () => {
          if (restarting) return
          errorStreak += 1
          // The backend closes the SSE stream shortly after a job is finished (idle window).
          // Refresh once on close/error so status/finishedAt become up-to-date.
          scheduleRefresh(0)

          if (errorStreak >= 3) setError('实时日志连接不稳定，正在由浏览器重连。')
        }
      } catch (e: unknown) {
        setError(errorMessage(e))
      }
    }

    void start()

    return () => {
      closed = true
      if (refreshTimer != null) window.clearTimeout(refreshTimer)
      es?.close()
    }
  }, [jobId, manualRefreshVersion, refresh])

  useManagementEventBatch(({ events, resyncRequired }) => {
    const relevant = resyncRequired || events.some((event) =>
      event.summary.jobId === jobId || event.entities.some((entity) => entity.entityType === 'job' && entity.id === jobId),
    )
    if (!relevant) return
    void refresh().catch(() => {})
  })

  useEffect(() => {
    setLogFollow(true)
    setLogIsAtBottom(true)
  }, [jobId])

  useEffect(() => {
    if (!logViewport) return
    const element = logViewport
    const onScroll = () => {
      const nearBottom = isLogViewportNearBottom(element)
      setLogIsAtBottom(nearBottom)
      if (!nearBottom) setLogFollow(false)
      else setLogFollow(true)
    }
    element.addEventListener('scroll', onScroll)
    return () => element.removeEventListener('scroll', onScroll)
  }, [logViewport])

  useEffect(() => {
    if (!logViewport || !logFollow || visibleLogs.length === 0) return
    const frame = window.requestAnimationFrame(() => {
      scrollLogViewportToBottom(logViewport)
      setLogIsAtBottom(true)
    })
    return () => window.cancelAnimationFrame(frame)
  }, [logFollow, logViewport, visibleLogs])

  useEffect(() => {
    onTopActions(
      <>
        <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: 'queue' })}>
          返回列表
        </Button>
        <Button
          variant="ghost"
          disabled={busy}
          onClick={() => {
            void (async () => {
              setBusy(true)
              try {
                await refresh()
                setManualRefreshVersion((version) => version + 1)
              } catch (e: unknown) {
                setError(errorMessage(e))
              } finally {
                setBusy(false)
              }
            })()
          }}
        >
          刷新
        </Button>
      </>,
    )
  }, [busy, onTopActions, refresh])

  const knownProgressPercent = progress ? getKnownProgressPercent(progress) : null
  const knownPlannedPercent = progress ? getKnownPlannedProgressPercent(progress) : null
  const isRunning = job?.status === 'running'
  const completedZeroPercentWhileRunning =
    progress !== null &&
    isRunning &&
    knownProgressPercent === 0 &&
    progress.total > 0 &&
    progress.current < progress.total
  const plannedCurrent = progress ? (progress.plannedCurrent ?? progress.current) : 0
  const plannedTotal = progress ? (progress.plannedTotal ?? progress.total) : 0
  const plannedZeroPercentWhileRunning =
    progress !== null && isRunning && knownPlannedPercent === 0 && plannedTotal > 0 && plannedCurrent < plannedTotal
  const isCompletedIndeterminateRunning =
    progress !== null && isRunning && (knownProgressPercent === null || completedZeroPercentWhileRunning)
  const isPlannedIndeterminateRunning =
    progress !== null && isRunning && (knownPlannedPercent === null || plannedZeroPercentWhileRunning)
  const displayedCompletedPercent = isCompletedIndeterminateRunning ? null : knownProgressPercent
  const displayedPlannedPercent = isPlannedIndeterminateRunning ? null : knownPlannedPercent
  const readable = job
    ? formatJobReadableDisplay(job.type, job.scope, job.summary)
    : { primaryLabel: '-', scopeTag: null, typeTone: 'default' as const }
  const plannedProgressLabel =
    displayedPlannedPercent !== null ? `${displayedPlannedPercent}%` : isRunning ? 'running' : job?.status ?? '-'
  const completedProgressLabel =
    displayedCompletedPercent !== null ? `${displayedCompletedPercent}%` : isRunning ? 'running' : job?.status ?? '-'
  const plannedProgressAriaText =
    displayedPlannedPercent !== null ? `${displayedPlannedPercent}%` : isRunning ? 'running' : job?.status ?? 'finished'
  const completedProgressAriaText =
    displayedCompletedPercent !== null ? `${displayedCompletedPercent}%` : isRunning ? 'running' : job?.status ?? 'finished'
  const dualProgressAriaText = `安排 ${plannedProgressAriaText} · 完成 ${completedProgressAriaText}`
  const isDualIndeterminate = isPlannedIndeterminateRunning || isCompletedIndeterminateRunning
  const downloadLabel = formatJobProgressDownload(progress?.download)

  return (
    <div className="page jobDetailPage">
      <div className="card">
        <div className="jobDetailHeader">
          <div className="jobDetailHeaderInfo">
            <div className="title">任务详情</div>
            {job ? (
              <div className="muted" style={{ marginTop: 8 }}>
                <div>
                  task{' '}
                  <span className="jobReadableTagGroup">
                    <span className={`jobTypeTag jobTypeTag-${readable.typeTone}`}>{readable.primaryLabel}</span>
                    {readable.scopeTag ? <span className="jobScopeTag">{readable.scopeTag}</span> : null}
                  </span>{' '}
                  · machine{' '}
                  <Mono>{formatJobMachineName(job.type, job.scope)}</Mono> · by <Mono>{job.createdBy}</Mono> · reason{' '}
                  <Mono>{job.reason}</Mono>
                </div>
                <div style={{ marginTop: 6 }}>
                  created <Mono>{formatShort(job.createdAt)}</Mono> · started <Mono>{formatShort(job.startedAt)}</Mono>
                  {job.status !== 'running' ? (
                    <>
                      {' '}
                      · finished <Mono>{formatShort(job.finishedAt)}</Mono>
                    </>
                  ) : null}
                </div>
              </div>
            ) : null}
          </div>

          <div className="jobDetailHeaderAside">
            <div className="jobDetailIdentity">
              <div className="muted">
                job: <Mono>{jobId}</Mono>
              </div>
              {job ? (
                <Pill tone={statusTone(job.status)} breathing={job.status === 'running'}>
                  {job.status}
                </Pill>
              ) : null}
            </div>
            <div className="jobDetailStopSlot">
              {job?.stop?.canStop ? (
                <IconButton className="jobDetailStopControl" variant="danger" disabled={busy} onClick={() => void requestStop()} title="停止更新">
                  <Square size={14} aria-hidden="true" />
                </IconButton>
              ) : job?.status === 'cancelled' ? (
                <span className="jobDetailStopState">已停止</span>
              ) : job?.stop?.state === 'requested' ? (
                <span className="jobDetailStopState">正在停止</span>
              ) : null}
            </div>
          </div>
        </div>

        {progress ? (
          <div className="jobProgress">
            <div className="jobProgressHeader">
              <div className="title">进度</div>
              <div className="mono">
                安排 {plannedProgressLabel} · 完成 {completedProgressLabel}
              </div>
            </div>
            <div
              className={isDualIndeterminate ? 'jobProgressBar jobProgressBarDual jobProgressBarIndeterminate' : 'jobProgressBar jobProgressBarDual'}
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={displayedCompletedPercent ?? displayedPlannedPercent ?? undefined}
              aria-valuetext={dualProgressAriaText}
            >
              <div
                className={
                  isPlannedIndeterminateRunning
                    ? 'jobProgressFill jobProgressFillPlanned jobProgressFillLayerPlanned jobProgressFillIndeterminate'
                    : 'jobProgressFill jobProgressFillPlanned jobProgressFillLayerPlanned'
                }
                style={
                  displayedPlannedPercent !== null
                    ? { transform: `scaleX(${displayedPlannedPercent / 100})` }
                    : isPlannedIndeterminateRunning
                      ? undefined
                      : { transform: 'scaleX(1)' }
                }
              />
              <div
                className={
                  isCompletedIndeterminateRunning
                    ? 'jobProgressFill jobProgressFillCompleted jobProgressFillLayerCompleted jobProgressFillIndeterminate'
                    : 'jobProgressFill jobProgressFillCompleted jobProgressFillLayerCompleted'
                }
                style={
                  displayedCompletedPercent !== null
                    ? { transform: `scaleX(${displayedCompletedPercent / 100})` }
                    : isCompletedIndeterminateRunning
                      ? undefined
                      : { transform: 'scaleX(1)' }
                }
              />
            </div>
            <div className="jobProgressCounters">
              <span>
                安排 <Mono>{plannedTotal > 0 ? `${plannedCurrent}/${plannedTotal}` : '-'}</Mono>
              </span>
              <span>
                完成 <Mono>{progress.total > 0 ? `${progress.current}/${progress.total}` : '-'}</Mono>
              </span>
            </div>
            <div className="jobProgressMeta">
              <span>
                <Mono>{progress.phase || '-'}</Mono>
                {progress.message ? ` · ${progress.message}` : ''}
              </span>
              {downloadLabel ? (
                <span>
                  下载 <Mono>{downloadLabel}</Mono>
                </span>
              ) : null}
              <span>
                安排 <Mono>{plannedTotal > 0 ? `${plannedCurrent}/${plannedTotal}` : '-'}</Mono> · 完成{' '}
                <Mono>{progress.total > 0 ? `${progress.current}/${progress.total}` : '-'}</Mono>
                {progress.currentTarget ? ` · ${progress.currentTarget}` : ''}
              </span>
              <span>
                updated <Mono>{formatShort(progress.updatedAt)}</Mono>
              </span>
            </div>
          </div>
        ) : job?.status === 'running' ? (
          <div className="jobProgress">
            <div className="jobProgressHeader">
              <div className="title">进度</div>
              <div className="mono">安排 running · 完成 running</div>
            </div>
            <div className="jobProgressBar jobProgressBarDual jobProgressBarIndeterminate" role="progressbar" aria-valuetext="安排 running · 完成 running">
              <div className="jobProgressFill jobProgressFillPlanned jobProgressFillLayerPlanned jobProgressFillIndeterminate" />
              <div className="jobProgressFill jobProgressFillCompleted jobProgressFillLayerCompleted jobProgressFillIndeterminate" />
            </div>
            <div className="jobProgressCounters">
              <span>
                安排 <Mono>-</Mono>
              </span>
              <span>
                完成 <Mono>-</Mono>
              </span>
            </div>
            <div className="jobProgressMeta">
              <span>运行中，等待进度数据…</span>
            </div>
          </div>
        ) : null}

        {job?.status !== 'running' ? (
          <TaskResultReason reason={job?.resultReason} lines={2} className="jobResultReason" label="结果原因" />
        ) : null}

        {error ? <div className="error">{error}</div> : null}
      </div>

      <div className="card jobDetailLogsCard">
        <div className="sectionRow">
          <div className="title">日志</div>
          <div style={{ marginLeft: 'auto' }} className="chipRow">
            {!logFollow && visibleLogs.length > 0 ? (
              <Button
                data-job-detail-log-jump="true"
                onClick={() => {
                  if (logViewport) {
                    scrollLogViewportToBottom(logViewport)
                  }
                  setLogFollow(true)
                  setLogIsAtBottom(true)
                }}
                variant="primary"
              >
                跳到最新
              </Button>
            ) : null}
            <span className="muted">时区</span>
            <Chip active={logTz === 'local'} onClick={() => setLogTz('local')} title={`浏览器时区：${LOCAL_TZ}`}>
              本地
            </Chip>
            <Chip active={logTz === 'utc'} onClick={() => setLogTz('utc')} title="后端存储的 job log ts 为 RFC3339（UTC）">
              UTC
            </Chip>
            <label className="chipRow" style={{ marginLeft: 8 }}>
              <Switch
                aria-label="显示 EVEN"
                checked={showEvents}
                data-job-detail-log-show-events="true"
                onChange={setShowEvents}
              />
              <span className="muted">显示 EVEN</span>
            </label>
          </div>
        </div>

        <OverlayScrollArea
          className="logs"
          data-job-detail-log-at-bottom={logIsAtBottom ? 'true' : 'false'}
          data-job-detail-log-count={visibleLogs.length}
          data-job-detail-log-follow={logFollow ? 'true' : 'false'}
          data-job-detail-log-surface="true"
          onViewportReady={setLogViewport}
          viewportLabel="任务日志"
        >
          {visibleLogs.map((l, idx) => (
            <div
              key={`${l.ts}-${idx}`}
              className={`logLine ${l.terminalSegments ? 'logLine-terminal' : `logLine-${(l.level ?? '').trim().toLowerCase() || 'unknown'}`}`}
            >
              <span className="mono logTs" title={formatLogTitle(l.ts)}>
                {formatLogTs(l.ts, logTz)}
              </span>
              <span className={`mono logLvl logLvl-${(l.level ?? '').trim().toLowerCase()}`}>
                {l.terminalSegments ? '' : formatLogLevel(l.level)}
              </span>
              <span className="logMsg">
                {l.terminalSegments
                  ? l.terminalSegments.map((segment, segmentIndex) => (
                      <span
                        key={`${segment.text}-${segmentIndex}`}
                        style={{
                          color: safeTerminalColor(segment.fg),
                          backgroundColor: safeTerminalColor(segment.bg),
                          fontWeight: segment.bold ? 700 : undefined,
                          opacity: segment.dim ? 0.65 : undefined,
                          textDecoration: segment.underline ? 'underline' : undefined,
                        }}
                      >
                        {segment.text}
                      </span>
                    ))
                  : l.msg}
              </span>
            </div>
          ))}
          {visibleLogs.length === 0 ? <div className="muted">无日志</div> : null}
        </OverlayScrollArea>
      </div>
    </div>
  )
}
