import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  getServiceLogs,
  newServiceLogsEventsSource,
  type ServiceLogEventEnvelope,
  type ServiceLogLine,
  type ServiceLogMeta,
} from '../api'

export const SERVICE_LOG_INITIAL_TAIL = 500
export const SERVICE_LOG_BUFFER_LIMIT = 2_000
const ESC = String.fromCharCode(27)
const ANSI_SGR_PATTERN = new RegExp(`${ESC}\\[[0-9;]*m`, 'g')
const ANSI_ERROR_PATTERN = new RegExp(`${ESC}\\[(?:[0-9;]*;)?31m`)
const ANSI_WARN_PATTERN = new RegExp(`${ESC}\\[(?:[0-9;]*;)?33m`)
const ANSI_INFO_PATTERN = new RegExp(`${ESC}\\[(?:[0-9;]*;)?(?:32|36)m`)
const ANSI_DEBUG_PATTERN = new RegExp(`${ESC}\\[(?:[0-9;]*;)?90m`)
const INLINE_TRACING_LEVEL_PATTERN =
  /^\d{4}-\d{2}-\d{2}T\S+\s+(TRACE|DEBUG|INFO|WARN|WARNING|ERROR|ERR|FATAL|CRITICAL)\b/i
const ANSI_FOREGROUND_TOKENS: Partial<Record<number, string>> = {
  31: 'var(--service-log-ansi-red)',
  32: 'var(--service-log-ansi-green)',
  33: 'var(--service-log-ansi-yellow)',
  34: 'var(--service-log-ansi-blue)',
  35: 'var(--service-log-ansi-magenta)',
  36: 'var(--service-log-ansi-cyan)',
  37: 'var(--service-log-ansi-white)',
  90: 'var(--service-log-ansi-muted)',
}

export type ServiceLogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace' | 'unknown'

export type ServiceLogRenderSegment = {
  text: string
  style?: React.CSSProperties
}

export type ServiceLogRecord = {
  id: number
  ts: string
  raw: string
  plain: string
  message: string
  meta?: ServiceLogMeta | null
  searchText: string
  level: ServiceLogLevel
  inlineLevel: boolean
  multiline: boolean
  segments: ServiceLogRenderSegment[]
}

function stripAnsi(input: string): string {
  return input.replace(ANSI_SGR_PATTERN, '')
}

function ansiSegments(input: string): ServiceLogRenderSegment[] {
  const segments: ServiceLogRenderSegment[] = []
  const ansiPattern = new RegExp(`${ESC}\\[([0-9;]*)m`, 'g')
  let foreground: string | undefined
  let bold = false
  let lastIndex = 0

  const pushText = (text: string) => {
    if (!text) return
    segments.push({
      text,
      style:
        foreground || bold
          ? {
              ...(foreground ? { color: foreground } : null),
              ...(bold ? { fontWeight: 700 } : null),
            }
          : undefined,
    })
  }

  for (const match of input.matchAll(ansiPattern)) {
    const index = match.index ?? 0
    pushText(input.slice(lastIndex, index))
    const codes = (match[1] ?? '')
      .split(';')
      .map((value) => Number.parseInt(value, 10))
      .filter(Number.isFinite)
    if (codes.length === 0 || codes.includes(0)) {
      foreground = undefined
      bold = false
    }
    for (const code of codes) {
      if (code === 1) bold = true
      const foregroundToken = ANSI_FOREGROUND_TOKENS[code]
      if (foregroundToken) foreground = foregroundToken
    }
    lastIndex = index + match[0].length
  }
  pushText(input.slice(lastIndex))
  return segments.length > 0 ? segments : [{ text: input }]
}

function inferLogLevel(raw: string, plain: string): ServiceLogLevel {
  if (ANSI_ERROR_PATTERN.test(raw)) return 'error'
  if (ANSI_WARN_PATTERN.test(raw)) return 'warn'
  if (ANSI_DEBUG_PATTERN.test(raw)) return 'debug'
  if (ANSI_INFO_PATTERN.test(raw)) return 'info'

  const lower = plain.trim().toLowerCase()
  if (!lower) return 'unknown'
  if (/\btrace\b/.test(lower)) return 'trace'
  if (/\bdebug\b|\bverbose\b|cache warmup/.test(lower)) return 'debug'
  if (/\bfatal\b|\bpanic\b|\bexception\b|\berr(or)?\b|\bfailed\b|\btimeout\b|\bdenied\b|\bunavailable\b/.test(lower)) {
    return 'error'
  }
  if (/\bwarn(ing)?\b|slow query|\bretry\b|\bdegraded\b|\bstale\b/.test(lower)) return 'warn'
  if (/\binfo\b|\bboot\b|\bcomplete\b|\bconnected\b|\bserving\b|\breload\b|\bready\b|\bhealthz\b|\breadiness\b/.test(lower)) {
    return 'info'
  }
  return 'unknown'
}

