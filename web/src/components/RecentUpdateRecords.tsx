import type { JobListItem, ServiceBackupRecordItem, StackDetail } from '../api'
import { ChevronLeft, ChevronRight, RotateCcw, ScrollText } from 'lucide-react'
import { useMemo } from 'react'
import { openGitHubReleaseDrawer } from '../releaseDrawer'
import { navigate } from '../routes'
import { IconButton, Mono, Pill } from '../ui'
import { backupTargetCountLabel, summarizeServiceOperationBackups } from './serviceOperationBackupSummary'
import { TaskResultReason } from './TaskResultReason'

export const SERVICE_OPERATION_HISTORY_PAGE_SIZE = 20

export function formatCompactDateTime(ts?: string | null): string {
  if (!ts) return '-'
  const date = new Date(ts)
  if (Number.isNaN(date.valueOf())) return ts
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  const hour = String(date.getHours()).padStart(2, '0')
  const minute = String(date.getMinutes()).padStart(2, '0')
  return `${month}/${day} ${hour}:${minute}`
}

export function jobSortTime(job: JobListItem): number {
  const ts = job.finishedAt ?? job.startedAt ?? job.createdAt
  const time = new Date(ts).valueOf()
  return Number.isNaN(time) ? 0 : time
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value)
}

export function serviceIdsFromSummary(summary: unknown): Set<string> {
  const serviceIds = new Set<string>()
  if (!isRecord(summary)) return serviceIds

  if (typeof summary.serviceId === 'string') serviceIds.add(summary.serviceId)
  const targets = Array.isArray(summary.targets) ? summary.targets : []
  for (const target of targets) {
    if (!isRecord(target)) continue
    if (typeof target.serviceId === 'string') serviceIds.add(target.serviceId)
  }
  return serviceIds
}

function summaryVersion(value: Record<string, unknown>): string | null {
  for (const key of ['targetDisplayTag', 'targetTag', 'to']) {
    const version = value[key]
    if (typeof version === 'string' && version.trim()) return version.trim()
  }
  return null
}

export function releaseVersionForServiceOperation(job: JobListItem, serviceId: string): string | null {
  if (!isRecord(job.summary)) return null

  const directVersion = summaryVersion(job.summary)
  if (directVersion) return directVersion

  const targets = Array.isArray(job.summary.targets) ? job.summary.targets : []
  for (const target of targets) {
    if (!isRecord(target) || target.serviceId !== serviceId) continue
    const targetVersion = summaryVersion(target)
    if (targetVersion) return targetVersion
  }
  return null
}

export function selectRecentServiceUpdateJobs(jobs: JobListItem[], serviceId: string): JobListItem[] {
  return selectServiceOperationJobs(jobs, serviceId)
    .filter((job) => job.type === 'update')
    .sort((a, b) => jobSortTime(b) - jobSortTime(a))
    .slice(0, 3)
}

export function selectServiceOperationJobs(jobs: JobListItem[], serviceId: string, stackId?: string): JobListItem[] {
  return filterServiceOperationJobs(jobs, serviceId, stackId)
    .sort((a, b) => jobSortTime(b) - jobSortTime(a))
}

export function filterServiceOperationJobs(jobs: JobListItem[], serviceId: string, stackId?: string): JobListItem[] {
  return jobs
    .filter((job) => {
      if (job.type !== 'update' && job.type !== 'rollback' && job.type !== 'service_lifecycle' && job.type !== 'stack_lifecycle') return false
      if (job.serviceId === serviceId) return true
      if (stackId && job.scope === 'stack' && job.stackId === stackId) {
        return serviceIdsFromSummary(job.summary).has(serviceId)
      }
      return serviceIdsFromSummary(job.summary).has(serviceId)
    })
}

export function paginateServiceOperationJobs(jobs: JobListItem[], requestedPage: number, pageSize = SERVICE_OPERATION_HISTORY_PAGE_SIZE) {
  const normalizedPageSize = Math.max(1, pageSize)
  const totalPages = Math.max(1, Math.ceil(jobs.length / normalizedPageSize))
  const page = Math.min(Math.max(1, requestedPage), totalPages)
  const start = (page - 1) * normalizedPageSize
  return { page, totalPages, jobs: jobs.slice(start, start + normalizedPageSize) }
}

export function selectRecentStackUpdateJobs(jobs: JobListItem[], stack: StackDetail): JobListItem[] {
  const stackServiceIds = new Set(stack.services.map((service) => service.id))
  return jobs
    .filter((job) => {
      if (job.type !== 'update' && job.type !== 'stack_lifecycle') return false
      if (job.stackId === stack.id) return true
      for (const serviceId of serviceIdsFromSummary(job.summary)) {
        if (stackServiceIds.has(serviceId)) return true
      }
      return false
    })
    .sort((a, b) => jobSortTime(b) - jobSortTime(a))
    .slice(0, 3)
}

