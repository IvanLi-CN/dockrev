import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { Activity, ArrowRight, CheckCircle2, CircleDot, Filter, Layers3, ListChecks, Webhook } from 'lucide-react'
import { listCompactJobs, type CleanupResourceKind, type CompactJobListItem } from '../api'
import { currentHref, navigate, type Route, type SettingsSection } from '../routes'
import { Button, Mono } from '../ui'
import { SETTINGS_DESTINATIONS } from './SettingsMobileNavigation'

type ContextSectionProps = {
  title: string
  icon?: ReactNode
  children: ReactNode
}

function ContextSection(props: ContextSectionProps) {
  return (
    <section className="pageContextSection">
      <div className="pageContextSectionTitle">
        {props.icon ? <span className="pageContextSectionIcon" aria-hidden="true">{props.icon}</span> : null}
        <span>{props.title}</span>
      </div>
      {props.children}
    </section>
  )
}

function ContextLink(props: {
  label: string
  to?: Route
  active?: boolean
  count?: number
  meta?: string
  onClick?: () => void
}) {
  const content = (
    <>
      <span className="pageContextLinkLabel">{props.label}</span>
      {props.meta ? <span className="pageContextLinkMeta">{props.meta}</span> : null}
      {props.count != null ? <span className="pageContextLinkCount"><Mono>{props.count}</Mono></span> : null}
      {props.to ? <ArrowRight className="pageContextLinkArrow" size={14} aria-hidden="true" /> : null}
    </>
  )
  if (!props.to) {
    return (
      <button type="button" className={props.active ? 'pageContextLink pageContextLinkActive' : 'pageContextLink'} aria-current={props.active ? 'location' : undefined} onClick={props.onClick}>
        {content}
      </button>
    )
  }
  return (
    <a
      href={currentHref(props.to)}
      className={props.active ? 'pageContextLink pageContextLinkActive' : 'pageContextLink'}
      aria-current={props.active ? 'page' : undefined}
      onClick={(event) => {
        event.preventDefault()
        navigate(props.to as Route)
      }}
    >
      {content}
    </a>
  )
}

export type OverviewContextGroup = { name: string; count: number; active?: boolean }

export function OverviewContextNavigation(props: {
  groups: OverviewContextGroup[]
  onSelectGroup: (name: string) => void
}) {
  return (
    <ContextSection title="页面分组" icon={<Layers3 size={15} />}>
      {props.groups.length === 0 ? <div className="pageContextEmpty">暂无分组</div> : null}
      {props.groups.map((group) => (
        <ContextLink key={group.name} label={group.name} count={group.count} active={group.active} onClick={() => props.onSelectGroup(group.name)} />
      ))}
    </ContextSection>
  )
}

const TERMINAL_JOB_STATUSES = new Set(['success', 'failed', 'rolled_back', 'cancelled'])

export function queueContextJobBuckets(jobs: CompactJobListItem[]): {
  active: CompactJobListItem[]
  recent: CompactJobListItem[]
} {
  return {
    active: jobs.filter((job) => job.status === 'queued' || job.status === 'running').slice(0, 8),
    recent: jobs
      .filter((job) => TERMINAL_JOB_STATUSES.has(job.status))
      .slice()
      .sort((left, right) => Date.parse(right.finishedAt ?? right.createdAt) - Date.parse(left.finishedAt ?? left.createdAt))
      .slice(0, 5),
  }
}

function jobLabel(job: CompactJobListItem): string {
  return job.displayLabel || job.type || `任务 ${job.id.slice(0, 8)}`
}

export function QueueContextNavigation() {
  const [jobs, setJobs] = useState<CompactJobListItem[]>([])

  useEffect(() => {
    let cancelled = false
    void listCompactJobs({ limit: 100 }).then((next) => {
      if (!cancelled) setJobs(next)
    }).catch(() => {
      if (!cancelled) setJobs([])
    })
    return () => { cancelled = true }
  }, [])

  const { active: activeJobs, recent: recentJobs } = useMemo(() => queueContextJobBuckets(jobs), [jobs])

  return (
    <>
      <ContextSection title="队列工具" icon={<ListChecks size={15} />}>
        <ContextLink label="版本推测" to={{ name: 'version-inference' }} />
        <ContextLink label="GHCR Webhook" to={{ name: 'ghcr-webhooks' }} />
      </ContextSection>
      <ContextSection title="活动任务" icon={<Activity size={15} />}>
        {activeJobs.length === 0 ? <div className="pageContextEmpty">暂无活动任务</div> : null}
        {activeJobs.map((job) => <ContextLink key={job.id} label={jobLabel(job)} meta={job.status === 'running' ? '运行中' : '排队中'} to={{ name: 'job', jobId: job.id }} />)}
      </ContextSection>
      <ContextSection title="最近完成" icon={<CheckCircle2 size={15} />}>
        {recentJobs.length === 0 ? <div className="pageContextEmpty">暂无终态任务</div> : null}
        {recentJobs.map((job) => <ContextLink key={job.id} label={jobLabel(job)} meta={job.status} to={{ name: 'job', jobId: job.id }} />)}
      </ContextSection>
    </>
  )
}

export function CleanupContextNavigation(props: {
  scope: string
  onScopeChange: (scope: string) => void
  resourceKinds: CleanupResourceKind[]
  availableResourceKinds: Array<{ key: CleanupResourceKind; label: string }>
  onResourceKindsChange: (kinds: CleanupResourceKind[]) => void
}) {
  return (
    <>
      <ContextSection title="清理范围" icon={<Filter size={15} />}>
        {['all', 'stack', 'service'].map((scope) => (
          <ContextLink key={scope} label={scope === 'all' ? '全部范围' : scope === 'stack' ? 'Stack' : 'Service'} active={props.scope === scope} onClick={() => props.onScopeChange(scope)} />
        ))}
      </ContextSection>
      <ContextSection title="资源类型">
        {props.availableResourceKinds.map((kind) => {
          const active = props.resourceKinds.includes(kind.key)
          return <ContextLink key={kind.key} label={kind.label} active={active} onClick={() => props.onResourceKindsChange(active ? props.resourceKinds.filter((value) => value !== kind.key) : [...props.resourceKinds, kind.key])} />
        })}
        <Button className="pageContextReset" variant="ghost" onClick={() => props.onResourceKindsChange([])}>清除筛选</Button>
      </ContextSection>
    </>
  )
}

export function SettingsContextNavigation(props: { section?: SettingsSection }) {
  const selectSection = (section: SettingsSection) => {
    if (typeof window !== 'undefined' && window.matchMedia('(max-width: 960px)').matches) {
      navigate({ name: 'settings', section })
      return
    }
    document.querySelector<HTMLElement>(`[data-settings-section="${section}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }
  return (
    <ContextSection title="设置目录" icon={<CircleDot size={15} />}>
      {SETTINGS_DESTINATIONS.map((item) => (
        <ContextLink key={item.section} label={item.title} meta={item.description} active={props.section === item.section} onClick={() => selectSection(item.section)} />
      ))}
      <ContextLink label="设置首页" active={!props.section} onClick={() => navigate({ name: 'settings' })} />
    </ContextSection>
  )
}

export function QueueContextBadge() {
  return <span className="pageContextBadge"><Webhook size={13} aria-hidden="true" /> Queue</span>
}