function normalizeLogLevel(level: string | null | undefined): ServiceLogLevel | null {
  const value = (level ?? '').trim().toLowerCase()
  if (!value) return null
  if (value === 'trace') return 'trace'
  if (value === 'debug' || value === 'verbose') return 'debug'
  if (value === 'info') return 'info'
  if (value === 'warn' || value === 'warning') return 'warn'
  if (value === 'error' || value === 'err' || value === 'fatal' || value === 'critical') return 'error'
  return null
}

function hasInlineTracingLevel(plain: string): boolean {
  return INLINE_TRACING_LEVEL_PATTERN.test(plain.trimStart())
}

function metadataSearchText(meta: ServiceLogMeta | null | undefined): string {
  if (!meta) return ''
  const attributes = meta.attributes ?? {}
  const attributeText = Object.entries(attributes)
    .map(([key, value]) => `${key}=${formatSearchValue(value)}`)
    .join(' ')
  return [meta.format, meta.level, meta.timestamp, meta.message, attributeText, ...(meta.highlights ?? [])]
    .filter(Boolean)
    .join(' ')
}

function formatSearchValue(value: unknown): string {
  if (value == null) return ''
  if (typeof value === 'string') return value
  if (typeof value === 'number' || typeof value === 'boolean') return String(value)
  try {
    return JSON.stringify(value)
  } catch {
    return String(value)
  }
}

function toRecord(line: ServiceLogLine, id: number): ServiceLogRecord {
  const plain = stripAnsi(line.plain || line.raw)
  const metaLevel = normalizeLogLevel(line.meta?.level)
  const level = metaLevel ?? inferLogLevel(line.raw, plain)
  const message = (line.meta?.message ?? '').trim() || plain
  const searchText = [plain, line.raw, metadataSearchText(line.meta)].join(' ').toLowerCase()
  return {
    id,
    ts: line.ts,
    raw: line.raw,
    plain,
    message,
    meta: line.meta,
    searchText,
    level,
    inlineLevel: !metaLevel && hasInlineTracingLevel(plain),
    multiline: line.raw.includes('\n'),
    segments: ansiSegments(line.raw),
  }
}

export function useServiceLogsState(serviceId: string) {
  const [records, setRecords] = useState<ServiceLogRecord[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [resetNonce, setResetNonce] = useState(0)
  const lastEventIdRef = useRef(0)

  const refreshSnapshot = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const snapshot = await getServiceLogs(serviceId, SERVICE_LOG_INITIAL_TAIL)
      lastEventIdRef.current = snapshot.lastEventId
      setRecords(
        snapshot.lines.map((line, index) =>
          toRecord(line, Math.max(1, snapshot.lastEventId - snapshot.lines.length + index + 1)),
        ),
      )
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [serviceId])

  useEffect(() => {
    void refreshSnapshot()
  }, [refreshSnapshot])

  useEffect(() => {
    let closed = false
    let eventSource: EventSource | null = null

    const start = () => {
      eventSource = newServiceLogsEventsSource(
        serviceId,
        lastEventIdRef.current > 0 ? { afterId: lastEventIdRef.current } : undefined,
      )

      eventSource.addEventListener('service_log_line', (event: Event) => {
        const payload = (event as MessageEvent).data
        if (typeof payload !== 'string' || !payload) return
        try {
          const parsed = JSON.parse(payload) as ServiceLogEventEnvelope
          if (parsed.type !== 'line') return
          setRecords((prev) => {
            const next = [...prev, toRecord(parsed.line, parsed.id)]
            return next.length > SERVICE_LOG_BUFFER_LIMIT ? next.slice(-SERVICE_LOG_BUFFER_LIMIT) : next
          })
          lastEventIdRef.current = parsed.id
        } catch {
          // ignore malformed events
        }
      })

      eventSource.addEventListener('service_log_reset', (event: Event) => {
        const payload = (event as MessageEvent).data
        if (typeof payload !== 'string' || !payload) return
        try {
          const parsed = JSON.parse(payload) as ServiceLogEventEnvelope
          if (parsed.type !== 'reset') return
          if (parsed.id > 0) {
            lastEventIdRef.current = Math.max(lastEventIdRef.current, parsed.id)
          }
          if (parsed.reason === 'buffer_gap_reset' || parsed.reason === 'subscriber_lagged') {
            setResetNonce((value) => value + 1)
            void refreshSnapshot()
          }
        } catch {
          void refreshSnapshot()
        }
      })

      eventSource.onerror = () => {
        if (closed) return
      }
    }

    if (!loading) start()
    return () => {
      closed = true
      eventSource?.close()
    }
  }, [loading, refreshSnapshot, serviceId])

  const normalizedQuery = query.trim().toLowerCase()
  const filteredRecords = useMemo(() => {
    if (!normalizedQuery) return records
    return records.filter((record) => record.searchText.includes(normalizedQuery))
  }, [normalizedQuery, records])

  return {
    error,
    filteredRecords,
    loading,
    query,
    records,
    resetNonce,
    setQuery,
  }
}