function statusTone(status: string): 'ok' | 'warn' | 'bad' | 'muted' | 'info' {
  if (status === 'success') return 'ok'
  if (status === 'rolled_back') return 'warn'
  if (status === 'running' || status === 'queued' || status === 'pending') return 'info'
  if (status === 'failed') return 'bad'
  return 'muted'
}

function reasonLabel(reason: string): string {
  if (reason === 'auto_policy') return 'auto policy'
  if (reason === 'ui') return 'manual'
  if (reason === 'webhook') return 'webhook'
  return reason
}

function operationLabel(job: JobListItem): string {
  if (job.type === 'rollback') return '回滚'
  if (job.type === 'service_lifecycle' || job.type === 'stack_lifecycle') {
    const action = job.summary && typeof job.summary === 'object' && 'action' in job.summary
      ? (job.summary as { action?: unknown }).action
      : null
    if (action === 'start') return '启动'
    if (action === 'stop') return '停止'
    if (action === 'restart') return '重启'
    return '服务生命周期'
  }
  return '更新'
}

function statusLabel(status: string): string {
  if (status === 'success') return '成功'
  if (status === 'rolled_back') return '已回滚'
  if (status === 'running') return '执行中'
  if (status === 'queued') return '排队中'
  if (status === 'pending') return '等待中'
  if (status === 'failed') return '失败'
  return status
}

function resultReasonSummary(job: JobListItem): string | null {
  const summary = job.resultReason?.summary?.trim()
  if (!summary) return null

  const redundantSummaries = new Set([
    statusLabel(job.status),
    job.type === 'update' ? '更新完成' : job.type === 'rollback' ? '回滚完成' : '',
    job.status === 'failed' ? '任务执行失败' : '',
  ])
  return redundantSummaries.has(summary) ? null : summary
}

export function ServiceOperationHistory(props: {
  backupRecords: ServiceBackupRecordItem[]
  jobs: JobListItem[]
  serviceId: string
  rollbackSourceJobId?: string | null
  rollbackBusy?: boolean
  onRollback?: () => void
  page: number
  hasPrevious: boolean
  hasNext: boolean
  paginationDisabled?: boolean
  onPrevious: () => void
  onNext: () => void
}) {
  const backupSummaryByJobId = useMemo(() => summarizeServiceOperationBackups(props.backupRecords), [props.backupRecords])

  return (
    <section className="serviceOperationHistory" data-service-detail-section-card="update-history">
      <div className="serviceOperationHistoryTable" role="table" aria-label="服务操作记录">
        <div className="serviceOperationHistoryHeader" role="row">
          <span role="columnheader">记录</span>
          <span role="columnheader">状态</span>
          <span role="columnheader">备份</span>
          <span role="columnheader">来源</span>
          <span role="columnheader">时间</span>
          <span role="columnheader">操作</span>
        </div>
        {props.jobs.map((job) => {
          const reason = resultReasonSummary(job)
          const releaseVersion = releaseVersionForServiceOperation(job, props.serviceId)
          const backupSummary = backupSummaryByJobId.get(job.id) ?? { state: 'empty' as const }
          const jobStatusTone = statusTone(job.status)
          const jobStatusLabel = statusLabel(job.status)
          const canRollback =
            job.id === props.rollbackSourceJobId && job.type === 'update' && job.status === 'success' && Boolean(props.onRollback)
          return (
            <div
              className={job.status === 'failed' ? 'serviceOperationHistoryRow serviceOperationHistoryRowFailed' : 'serviceOperationHistoryRow'}
              key={job.id}
              role="button"
              tabIndex={0}
              onClick={() => navigate({ name: 'job', jobId: job.id })}
              onKeyDown={(event) => {
                const target = event.target as HTMLElement | null
                if (target && target !== event.currentTarget) return
                if (event.key === 'Enter' || event.key === ' ') {
                  event.preventDefault()
                  navigate({ name: 'job', jobId: job.id })
                }
              }}
            >
              <div className="serviceOperationHistoryOperation" data-label="操作">
                <div className="serviceOperationHistoryOperationHeader">
                  <div className="serviceOperationHistoryOperationSummary">
                    <div className="serviceOperationHistoryOperationTitle">{operationLabel(job)}</div>
                    {reason ? (
                      <span className="serviceOperationHistoryReason" title={job.resultReason?.detail ?? reason}>
                        {reason}
                      </span>
                    ) : null}
                  </div>
                  <div className="serviceOperationHistoryMobileStatus" data-label="状态">
                    <Pill tone={jobStatusTone}>{jobStatusLabel}</Pill>
                  </div>
                </div>
                <Mono>{job.id}</Mono>
              </div>
              <div className="serviceOperationHistoryStatus" data-label="状态">
                <Pill tone={jobStatusTone}>{jobStatusLabel}</Pill>
              </div>
              <div className="serviceOperationHistoryBackup" data-backup-state={backupSummary.state} data-label="备份">
                {backupSummary.state === 'empty' ? (
                  <>
                    <span className="serviceOperationHistoryBackupPlaceholder">--</span>
                    <span className="serviceOperationHistoryBackupPlaceholder">--</span>
                  </>
                ) : backupSummary.state === 'partial' ? (
                  <>
                    <span>{backupTargetCountLabel(backupSummary.targetCount)}</span>
                    <span className="serviceOperationHistoryBackupPlaceholder">--</span>
                  </>
                ) : (
                  <>
                    <span>{backupTargetCountLabel(backupSummary.targetCount)}</span>
                    <span className="serviceOperationHistoryBackupSize">{backupSummary.sizeLabel}</span>
                  </>
                )}
              </div>
              <div className="serviceOperationHistorySource" data-label="来源">
                <span>{reasonLabel(job.reason)}</span>
                <span className="muted">by {job.createdBy}</span>
              </div>
              <div className="serviceOperationHistoryTime" data-label="时间">
                {formatCompactDateTime(job.finishedAt ?? job.startedAt ?? job.createdAt)}
              </div>
              <div
                className="serviceOperationHistoryAction"
                data-label="操作"
                onClick={(event) => event.stopPropagation()}
              >
                {releaseVersion ? (
                  <span data-release-version={releaseVersion} data-service-operation-action="release-notes">
                    <IconButton
                      hint={`查看 ${releaseVersion} 的更新日志`}
                      onClick={() => openGitHubReleaseDrawer({ serviceId: props.serviceId, version: releaseVersion })}
                      title={`查看 ${releaseVersion} 的更新日志`}
                    >
                      <ScrollText aria-hidden="true" size={15} strokeWidth={2} />
                    </IconButton>
                  </span>
                ) : null}
                {canRollback ? (
                  <button
                    className="serviceOperationHistoryRollbackButton"
                    data-service-operation-action="rollback"
                    disabled={props.rollbackBusy}
                    onClick={() => props.onRollback?.()}
                    type="button"
                  >
                    <RotateCcw aria-hidden="true" size={14} strokeWidth={2} />
                    回滚
                  </button>
                ) : null}
              </div>
            </div>
          )
        })}
        {props.jobs.length === 0 ? <div className="serviceOperationHistoryEmpty">当前服务暂无操作记录。</div> : null}
      </div>
      {props.hasPrevious || props.hasNext ? (
        <nav className="serviceOperationHistoryPager" aria-label="更新记录分页">
          <span className="serviceOperationHistoryPagerStatus" aria-live="polite">
            第 {props.page} 页，每页 {SERVICE_OPERATION_HISTORY_PAGE_SIZE} 条
          </span>
          <div className="serviceOperationHistoryPagerActions">
            <IconButton
              disabled={!props.hasPrevious || props.paginationDisabled}
              hint="上一页"
              onClick={props.onPrevious}
              title="上一页"
            >
              <ChevronLeft aria-hidden="true" size={16} strokeWidth={2} />
            </IconButton>
            <IconButton
              disabled={!props.hasNext || props.paginationDisabled}
              hint="下一页"
              onClick={props.onNext}
              title="下一页"
            >
              <ChevronRight aria-hidden="true" size={16} strokeWidth={2} />
            </IconButton>
          </div>
        </nav>
      ) : null}
    </section>
  )
}

