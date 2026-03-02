import refreshIcon from '@iconify-icons/mdi/refresh'
import trashCanOutline from '@iconify-icons/mdi/trash-can-outline'
import { Icon } from '@iconify/react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  deleteGitHubPackagesRepo,
  getGitHubPackagesWebhookOverview,
  listGitHubPackagesRepos,
  listJobs,
  newJobsEventsSource,
  setGitHubPackagesRepoSelected,
  type GitHubPackagesRepo,
  type GitHubPackagesWebhookOverviewResponse,
  type JobListItem,
} from '../api'
import { useConfirm } from '../confirm'
import { navigate } from '../routes'
import { Button, Chip, Mono, Pill, ResponsiveActionButton } from '../ui'
import { webhookStateDotClass, webhookStateIcon } from '../webhookStatus'

type RepoStateFilter = 'all' | 'ok' | 'missing' | 'error' | 'conflict' | 'queued' | 'running' | 'unknown'

const REPO_FETCH_PER_PAGE = 200
const MAX_REPO_FETCH_PAGES = 100
const JOBS_REFRESH_INTERVAL_MS = 30_000

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

async function listAllTrackedRepos(): Promise<{ repos: GitHubPackagesRepo[]; truncated: boolean }> {
  const rows: GitHubPackagesRepo[] = []
  let page = 1
  let guard = 0

  while (guard < MAX_REPO_FETCH_PAGES) {
    guard += 1
    const resp = await listGitHubPackagesRepos({
      page,
      perPage: REPO_FETCH_PER_PAGE,
      selectedFilter: 'selected',
    })
    rows.push(...resp.repos)

    const maxPage = Math.max(1, Math.ceil(resp.filteredTotal / resp.perPage))
    if (resp.page >= maxPage) return { repos: rows, truncated: false }
    page = resp.page + 1
  }

  return { repos: rows, truncated: true }
}

