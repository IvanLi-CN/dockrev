import refreshIcon from '@iconify-icons/mdi/refresh'
import linkVariant from '@iconify-icons/mdi/link-variant'
import trashCanOutline from '@iconify-icons/mdi/trash-can-outline'
import { Icon } from '@iconify/react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  deleteGitHubPackagesRepo,
  getGitHubPackagesWebhookOverview,
  listGitHubPackagesRepos,
  listJobsPage,
  triggerGitHubPackagesWebhookSyncAll,
  triggerGitHubPackagesWebhookSyncRepo,
  type GitHubPackagesWebhookOverviewResponse,
  type JobListItem,
  type ListGitHubPackagesReposResponse,
} from '../api'
import { useConfirm } from '../confirm'
import { useManagementEventBatch } from '../managementEvents'
import { navigate } from '../routes'
import { Button, Chip, Input, Mono, Pill, ResponsiveActionButton, SelectField } from '../ui'
import { AsyncDataRegion, AsyncDataSkeleton } from '../components/AsyncDataRegion'
import type { AsyncDataPhase, AsyncDataSource } from '../asyncData'
import { webhookStateDotClass, webhookStateIcon } from '../webhookStatus'

type RepoStateFilter = 'all' | 'ok' | 'missing' | 'error' | 'conflict' | 'queued' | 'running' | 'unknown'

type RepoQuery = {
  filter: RepoStateFilter
  query: string
  page: number
  perPage: number
}

const REPO_PER_PAGE_OPTIONS = [25, 50, 100] as const
const DEFAULT_REPO_PER_PAGE = 50
const ACTIVE_JOBS_PER_STATUS = 200
const GHCR_JOB_TYPES: string[] = [
  'github_packages_webhook',
  'github_packages_webhook_sync_all',
  'github_packages_webhook_sync_repo',
]
const EMPTY_REPOS: ListGitHubPackagesReposResponse['repos'] = []

function normalizeRepoKey(fullName: string): string {
  return fullName.trim().toLowerCase()
}

function parseRepoFullName(fullName: string): { owner: string; repo: string } | null {
  const input = fullName.trim()
  const slash = input.indexOf('/')
  if (slash <= 0 || slash !== input.lastIndexOf('/')) return null
  const owner = input.slice(0, slash).trim()
  const repo = input.slice(slash + 1).trim()
  if (!owner || !repo) return null
  return { owner, repo }
}

function buildRepoWebUrl(fullName: string): string | null {
  const parsed = parseRepoFullName(fullName)
  if (!parsed) return null
  return `https://github.com/${parsed.owner}/${parsed.repo}`
}

function buildRepoWebhookWebUrl(fullName: string, hookId?: number | null): string | null {
  const parsed = parseRepoFullName(fullName)
  if (!parsed) return null
  const base = `https://github.com/${parsed.owner}/${parsed.repo}/settings/hooks`
  if (hookId == null) return base
  return `${base}/${hookId}`
}

function readJobRepoTargets(job: JobListItem): string[] {
  const summary = job.summary
  if (!summary || typeof summary !== 'object') return []
  const repos = (summary as { repos?: unknown }).repos
  if (!Array.isArray(repos)) return []
  return repos.filter((v): v is string => typeof v === 'string').map((v) => v.trim()).filter((v) => v.length > 0)
}

