import { useCallback,useEffect,useRef,useState } from 'react'
import {
type DiscoveredProject,
type Service,
type ServiceDigestTagsScanSummary,
type StackListItem
} from '../api'
import { type AggregateUpdatePreviewListItem } from '../components/AggregateUpdatePreviewList'
import { type UpdateCandidateFilter } from '../components/UpdateCandidateFilters'
import { isDockrevImageRef } from '../runtimeConfig'
import {
Dialog,
DialogClose,
DialogContent,
DialogDescription,
DialogFooter,
DialogHeader,
DialogTitle,
Pill
} from '../ui'
import { type RowStatus } from '../updateStatus'
import {
isStrictSemverTag
} from '../versionDisplay'



export function formatShort(ts?: string | null) {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

export function formatCompactDateTime(ts?: string | null) {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  const h = String(d.getHours()).padStart(2, '0')
  const min = String(d.getMinutes()).padStart(2, '0')
  return `${m}/${day} ${h}:${min}`
}

export function scanHasFailures(scan: ServiceDigestTagsScanSummary | null | undefined): boolean {
  if (!scan) return false
  return scan.manifestsTimeout > 0 || scan.manifestsError > 0
}

export function scanIsComplete(scan: ServiceDigestTagsScanSummary | null | undefined): boolean {
  if (!scan) return false
  return scan.repoTagsConsidered >= scan.repoTagsTotal
}

export function getDiscoveryScanStartedAt(summary: unknown): string | null {
  if (typeof summary !== 'object' || summary === null) return null
  const scan = (summary as Record<string, unknown>).scan
  if (typeof scan !== 'object' || scan === null) return null
  const startedAt = (scan as Record<string, unknown>).startedAt
  return typeof startedAt === 'string' ? startedAt : null
}

export function isDockrevService(svc: Service): boolean {
  return isDockrevImageRef(svc.image.ref)
}

export function shouldPrefetchFloatingCandidate(
  candidateTag: string | null | undefined,
  candidateResolvedTag: string | null | undefined,
  candidateDigest: string | null | undefined,
): boolean {
  const raw = (candidateTag ?? '').trim()
  if (raw === '-') return false
  if (!raw || isStrictSemverTag(raw)) return false
  if (isStrictSemverTag(candidateResolvedTag)) return false
  return (candidateDigest ?? '').trim().length > 0
}

export function StackIcon(props: { variant: 'collapsed' | 'expanded' }) {
  return (
    <svg className="stackIcon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {props.variant === 'expanded' ? (
        <path d="m5 19l2.757-7.351A1 1 0 0 1 8.693 11H21a1 1 0 0 1 .986 1.164l-.996 5.211A2 2 0 0 1 19.026 19za2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h4l3 3h7a2 2 0 0 1 2 2v2" />
      ) : (
        <path d="M5 4h4l3 3h7a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2" />
      )}
    </svg>
  )
}

export function formatGroupSummary(services: number, counts: Record<Exclude<RowStatus, 'ok'>, number>) {
  const parts: string[] = [`${services} services`]
  if (counts.updatable > 0) parts.push(`${counts.updatable} 可更新`)
  if (counts.hint > 0) parts.push(`${counts.hint} 需确认`)
  if (counts.archMismatch > 0) parts.push(`${counts.archMismatch} 架构不匹配`)
  if (counts.blocked > 0) parts.push(`${counts.blocked} 被阻止`)
  return parts.join(' · ')
}

export function withAggregateDisplayName(
  items: Array<Pick<AggregateUpdatePreviewListItem, 'svc' | 'status' | 'guardedDockrev'>>,
  stackName?: string,
  stackId?: string,
): AggregateUpdatePreviewListItem[] {
  return items.map((item) => ({
    ...item,
    displayName: stackName ? `${stackName}/${item.svc.name}` : item.svc.name,
    stackId,
  }))
}

export function GroupGuide() {
  return <div className="groupGuide" aria-hidden="true" />
}

export type DiscoveryIssueTone = 'warning' | 'missing' | 'invalid'

export type DiscoveryIssueItem = {
  project: string
  tone: DiscoveryIssueTone
  label: string
  summary: string
  fullError: string | null
  lastSeenAt: string | null
  lastScanAt: string | null
  configSummary: string | null
  stackId: string | null
}

export const DISCOVERY_ISSUE_ORDER: Record<DiscoveryIssueTone, number> = {
  invalid: 0,
  missing: 1,
  warning: 2,
}

