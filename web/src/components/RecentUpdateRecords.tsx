import type { JobListItem, StackDetail } from '../api'
import { Mono, Pill } from '../ui'
import { TaskResultReason } from './TaskResultReason'

function formatCompactDateTime(ts?: string | null): string {
  if (!ts) return '-'
  const date = new Date(ts)
  if (Number.isNaN(date.valueOf())) return ts
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  const hour = String(date.getHours()).padStart(2, '0')
  const minute = String(date.getMinutes()).padStart(2, '0')
  return `${month}/${day} ${hour}:${minute}`
}

function jobSortTime(job: JobListItem): number {
  const ts = job.finishedAt ?? job.startedAt ?? job.createdAt
  const time = new Date(ts).valueOf()
  return Number.isNaN(time) ? 0 : time
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value)
}

function serviceIdsFromSummary(summary: unknown): Set<string> {
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
  return jobs
    .filter((job) => {
      if (job.type !== 'update') return false
      if (job.serviceId === serviceId) return true
      return serviceIdsFromSummary(job.summary).has(serviceId)
    })
    .sort((a, b) => jobSortTime(b) - jobSortTime(a))
    .slice(0, 3)
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