export function RecentUpdateRecords(props: { jobs: JobListItem[] }) {
  return (
    <div className="card recentUpdatesCard">
      <div className="recentUpdatesHead">
        <div>
          <div className="title">最近更新记录</div>
          <div className="muted">只显示最近三次更新任务。</div>
        </div>
        <Pill tone={props.jobs.length > 0 ? 'info' : 'muted'}>{props.jobs.length}/3</Pill>
      </div>
      <div className="recentUpdatesList">
        {props.jobs.map((job) => {
          const hasResultReason = Boolean(job.resultReason?.summary?.trim() && (job.resultReason?.detail?.trim() || job.resultReason?.summary?.trim()))
          return (
            <div className="recentUpdateRow" key={job.id}>
              <button
                className="recentUpdateLink"
                data-recent-update-job-id={job.id}
                onClick={() => navigate({ name: 'job', jobId: job.id })}
                type="button"
              >
                <div className="recentUpdateCopy">
                  <div className="recentUpdateTitle">
                    <Mono>{job.id}</Mono>
                    <Pill tone={statusTone(job.status)}>{job.status}</Pill>
                  </div>
                  <div className="muted recentUpdateMeta">
                    {reasonLabel(job.reason)} · by <Mono>{job.createdBy}</Mono>
                  </div>
                </div>
                <div className="recentUpdateTime">{formatCompactDateTime(job.finishedAt ?? job.startedAt ?? job.createdAt)}</div>
              </button>
              {hasResultReason ? (
                <div className="recentUpdateReasonSlot">
                  <TaskResultReason reason={job.resultReason} lines={1} className="recentUpdateReason" />
                </div>
              ) : null}
            </div>
          )
        })}
        {props.jobs.length === 0 ? <div className="muted">暂无更新记录</div> : null}
      </div>
    </div>
  )
}