export function GhcrWebhookRegistryPage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const confirm = useConfirm()
  const [overview, setOverview] = useState<GitHubPackagesWebhookOverviewResponse | null>(null)
  const [repos, setRepos] = useState<GitHubPackagesRepo[]>([])
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [filter, setFilter] = useState<RepoStateFilter>('all')
  const [useFilterDropdown, setUseFilterDropdown] = useState(false)
  const [queryInput, setQueryInput] = useState('')
  const [query, setQuery] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [repoWarning, setRepoWarning] = useState<string | null>(null)
  const refreshRequestIdRef = useRef(0)
  const filterRowRef = useRef<HTMLDivElement | null>(null)
  const lastJobsRefreshAtRef = useRef(0)
  const refreshRunningRef = useRef(false)
  const refreshQueuedRef = useRef(false)
  const refreshForceJobsRef = useRef(false)

  const runRefreshOnce = useCallback(async (forceJobs: boolean) => {
    const requestId = ++refreshRequestIdRef.current
    const shouldRefreshJobs = forceJobs || Date.now() - lastJobsRefreshAtRef.current >= JOBS_REFRESH_INTERVAL_MS
    setError(null)
    try {
      const [nextOverview, repoResult, allJobs] = await Promise.all([
        getGitHubPackagesWebhookOverview(),
        listAllTrackedRepos(),
        shouldRefreshJobs ? listJobs() : Promise.resolve<JobListItem[] | null>(null),
      ])
      if (requestId !== refreshRequestIdRef.current) return
      setOverview(nextOverview)
      setRepos(repoResult.repos)
      setRepoWarning(
        repoResult.truncated
          ? `仓库数量较多，仅展示前 ${REPO_FETCH_PER_PAGE * MAX_REPO_FETCH_PAGES} 条，请缩小筛选范围后重试。`
          : null,
      )
      if (allJobs) {
        setJobs(allJobs.filter((job) => job.type === 'github_packages_webhook'))
        lastJobsRefreshAtRef.current = Date.now()
      }
    } catch (e: unknown) {
      if (requestId !== refreshRequestIdRef.current) return
      setError(errorMessage(e))
    }
  }, [])

  const refresh = useCallback(
    async (opts?: { forceJobs?: boolean }) => {
      if (opts?.forceJobs) refreshForceJobsRef.current = true
      if (refreshRunningRef.current) {
        refreshQueuedRef.current = true
        return
      }

      refreshRunningRef.current = true
      refreshQueuedRef.current = true
      try {
        while (refreshQueuedRef.current) {
          refreshQueuedRef.current = false
          const forceJobs = refreshForceJobsRef.current
          refreshForceJobsRef.current = false
          await runRefreshOnce(forceJobs)
        }
      } finally {
        refreshRunningRef.current = false
      }
    },
    [runRefreshOnce],
  )

  useEffect(() => {
    void refresh({ forceJobs: true })
  }, [refresh])

  useEffect(() => {
    let closed = false
    let es: EventSource | null = null
    let refreshTimer: number | null = null
    let pollTimer: number | null = null

    const scheduleRefresh = (delayMs: number) => {
      if (refreshTimer != null) return
      refreshTimer = window.setTimeout(() => {
        refreshTimer = null
        void refresh()
      }, delayMs)
    }

    const startPolling = () => {
      if (pollTimer != null) return
      pollTimer = window.setInterval(() => {
        void refresh()
      }, 10_000)
    }

    const stopPolling = () => {
      if (pollTimer == null) return
      window.clearInterval(pollTimer)
      pollTimer = null
    }

    const connect = () => {
      if (closed) return
      es = newJobsEventsSource()
      es.addEventListener('open', () => {
        stopPolling()
        scheduleRefresh(0)
      })
      es.addEventListener('job_event', () => scheduleRefresh(250))
      es.addEventListener('job_events_error', () => {
        scheduleRefresh(0)
        startPolling()
      })
      es.onerror = () => {
        scheduleRefresh(0)
        startPolling()
      }
    }

    // Keep polling as fallback until SSE is confirmed open.
    startPolling()
    connect()

    return () => {
      closed = true
      if (refreshTimer != null) window.clearTimeout(refreshTimer)
      stopPolling()
      es?.close()
    }
  }, [refresh])

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
              await refresh({ forceJobs: true })
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

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    return repos.filter((repo) => {
      const state = normalizeWebhookState(repo.webhookState)
      if (filter !== 'all' && state !== filter) return false
      if (!q) return true
      return (
        repo.fullName.toLowerCase().includes(q) ||
        state.includes(q) ||
        (repo.lastError ?? '').toLowerCase().includes(q) ||
        String(repo.hookId ?? '').includes(q)
      )
    })
  }, [filter, query, repos])

  const runningJob = useMemo(() => jobs.find((job) => job.status === 'running') ?? null, [jobs])

  const filterItems = useMemo(
    () => [
      { key: 'all', label: '全部', count: repos.length },
      { key: 'ok', label: '已注册', count: summary.ok },
      { key: 'missing', label: '缺失', count: summary.missing },
      { key: 'error', label: '失败', count: summary.error },
      { key: 'conflict', label: '冲突', count: summary.conflict },
      { key: 'queued', label: '排队中', count: summary.queued },
      { key: 'running', label: '运行中', count: summary.running },
      { key: 'unknown', label: '未知', count: summary.unknown },
    ],
    [repos.length, summary.conflict, summary.error, summary.missing, summary.ok, summary.queued, summary.running, summary.unknown],
  )

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
    <div className="page ghcrRegistryPage">
      <div className="ghcrRegistrySummaryGrid">
        <div className="ghcrRegistrySummaryItem">
          <div className="muted">Tracked Repos</div>
          <div className="ghcrRegistrySummaryValue">
            <Mono>{summary.tracked}</Mono>
          </div>
        </div>
        <div className="ghcrRegistrySummaryItem">
          <div className="muted">Webhook 状态</div>
          <div className="ghcrRegistrySummaryValue">
            <Mono>
              ok {summary.ok} · missing {summary.missing} · error {summary.error} · conflict {summary.conflict}
            </Mono>
          </div>
        </div>
        <div className="ghcrRegistrySummaryItem">
          <div className="muted">Job 状态</div>
          <div className="ghcrRegistrySummaryValue">
            <Mono>
              queued {overview?.jobsQueued ?? 0} · running {overview?.jobsRunning ?? 0}
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
              onClick={() => setFilter(it.key as RepoStateFilter)}
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
            <select
              id="ghcr-registry-filter-select"
              className="select ghcrRegistryFilterSelect"
              value={filter}
              onChange={(event) => setFilter(event.target.value as RepoStateFilter)}
            >
              {filterItems.map((it) => (
                <option key={it.key} value={it.key}>
                  {`${it.label} (${it.count})`}
                </option>
              ))}
            </select>
          </div>
        ) : null}

        <div className="ghcrRegistrySearchForm">
          <input
            className="input"
            value={queryInput}
            onChange={(event) => setQueryInput(event.target.value)}
            placeholder="搜索 owner/repo、状态、hookId、错误信息"
            onKeyDown={(event) => {
              if (event.key !== 'Enter') return
              event.preventDefault()
              setQuery(queryInput.trim())
            }}
          />
          <Button variant="ghost" onClick={() => setQuery(queryInput.trim())}>
            搜索
          </Button>
          <Button
            variant="ghost"
            disabled={!query && !queryInput}
            onClick={() => {
              setQueryInput('')
              setQuery('')
            }}
          >
            清除
          </Button>
        </div>

        <div className="chipRow" style={{ marginLeft: 'auto' }}>
          <Button variant="ghost" onClick={() => navigate({ name: 'settings' })}>
            返回设置
          </Button>
          <Button variant="ghost" onClick={() => navigate({ name: 'ghcr-webhooks' })}>
            队列视图
          </Button>
          <Button variant="ghost" onClick={() => navigate({ name: 'ghcr-webhook-inbox' })}>
            收件箱
          </Button>
        </div>
      </div>

      {runningJob ? (
        <div className="muted">
          当前运行任务：<Mono>{runningJob.id}</Mono>
          {runningJob.progress?.message ? ` · ${runningJob.progress.message}` : ''}
        </div>
      ) : null}
      {repoWarning ? <div className="muted">{repoWarning}</div> : null}

      <div className="ghcrRegistryList">
        {filtered.length === 0 ? (
          <div className="ghcrRegistryEmpty muted">{query || filter !== 'all' ? '当前筛选条件下无仓库' : '暂无已跟踪仓库'}</div>
        ) : null}

        {filtered.map((repo) => {
          const state = normalizeWebhookState(repo.webhookState)
          const dotClass = webhookStateDotClass(state)
          const isInFlight = state === 'queued' || state === 'running'
          const isUnregisterInFlight = isInFlight && (repo.lastOp ?? '') === 'unregister'
          const showRetryDelete = state === 'error' && (repo.lastOp ?? '') === 'unregister'
          const showRetryRegister = state === 'missing' || state === 'conflict' || (state === 'error' && !showRetryDelete)

          return (
            <div key={repo.fullName} className="ghcrRegistryRow">
              <div className="ghcrRegistryMain">
                <div className="ghcrRegistryHeader">
                  <div className="ghcrRegistryTitle">
                    <Icon icon={webhookStateIcon(state)} className={dotClass} aria-hidden="true" />
                    <Mono>{repo.fullName}</Mono>
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
                  {repo.webhookJobId ? (
                    <Button variant="ghost" onClick={() => navigate({ name: 'job', jobId: repo.webhookJobId! })}>
                      查看任务
                    </Button>
                  ) : null}

                  {showRetryRegister ? (
                    <ResponsiveActionButton
                      variant="ghost"
                      disabled={busy}
                      label="重新注册"
                      hint="重新触发 webhook 注册任务"
                      icon={<Icon icon={refreshIcon} aria-hidden="true" />}
                      onClick={() => {
                        void (async () => {
                          setBusy(true)
                          setError(null)
                          try {
                            await setGitHubPackagesRepoSelected({ fullName: repo.fullName, selected: true })
                            await refresh({ forceJobs: true })
                          } catch (e: unknown) {
                            setError(errorMessage(e))
                          } finally {
                            setBusy(false)
                          }
                        })()
                      }}
                    />
                  ) : null}

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
                            await refresh({ forceJobs: true })
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
                          await refresh({ forceJobs: true })
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

      {error ? <div className="error">{error}</div> : null}
    </div>
  )
}