export function truncateText(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max - 1).trimEnd()}…` : text
}

export function compactPathLabel(path: string): string {
  const trimmed = path.trim()
  if (!trimmed) return '-'
  const parts = trimmed.split(/[\\/]/).filter(Boolean)
  return parts[parts.length - 1] ?? trimmed
}

export function formatDiscoveryConfigSummary(configFiles?: string[] | null): string | null {
  const items = (configFiles ?? []).map((item) => item.trim()).filter(Boolean)
  if (items.length === 0) return null
  const first = compactPathLabel(items[0])
  if (items.length === 1) return `配置 ${first}`
  return `配置 ${first} +${items.length - 1}`
}

export function normalizeDiscoveryIssueError(message?: string | null): string | null {
  const raw = (message ?? '').trim()
  if (!raw) return null
  let normalized = raw.replace(/\s+/g, ' ').trim()
  while (/^(warning|invalid|missing)\s*:\s*/i.test(normalized)) {
    normalized = normalized.replace(/^(warning|invalid|missing)\s*:\s*/i, '')
  }
  normalized = normalized.replace(/^[a-z0-9_]+:\s*/i, '').trim()
  return normalized || raw
}

export function summarizeDiscoveryIssueError(message?: string | null): { summary: string | null; fullError: string | null } {
  const full = normalizeDiscoveryIssueError(message)
  if (!full) return { summary: null, fullError: null }

  const hintIndex = full.search(/\bHint:/i)
  const withoutHint = hintIndex >= 0 ? full.slice(0, hintIndex).trim().replace(/[;:,.]+$/, '') : full
  const summary = truncateText(withoutHint || full, 120)
  return { summary, fullError: full === summary ? null : full }
}

export function buildDiscoveryIssue(project: DiscoveredProject, tone: DiscoveryIssueTone): DiscoveryIssueItem {
  const { summary, fullError } = summarizeDiscoveryIssueError(project.lastError)
  return {
    project: project.project,
    tone,
    label: tone === 'warning' ? '告警' : tone === 'missing' ? '缺失' : '无效',
    summary:
      summary ??
      (tone === 'warning'
        ? '发现扫描已标记告警，请检查 compose 与挂载状态。'
        : tone === 'missing'
          ? '发现项目已缺失，请检查 compose 文件或挂载路径。'
          : '发现项目无效，请修复 compose / override 配置。'),
    fullError,
    lastSeenAt: project.lastSeenAt ?? null,
    lastScanAt: project.lastScanAt ?? null,
    configSummary: formatDiscoveryConfigSummary(project.configFiles),
    stackId: project.stackId ?? null,
  }
}

export function latestDiscoveryObservationAt(issue: Pick<DiscoveryIssueItem, 'lastSeenAt' | 'lastScanAt'>): string {
  const seenAt = issue.lastSeenAt ?? ''
  const scanAt = issue.lastScanAt ?? ''
  return seenAt.localeCompare(scanAt) >= 0 ? seenAt : scanAt
}

export function buildDiscoveryIssueMetaParts(
  issue: Pick<DiscoveryIssueItem, 'lastSeenAt' | 'lastScanAt' | 'configSummary' | 'stackId'>,
): string[] {
  return [
    issue.lastSeenAt ? `最近发现 ${formatCompactDateTime(issue.lastSeenAt)}` : null,
    issue.lastScanAt ? `最近扫描 ${formatCompactDateTime(issue.lastScanAt)}` : null,
    issue.configSummary,
    issue.stackId ? `关联 ${issue.stackId}` : null,
  ].filter((part): part is string => Boolean(part))
}

export function DiscoveryIssueDetailDialog(props: {
  issue: DiscoveryIssueItem | null
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const [copyState, setCopyState] = useState<'idle' | 'success' | 'error'>('idle')
  const copyResetTimerRef = useRef<number | null>(null)

  useEffect(() => {
    return () => {
      if (copyResetTimerRef.current != null) {
        window.clearTimeout(copyResetTimerRef.current)
      }
    }
  }, [])

  useEffect(() => {
    if (props.open) return
    if (copyResetTimerRef.current != null) {
      window.clearTimeout(copyResetTimerRef.current)
      copyResetTimerRef.current = null
    }
    setCopyState('idle')
  }, [props.open])

  const scheduleCopyReset = useCallback(() => {
    if (copyResetTimerRef.current != null) {
      window.clearTimeout(copyResetTimerRef.current)
    }
    copyResetTimerRef.current = window.setTimeout(() => {
      copyResetTimerRef.current = null
      setCopyState('idle')
    }, 1600)
  }, [])

  const handleCopy = useCallback(async () => {
    const text = props.issue?.fullError?.trim()
    if (!text) return
    try {
      if (!navigator.clipboard?.writeText) {
        throw new Error('Clipboard API unavailable')
      }
      await navigator.clipboard.writeText(text)
      setCopyState('success')
    } catch {
      setCopyState('error')
    } finally {
      scheduleCopyReset()
    }
  }, [props.issue?.fullError, scheduleCopyReset])

  const issue = props.issue
  if (!issue) return null

  const metaParts = buildDiscoveryIssueMetaParts(issue)
  const pillTone = issue.tone === 'warning' ? 'warn' : 'bad'
  const fullError = issue.fullError ?? issue.summary
  const copyButtonClassName =
    copyState === 'success' ? 'btn btnPrimary' : copyState === 'error' ? 'btn btnDanger' : 'btn btnGhost'
  const copyLabel = copyState === 'success' ? '已复制' : copyState === 'error' ? '复制失败' : '复制完整详情'

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="modalCard discoveryIssueDialogCard">
        <DialogHeader className="modalHeader">
          <div className="modalTitleRow discoveryIssueDialogTitleRow">
            <div className="discoveryIssueDialogTitleWrap">
              <Pill tone={pillTone}>{issue.label}</Pill>
              <DialogTitle asChild>
                <div className="modalTitle">
                  <span className="mono monoPrimary">{issue.project}</span>
                </div>
              </DialogTitle>
            </div>
          </div>
          <DialogDescription asChild>
            <div className="modalBody discoveryIssueDialogBody">
              <div className="discoveryIssueDialogSummary">{issue.summary}</div>
              {metaParts.length > 0 ? (
                <div className="discoveryIssueDialogMeta">
                  {metaParts.map((part) => (
                    <span key={`${issue.project}:${part}`} className="discoveryIssueDialogMetaItem">
                      {part}
                    </span>
                  ))}
                </div>
              ) : null}
              <div className="discoveryIssueDialogSectionLabel">完整异常详情</div>
              <pre className="discoveryIssueDialogError">{fullError}</pre>
            </div>
          </DialogDescription>
        </DialogHeader>
        <DialogFooter className="modalActions">
          <DialogClose asChild>
            <button type="button" className="btn btnGhost">
              关闭
            </button>
          </DialogClose>
          <button type="button" className={copyButtonClassName} onClick={() => void handleCopy()}>
            {copyLabel}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export const UPDATE_CANDIDATE_FILTER_QUERY_KEY = 'updates'
export const UPDATE_CANDIDATE_COLLAPSED_STORAGE_PREFIX = 'dockrev:overview:updateCandidates:collapsed:v1:'
export const OVERVIEW_JOBS_SSE_REFRESH_DEBOUNCE_MS = 180
export const OVERVIEW_JOBS_SSE_FALLBACK_POLL_MS = 5000
export const OVERVIEW_JOBS_SSE_ERROR_THRESHOLD = 3
export const OVERVIEW_JOBS_SSE_RECONNECT_MS = 1500
export const UPDATE_CANDIDATE_FILTERS: UpdateCandidateFilter[] = [
  'all',
  'updatable',
  'hint',
  'archMismatch',
  'blocked',
]

export function normalizeUpdateCandidateFilter(value: string | null): UpdateCandidateFilter | null {
  const v = (value ?? '').trim()
  if (!v) return null
  // `UpdateCandidateFilter` is a string union; keep this explicit to avoid accidental acceptance.
  if ((UPDATE_CANDIDATE_FILTERS as readonly string[]).includes(v)) return v as UpdateCandidateFilter
  return null
}

export function readUpdateCandidateFilterFromUrl(): UpdateCandidateFilter | null {
  try {
    const params = new URLSearchParams(window.location.search)
    return normalizeUpdateCandidateFilter(params.get(UPDATE_CANDIDATE_FILTER_QUERY_KEY))
  } catch {
    return null
  }
}

export function writeUpdateCandidateFilterToUrl(filter: UpdateCandidateFilter, mode: 'push' | 'replace') {
  const key = UPDATE_CANDIDATE_FILTER_QUERY_KEY
  try {
    const url = new URL(window.location.href)
    if (filter === 'all') url.searchParams.delete(key)
    else url.searchParams.set(key, filter)

    const next = `${url.pathname}${url.search}${url.hash}`
    if (mode === 'push') window.history.pushState({}, '', next)
    else window.history.replaceState({}, '', next)
  } catch {
    // ignore URL update errors (e.g. locked-down environments)
  }
}

export function readCollapsedFromStorage(filter: UpdateCandidateFilter): Record<string, boolean> {
  const key = `${UPDATE_CANDIDATE_COLLAPSED_STORAGE_PREFIX}${filter}`
  try {
    const raw = window.localStorage.getItem(key)
    if (!raw) return {}
    const json = JSON.parse(raw)
    if (!json || typeof json !== 'object') return {}
    const out: Record<string, boolean> = {}
    for (const [k, v] of Object.entries(json as Record<string, unknown>)) {
      if (typeof k !== 'string' || !k) continue
      if (typeof v !== 'boolean') continue
      out[k] = v
    }
    return out
  } catch {
    return {}
  }
}

export function writeCollapsedToStorage(filter: UpdateCandidateFilter, value: Record<string, boolean>) {
  const key = `${UPDATE_CANDIDATE_COLLAPSED_STORAGE_PREFIX}${filter}`
  try {
    window.localStorage.setItem(key, JSON.stringify(value))
  } catch {
    // ignore quota/serialization errors
  }
}

export function withCollapseDefaults(
  collapsed: Record<string, boolean>,
  stacks: StackListItem[],
): Record<string, boolean> {
  const next = { ...collapsed }
  for (const st of stacks) {
    if (next[st.id] == null) next[st.id] = st.updates === 0
  }
  return next
}
