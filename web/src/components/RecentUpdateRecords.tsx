import type { JobListItem, StackDetail } from '../api'
import { navigate } from '../routes'
import { Mono, Pill } from '../ui'
import { TaskResultReason } from './TaskResultReason'

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

export function selectRecentServiceUpdateJobs(jobs: JobListItem[], serviceId: string): JobListItem[] {
  return selectServiceOperationJobs(jobs, serviceId)
    .filter((job) => job.type === 'update')
    .sort((a, b) => jobSortTime(b) - jobSortTime(a))
    .slice(0, 3)
}

export function selectServiceOperationJobs(jobs: JobListItem[], serviceId: string, stackId?: string): JobListItem[] {
  return jobs
    .filter((job) => {
      if (job.type !== 'update' && job.type !== 'rollback') return false
      if (job.serviceId === serviceId) return true
      if (stackId && job.scope === 'stack' && job.stackId === stackId) return true
      return serviceIdsFromSummary(job.summary).has(serviceId)
    })
    .sort((a, b) => jobSortTime(b) - jobSortTime(a))
}

export function selectRecentStackUpdateJobs(jobs: JobListItem[], stack: StackDetail): JobListItem[] {
  const stackServiceIds = new Set(stack.services.map((service) => service.id))
  return jobs
    .filter((job) => {
      if (job.type !== 'update') return false
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
  if (status === 'success' || status === 'rolled_back') return 'ok'
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

function operationLabel(type: string): string {
  return type === 'rollback' ? '回滚' : '更新'
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
  return summary || null
}

export function ServiceOperationHistory(props: { jobs: JobListItem[] }) {
  return (
    <section className="card serviceOperationHistory" data-service-detail-section-card="update-history">
      <div className="serviceOperationHistoryHead">
        <div>
          <div className="title">更新记录</div>
          <div className="muted">显示当前服务的更新与回滚任务，按最新时间排序。</div>
        </div>
        <Pill tone={props.jobs.length > 0 ? 'info' : 'muted'}>{props.jobs.length}</Pill>
      </div>
      <div className="serviceOperationHistoryTable" role="table" aria-label="更新和回滚记录">
        <div className="serviceOperationHistoryHeader" role="row">
          <span role="columnheader">操作</span>
          <span role="columnheader">状态</span>
          <span role="columnheader">来源</span>
          <span role="columnheader">时间</span>
        </div>
        {props.jobs.map((job) => {
          const reason = resultReasonSummary(job)
          return (
            <div
              className="serviceOperationHistoryRow"
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
                <div className="serviceOperationHistoryOperationTitle">{operationLabel(job.type)}</div>
                <Mono>{job.id}</Mono>
                {reason ? (
                  <span className="serviceOperationHistoryReason" title={job.resultReason?.detail ?? reason}>
                    {reason}
                  </span>
                ) : null}
              </div>
              <div className="serviceOperationHistoryStatus" data-label="状态">
                <Pill tone={statusTone(job.status)}>{statusLabel(job.status)}</Pill>
              </div>
              <div className="serviceOperationHistorySource" data-label="来源">
                <span>{reasonLabel(job.reason)}</span>
                <span className="muted">by {job.createdBy}</span>
              </div>
              <div className="serviceOperationHistoryTime" data-label="时间">
                {formatCompactDateTime(job.finishedAt ?? job.startedAt ?? job.createdAt)}
              </div>
            </div>
          )
        })}
        {props.jobs.length === 0 ? <div className="serviceOperationHistoryEmpty">当前服务暂无更新或回滚记录。</div> : null}
      </div>
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
        {props.jobs.map((job) => (
          <div className="recentUpdateRow" key={job.id}>
            <div>
              <div className="recentUpdateTitle">
                <Mono>{job.id}</Mono>
                <Pill tone={statusTone(job.status)}>{job.status}</Pill>
              </div>
              <div className="muted">
                {reasonLabel(job.reason)} · by <Mono>{job.createdBy}</Mono>
              </div>
              <TaskResultReason reason={job.resultReason} lines={1} className="recentUpdateReason" />
            </div>
            <div className="recentUpdateTime">{formatCompactDateTime(job.finishedAt ?? job.startedAt ?? job.createdAt)}</div>
          </div>
        ))}
        {props.jobs.length === 0 ? <div className="muted">暂无更新记录</div> : null}
      </div>
    </div>
  )
}