function readJobOp(job: JobListItem): string {
  const summary = job.summary
  if (!summary || typeof summary !== 'object') return ''
  const op = (summary as { op?: unknown }).op
  return typeof op === 'string' ? op : ''
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function normalizeWebhookState(raw: string | null | undefined): string {
  const state = (raw ?? '').trim().toLowerCase()
  return state || 'unknown'
}

function webhookStateLabel(state: string): string {
  if (state === 'queued') return '排队中'
  if (state === 'running') return '运行中'
  if (state === 'ok') return '已注册'
  if (state === 'missing') return '缺失'
  if (state === 'error') return '失败'
  if (state === 'conflict') return '冲突'
  return '未知'
}

function webhookStateTone(state: string): 'ok' | 'warn' | 'bad' | 'muted' {
  if (state === 'ok') return 'ok'
  if (state === 'queued' || state === 'running' || state === 'missing') return 'warn'
  if (state === 'error' || state === 'conflict') return 'bad'
  return 'muted'
}

function formatShort(ts?: string | null): string {
  if (!ts) return '-'
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  return d.toLocaleString()
}

async function listActiveGhcrJobs(): Promise<JobListItem[]> {
  const pages = await Promise.all(
    ['queued', 'running'].map((status) =>
      listJobsPage({ type: GHCR_JOB_TYPES, status, limit: ACTIVE_JOBS_PER_STATUS }),
    ),
  )
  return pages.flatMap((page) => page.jobs)
}

export function GhcrWebhookRegistryPage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const confirm = useConfirm()
  const [overview, setOverview] = useState<GitHubPackagesWebhookOverviewResponse | null>(null)
  const [repoPage, setRepoPage] = useState<ListGitHubPackagesReposResponse | null>(null)
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [filter, setFilter] = useState<RepoStateFilter>('all')
  const [useFilterDropdown, setUseFilterDropdown] = useState(false)
  const [queryInput, setQueryInput] = useState('')
  const [query, setQuery] = useState('')
  const [page, setPage] = useState(1)
  const [perPage, setPerPage] = useState<number>(DEFAULT_REPO_PER_PAGE)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [phase, setPhase] = useState<AsyncDataPhase>('initial-loading')
  const [source, setSource] = useState<AsyncDataSource>('none')
  const refreshRequestIdRef = useRef(0)
  const hasCommittedDataRef = useRef(false)
  const committedQueryRef = useRef<RepoQuery>({ filter, query, page, perPage })
  const filterRowRef = useRef<HTMLDivElement | null>(null)
  committedQueryRef.current = { filter, query, page, perPage }

  const refresh = useCallback(async (opts?: AsyncDataSource | { source?: AsyncDataSource; query?: RepoQuery }): Promise<void> => {
    const requestId = ++refreshRequestIdRef.current
    const requestedQuery = typeof opts === 'string' ? committedQueryRef.current : (opts?.query ?? committedQueryRef.current)
    const requestedSource = typeof opts === 'string' ? opts : (opts?.source ?? 'live')
    setSource(requestedSource)
    setPhase(hasCommittedDataRef.current ? 'refreshing' : 'initial-loading')
    setError(null)
    try {
      const [nextOverview, initialRepoPage, activeJobs] = await Promise.all([
        getGitHubPackagesWebhookOverview(),
        listGitHubPackagesRepos({
          page: requestedQuery.page,
          perPage: requestedQuery.perPage,
          q: requestedQuery.query,
          selectedFilter: 'selected',
          webhookState: requestedQuery.filter,
        }),
        listActiveGhcrJobs(),
      ])
      if (requestId !== refreshRequestIdRef.current) return
      let nextRepoPage = initialRepoPage
      const maxPage = Math.max(1, Math.ceil(nextRepoPage.filteredTotal / nextRepoPage.perPage))
      if (nextRepoPage.page > maxPage) {
        nextRepoPage = await listGitHubPackagesRepos({
          page: maxPage,
          perPage: requestedQuery.perPage,
          q: requestedQuery.query,
          selectedFilter: 'selected',
          webhookState: requestedQuery.filter,
        })
        if (requestId !== refreshRequestIdRef.current) return
      }
      setOverview(nextOverview)
      setRepoPage(nextRepoPage)
      setJobs(activeJobs)
      setFilter(requestedQuery.filter)
      setQuery(requestedQuery.query)
      setPage(nextRepoPage.page)
      setPerPage(requestedQuery.perPage)
      hasCommittedDataRef.current = true
      setPhase(nextRepoPage.repos.length === 0 ? 'ready-empty' : 'ready-data')
    } catch (e: unknown) {
      if (requestId !== refreshRequestIdRef.current) return
      setError(errorMessage(e))
      setPhase('error')
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  useManagementEventBatch(({ events, resyncRequired }) => {
    if (!resyncRequired && !events.some((event) =>
      event.domain === 'github_packages' || (typeof event.summary.jobType === 'string' && event.summary.jobType.startsWith('github_packages')),
    )) return
    void refresh().catch((error: unknown) => setError(errorMessage(error)))
  })

  useEffect(() => {
    onTopActions(
      <Button
        variant="ghost"
        disabled={busy}
        onClick={() => {
          void (async () => {
            setBusy(true)
            setError(null)
            try {
              await refresh()
            } catch (e: unknown) {
              setError(errorMessage(e))
            } finally {
              setBusy(false)
            }
          })()
        }}
      >
        刷新
      </Button>,
    )
  }, [busy, onTopActions, refresh])

  const repos = repoPage?.repos ?? EMPTY_REPOS

  const summary = useMemo(() => {
    const fallback = {
      tracked: repos.length,
      ok: 0,
      missing: 0,
      error: 0,
      conflict: 0,
      queued: 0,
      running: 0,
      unknown: 0,
    }
    for (const repo of repos) {
      const state = normalizeWebhookState(repo.webhookState)
      if (state === 'ok') fallback.ok += 1
      else if (state === 'missing') fallback.missing += 1
      else if (state === 'error') fallback.error += 1
      else if (state === 'conflict') fallback.conflict += 1
      else if (state === 'queued') fallback.queued += 1
      else if (state === 'running') fallback.running += 1
      else fallback.unknown += 1
    }
    return overview?.summary ?? fallback
  }, [overview?.summary, repos])

  const maxPage = useMemo(
    () => Math.max(1, Math.ceil((repoPage?.filteredTotal ?? 0) / Math.max(1, repoPage?.perPage ?? perPage))),
    [perPage, repoPage?.filteredTotal, repoPage?.perPage],
  )
  const currentPage = page
  const dataBusy = phase === 'initial-loading' || phase === 'refreshing'

  const runningJob = useMemo(() => jobs.find((job) => job.status === 'running') ?? null, [jobs])

  const activeSyncAllJob = useMemo(() => {
    const running = jobs.find((job) => job.type === 'github_packages_webhook_sync_all' && job.status === 'running')
    if (running) return running
    return jobs.find((job) => job.type === 'github_packages_webhook_sync_all' && job.status === 'queued') ?? null
  }, [jobs])

  const activeSyncRepoJobs = useMemo(() => {
    const map = new Map<string, JobListItem>()
    const score = (job: JobListItem) => {
      const runningBoost = job.status === 'running' ? 1_000_000_000_000_000 : 0
      return runningBoost + Date.parse(job.createdAt || '')
    }
    for (const job of jobs) {
      if (job.type !== 'github_packages_webhook_sync_repo') continue
      if (job.status !== 'queued' && job.status !== 'running') continue
      const targets = readJobRepoTargets(job)
      const key = normalizeRepoKey(targets[0] ?? '')
      if (!key) continue
      const existing = map.get(key)
      if (!existing || score(job) > score(existing)) {
        map.set(key, job)
      }
    }
    return map
  }, [jobs])

  const activeLegacyRegisterJobs = useMemo(() => {
    const map = new Map<string, JobListItem>()
    for (const job of jobs) {
      if (job.type !== 'github_packages_webhook') continue
      if (job.status !== 'queued' && job.status !== 'running') continue
      if (readJobOp(job) !== 'register') continue
      const targets = readJobRepoTargets(job)
      const key = normalizeRepoKey(targets[0] ?? '')
      if (!key || map.has(key)) continue
      map.set(key, job)
    }
    return map
  }, [jobs])

  const filterItems = useMemo(
    () => [
      { key: 'all', label: '全部', count: summary.tracked },
      { key: 'ok', label: '已注册', count: summary.ok },
      { key: 'missing', label: '缺失', count: summary.missing },
      { key: 'error', label: '失败', count: summary.error },
      { key: 'conflict', label: '冲突', count: summary.conflict },
      { key: 'queued', label: '排队中', count: summary.queued },
      { key: 'running', label: '运行中', count: summary.running },
      { key: 'unknown', label: '未知', count: summary.unknown },
    ],
    [summary.conflict, summary.error, summary.missing, summary.ok, summary.queued, summary.running, summary.tracked, summary.unknown],
  )

  const updateFilter = useCallback((nextFilter: RepoStateFilter) => {
    void refresh({ source: 'memory', query: { ...committedQueryRef.current, filter: nextFilter, page: 1 } })
  }, [refresh])

  const submitSearch = useCallback(() => {
    void refresh({ source: 'memory', query: { ...committedQueryRef.current, query: queryInput.trim(), page: 1 } })
  }, [queryInput, refresh])

  const clearSearch = useCallback(() => {
    setQueryInput('')
    void refresh({ source: 'memory', query: { ...committedQueryRef.current, query: '', page: 1 } })
  }, [refresh])

  const goToPage = useCallback((nextPage: number) => {
    void refresh({ source: 'memory', query: { ...committedQueryRef.current, page: Math.max(1, nextPage) } })
  }, [refresh])

  useEffect(() => {
    if (typeof window === 'undefined') return undefined
    const row = filterRowRef.current
    if (!row) return undefined

    let rafId = 0
    const measure = () => {
      const chips = Array.from(row.children) as HTMLElement[]
      if (chips.length <= 1) {
        setUseFilterDropdown(false)
        return
      }
      const firstTop = chips[0].offsetTop
      const wrapped = chips.some((chip) => Math.abs(chip.offsetTop - firstTop) > 1)
      setUseFilterDropdown(wrapped)
    }
    const scheduleMeasure = () => {
      window.cancelAnimationFrame(rafId)
      rafId = window.requestAnimationFrame(measure)
    }

    scheduleMeasure()
    const resizeObserver = new ResizeObserver(scheduleMeasure)
    resizeObserver.observe(row)
    const toolbar = row.closest('.ghcrRegistryToolbar')
    if (toolbar) resizeObserver.observe(toolbar)
    window.addEventListener('resize', scheduleMeasure)

    return () => {
      window.cancelAnimationFrame(rafId)
      resizeObserver.disconnect()
      window.removeEventListener('resize', scheduleMeasure)
    }
  }, [filterItems])

  return (
    <AsyncDataRegion
      className="page ghcrRegistryPage"
      error={error}
      hasData={hasCommittedDataRef.current}
      label="正在刷新 GHCR Webhook 维护数据"
      onRetry={() => void refresh('memory')}
      phase={phase}
      skeleton={<AsyncDataSkeleton className="ghcrRegistryLoadingSkeleton" lines={10} />}
      source={source}
    >
      <div className="ghcrRegistrySummaryGrid">
        <div className="ghcrRegistrySummaryItem">
          <div className="muted">Tracked Repos</div>
          <div className="ghcrRegistrySummaryValue">
            <Mono>{phase === 'initial-loading' ? '—' : summary.tracked}</Mono>
          </div>
        </div>
        <div className="ghcrRegistrySummaryItem">
          <div className="muted">Webhook 状态</div>
          <div className="ghcrRegistrySummaryValue">
            <Mono>
              {phase === 'initial-loading' ? '正在加载…' : `ok ${summary.ok} · missing ${summary.missing} · error ${summary.error} · conflict ${summary.conflict}`}
            </Mono>
          </div>
        </div>
        <div className="ghcrRegistrySummaryItem">
          <div className="muted">Job 状态</div>
          <div className="ghcrRegistrySummaryValue">
            <Mono>
              {phase === 'initial-loading' ? '正在加载…' : `queued ${overview?.jobsQueued ?? '—'} · running ${overview?.jobsRunning ?? '—'}`}
            </Mono>
          </div>
        </div>
        <div className="ghcrRegistrySummaryItem">
          <div className="muted">Last Audit</div>
          <div className="ghcrRegistrySummaryValue">
            <Mono>{formatShort(overview?.lastAuditAt)}</Mono>
          </div>
        </div>
      </div>

      <div className="ghcrRegistryToolbar">
        <div
          ref={filterRowRef}
          className={useFilterDropdown ? 'chipRow ghcrRegistryFilterMeasureRow' : 'chipRow'}
          aria-hidden={useFilterDropdown}
        >
          {filterItems.map((it) => (
            <Chip
              key={it.key}
              active={filter === it.key}
              disabled={dataBusy || busy}
              onClick={() => updateFilter(it.key as RepoStateFilter)}
              title={`${it.label}: ${it.count}`}
            >
              <span>{it.label}</span>
              <span className="chipCount">{it.count}</span>
            </Chip>
          ))}
        </div>

        {useFilterDropdown ? (
          <div className="ghcrRegistryFilterSelectWrap">
            <label className="ghcrRegistryFilterSelectLabel" htmlFor="ghcr-registry-filter-select">
              状态筛选
            </label>
            <SelectField
              className="select ghcrRegistryFilterSelect"
              disabled={dataBusy || busy}
              id="ghcr-registry-filter-select"
              onChange={(value) => updateFilter(value as RepoStateFilter)}
              options={filterItems.map((item) => ({
                value: item.key,
                label: `${item.label} (${item.count})`,
              }))}
              value={filter}
            />
          </div>
        ) : null}

        <div className="ghcrRegistrySearchForm">
          <Input
            className="input"
            disabled={dataBusy || busy}
            onChange={(event) => setQueryInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== 'Enter') return
              event.preventDefault()
              submitSearch()
            }}
            placeholder="搜索 owner/repo、状态、hookId、错误信息"
            value={queryInput}
          />
          <Button variant="ghost" disabled={dataBusy || busy} onClick={submitSearch}>
            搜索
          </Button>
          <Button
            variant="ghost"
            disabled={dataBusy || busy || (!query && !queryInput)}
            onClick={clearSearch}
          >
            清除
          </Button>
        </div>

        <div className="chipRow" style={{ marginLeft: 'auto' }}>
          <ResponsiveActionButton
            variant="ghost"
            disabled={dataBusy || (busy && !activeSyncAllJob)}
            label={
              activeSyncAllJob?.status === 'running'
                ? '全量同步中…'
                : activeSyncAllJob?.status === 'queued'
                  ? '全量同步排队中…'
                  : '全部状态同步'
            }
            hint={
              activeSyncAllJob
                ? '任务进行中，点击查看任务详情'
                : '触发全部已跟踪仓库的 webhook 状态同步任务'
            }
            icon={
              <Icon
                icon={refreshIcon}
                className={`ghcrSyncAllBtnIcon ${
                  activeSyncAllJob?.status === 'running' || activeSyncAllJob?.status === 'queued'
                    ? 'ghcrSyncAllBtnIconSpinning'
                    : ''
                }`}
                aria-hidden="true"
              />
            }
            onClick={() => {
              void (async () => {
                if (activeSyncAllJob) {
                  navigate({ name: 'job', jobId: activeSyncAllJob.id })
                  return
                }
                setBusy(true)
                setError(null)
                try {
                  await triggerGitHubPackagesWebhookSyncAll()
                  await refresh('memory')
                } catch (e: unknown) {
                  setError(errorMessage(e))
                } finally {
                  setBusy(false)
                }
              })()
            }}
          />
        </div>
      </div>

      {runningJob ? (
        <div className="muted">
          当前运行任务：<Mono>{runningJob.id}</Mono>
          {runningJob.progress?.message ? ` · ${runningJob.progress.message}` : ''}
        </div>
      ) : null}

      <div className="ghcrRegistryPager" aria-label="仓库分页">
        <span className="muted">
          {phase === 'initial-loading' ? '正在加载仓库…' : repoPage ? `第 ${currentPage} / ${maxPage} 页，共 ${repoPage.filteredTotal} 个仓库` : '暂无已提交页面'}
        </span>
        <label className="ghcrRegistryPerPage">
          <span className="muted">每页</span>
          <SelectField
            ariaLabel="每页仓库数量"
            className="select"
            onChange={(value) => {
              const nextPerPage = Number.parseInt(value, 10)
              void refresh({
                source: 'memory',
                query: {
                  ...committedQueryRef.current,
                  page: 1,
                  perPage: Number.isFinite(nextPerPage) && nextPerPage > 0 ? nextPerPage : DEFAULT_REPO_PER_PAGE,
                },
              })
            }}
            options={REPO_PER_PAGE_OPTIONS.map((size) => ({ value: String(size), label: String(size) }))}
            value={String(perPage)}
          />
        </label>
        <Button variant="ghost" disabled={dataBusy || busy || currentPage <= 1} onClick={() => goToPage(currentPage - 1)}>
          上一页
        </Button>
        <Button variant="ghost" disabled={dataBusy || busy || currentPage >= maxPage} onClick={() => goToPage(Math.min(maxPage, currentPage + 1))}>
          下一页
        </Button>
      </div>

      <div className="ghcrRegistryList">
        {phase === 'ready-empty' ? (
          <div className="ghcrRegistryEmpty muted">{query || filter !== 'all' ? '当前筛选条件下无仓库' : '暂无已跟踪仓库'}</div>
        ) : null}

        {repos.map((repo) => {
          const state = normalizeWebhookState(repo.webhookState)
          const dotClass = webhookStateDotClass(state)
          const isInFlight = state === 'queued' || state === 'running'
          const isUnregisterInFlight = isInFlight && (repo.lastOp ?? '') === 'unregister'
          const showRetryDelete = state === 'error' && (repo.lastOp ?? '') === 'unregister'
          const repoSyncJob = activeSyncRepoJobs.get(normalizeRepoKey(repo.fullName)) ?? null
          const repoLegacyRegisterJob = activeLegacyRegisterJobs.get(normalizeRepoKey(repo.fullName)) ?? null
          const repoPendingJob = repoSyncJob ?? repoLegacyRegisterJob
          const repoPending = repoPendingJob?.status === 'queued' || repoPendingJob?.status === 'running'
          const syncBlockedByDelete = isUnregisterInFlight
          const repoWebUrl = buildRepoWebUrl(repo.fullName)
          const repoWebhookWebUrl = buildRepoWebhookWebUrl(repo.fullName, repo.hookId)

          return (
            <div key={repo.fullName} className="ghcrRegistryRow">
              <div className="ghcrRegistryMain">
                <div className="ghcrRegistryHeader">
                  <div className="ghcrRegistryTitle">
                    <Icon icon={webhookStateIcon(state)} className={dotClass} aria-hidden="true" />
                    {repoWebUrl ? (
                      <a
                        className="ghcrRegistryTitleLink"
                        href={repoWebUrl}
                        target="_blank"
                        rel="noopener noreferrer"
                        title={`打开仓库：${repo.fullName}`}
                        aria-label={`打开仓库：${repo.fullName}`}
                      >
                        {repo.fullName}
                      </a>
                    ) : (
                      <span className="ghcrRegistryTitleText">{repo.fullName}</span>
                    )}
                  </div>
                  <div className="ghcrRegistryStatus">
                    <Pill tone={webhookStateTone(state)}>{webhookStateLabel(state)}</Pill>
                  </div>
                </div>
                <div className="ghcrRegistryMeta">
                  <span>
                    hookId <Mono>{repo.hookId != null ? String(repo.hookId) : '-'}</Mono>
                  </span>
                  <span>
                    lastOp <Mono>{repo.lastOp ?? '-'}</Mono>
                  </span>
                  <span>
                    lastSyncAt <Mono>{formatShort(repo.lastSyncAt)}</Mono>
                  </span>
                  <span>
                    lastAuditAt <Mono>{formatShort(repo.lastAuditAt)}</Mono>
                  </span>
                </div>
                {repo.lastError ? (
                  <div className="ghcrRegistryError">
                    lastError <Mono>{repo.lastError}</Mono>
                  </div>
                ) : null}

                <div className="ghcrRegistryActions">
                  <ResponsiveActionButton
                    variant="ghost"
                    disabled={!repoWebhookWebUrl || busy}
                    label="Webhook 页面"
                    hint={repo.hookId != null ? '打开该仓库 webhook 详情页' : '打开该仓库 webhook 列表页'}
                    icon={<Icon icon={linkVariant} aria-hidden="true" />}
                    onClick={() => {
                      if (!repoWebhookWebUrl) return
                      window.open(repoWebhookWebUrl, '_blank', 'noopener,noreferrer')
                    }}
                  />
                  <ResponsiveActionButton
                    variant="ghost"
                    disabled={syncBlockedByDelete || (busy && !repoPending)}
                    label={
                      syncBlockedByDelete
                        ? '删除中…'
                        : repoPendingJob?.status === 'running'
                        ? '同步中…'
                        : repoPendingJob?.status === 'queued'
                          ? '排队中…'
                          : '同步状态'
                    }
                    hint={
                      syncBlockedByDelete
                        ? '反注册删除任务进行中，完成后可再次同步'
                        : repoPending
                          ? '任务进行中，点击查看任务详情'
                          : '触发该仓库 webhook 状态同步任务'
                    }
                    icon={<Icon icon={refreshIcon} aria-hidden="true" />}
                    onClick={() => {
                      void (async () => {
                        if (repoPendingJob) {
                          navigate({ name: 'job', jobId: repoPendingJob.id })
                          return
                        }
                        setBusy(true)
                        setError(null)
                        try {
                          await triggerGitHubPackagesWebhookSyncRepo({ fullName: repo.fullName })
                          await refresh()
                        } catch (e: unknown) {
                          setError(errorMessage(e))
                        } finally {
                          setBusy(false)
                        }
                      })()
                    }}
                  />

                  {showRetryDelete ? (
                    <ResponsiveActionButton
                      variant="ghost"
                      disabled={busy}
                      label="重试删除"
                      hint="重新触发反注册并删除任务"
                      icon={<Icon icon={trashCanOutline} aria-hidden="true" />}
                      onClick={() => {
                        void (async () => {
                          setBusy(true)
                          setError(null)
                          try {
                            await deleteGitHubPackagesRepo({ fullName: repo.fullName })
                            await refresh()
                          } catch (e: unknown) {
                            setError(errorMessage(e))
                          } finally {
                            setBusy(false)
                          }
                        })()
                      }}
                    />
                  ) : null}

                  <ResponsiveActionButton
                    variant="danger"
                    disabled={busy || isUnregisterInFlight}
                    label="删除"
                    hint="先反注册 webhook，成功后移除记录"
                    icon={<Icon icon={trashCanOutline} aria-hidden="true" />}
                    onClick={() => {
                      void (async () => {
                        const pass1 = await confirm({
                          title: '删除跟踪仓库（步骤 1/2）',
                          body: (
                            <div>
                              <div className="modalLead">将为该仓库创建反注册任务，完成后才会真正移除：</div>
                              <div className="modalKvGrid">
                                <div className="modalKvLabel">Repo</div>
                                <div className="modalKvValue">
                                  <Mono>{repo.fullName}</Mono>
                                </div>
                              </div>
                            </div>
                          ),
                          confirmText: '继续',
                          cancelText: '取消',
                          confirmVariant: 'danger',
                          badgeText: '高影响',
                          badgeTone: 'bad',
                        })
                        if (!pass1) return

                        const pass2 = await confirm({
                          title: '最终确认删除（步骤 2/2）',
                          body: (
                            <div>
                              <div className="modalLead">请再次确认删除该仓库 webhook 跟踪：</div>
                              <div className="modalKvGrid">
                                <div className="modalKvLabel">Repo</div>
                                <div className="modalKvValue">
                                  <Mono>{repo.fullName}</Mono>
                                </div>
                                <div className="modalKvLabel">行为</div>
                                <div className="modalKvValue">先反注册 webhook，成功后移除记录</div>
                              </div>
                            </div>
                          ),
                          confirmText: '确认删除',
                          cancelText: '取消',
                          confirmVariant: 'danger',
                          badgeText: '将删除 webhook',
                          badgeTone: 'bad',
                        })
                        if (!pass2) return

                        setBusy(true)
                        setError(null)
                        try {
                          await deleteGitHubPackagesRepo({ fullName: repo.fullName })
                          await refresh()
                        } catch (e: unknown) {
                          setError(errorMessage(e))
                        } finally {
                          setBusy(false)
                        }
                      })()
                    }}
                  />
                </div>
              </div>
            </div>
          )
        })}
      </div>

    </AsyncDataRegion>
  )
}
