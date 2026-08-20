import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  deleteGitHubPackagesRepo,
  getGitHubPackagesWebhookOverview,
  listGitHubPackagesRepos,
  listJobs,
  triggerGitHubPackagesWebhookSyncRepo,
  type GitHubPackagesRepo,
  type GitHubPackagesWebhookOverviewResponse,
  type JobListItem,
} from '../api'
import { useConfirm } from '../confirm'
import { useManagementEventBatch } from '../managementEvents'
import { navigate } from '../routes'
import { Button, Mono, Pill } from '../ui'
import { AsyncDataRegion, AsyncDataSkeleton } from '../components/AsyncDataRegion'
import { isAsyncDataBusy, type AsyncDataPhase, type AsyncDataSource, type AsyncDataTrigger } from '../asyncData'

const GHCR_JOB_TYPES = new Set([
  'github_packages_webhook',
  'github_packages_webhook_sync_all',
  'github_packages_webhook_sync_repo',
])

function isGhcrJobType(type: string): boolean {
  return GHCR_JOB_TYPES.has(type)
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

function ghcrJobStatusTone(status: string): 'ok' | 'warn' | 'bad' | 'muted' {
  if (status === 'success') return 'ok'
  if (status === 'failed') return 'bad'
  if (status === 'queued' || status === 'running') return 'warn'
  return 'muted'
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

export function GhcrWebhookQueuePage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const confirm = useConfirm()
  const [overview, setOverview] = useState<GitHubPackagesWebhookOverviewResponse | null>(null)
  const [repos, setRepos] = useState<GitHubPackagesRepo[]>([])
  const [jobs, setJobs] = useState<JobListItem[]>([])
  const [phase, setPhase] = useState<AsyncDataPhase>('initial-loading')
  const [source, setSource] = useState<AsyncDataSource>('none')
  const [trigger, setTrigger] = useState<AsyncDataTrigger>('background')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const refreshRequestIdRef = useRef(0)
  const hasCommittedDataRef = useRef(false)

  const refresh = useCallback(async (nextSource: AsyncDataSource = 'live', nextTrigger: AsyncDataTrigger = 'background') => {
    const requestId = ++refreshRequestIdRef.current
    setSource(nextSource)
    setTrigger(nextTrigger)
    setPhase(hasCommittedDataRef.current ? 'refreshing' : 'initial-loading')
    setError(null)
    try {
      const [nextOverview, repoResp, allJobs] = await Promise.all([
        getGitHubPackagesWebhookOverview(),
        listGitHubPackagesRepos({ page: 1, perPage: 200, selectedFilter: 'selected' }),
        listJobs(),
      ])
      if (requestId !== refreshRequestIdRef.current) return
      setOverview(nextOverview)
      setRepos(repoResp.repos)
      setJobs(allJobs.filter((job) => isGhcrJobType(job.type)))
      hasCommittedDataRef.current = true
      setPhase('ready-data')
    } catch (reason: unknown) {
      if (requestId !== refreshRequestIdRef.current) return
      setPhase('error')
      throw reason
    }
  }, [])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(errorMessage(e)))
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
        disabled={busy || isAsyncDataBusy(phase, trigger)}
        onClick={() => {
          void (async () => {
            setBusy(true)
            try {
              await refresh('memory', 'user-action')
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
  }, [busy, onTopActions, phase, refresh, trigger])

  const runningJob = useMemo(() => jobs.find((job) => job.status === 'running') ?? null, [jobs])
  const recentJobs = useMemo(() => jobs.slice(0, 20), [jobs])

  return (
    <div className="page">
      <AsyncDataRegion
        className="card"
        error={error}
        hasData={overview !== null}
        label="正在刷新 GHCR Webhook 状态"
        onRetry={() => void refresh('memory', 'user-action').catch((reason: unknown) => setError(errorMessage(reason)))}
        phase={phase}
        skeleton={<AsyncDataSkeleton className="ghcrQueueLoadingSkeleton" lines={7} />}
        source={source}
        trigger={trigger}
      >
        <div className="sectionRow">
          <div className="title">GHCR Webhook 状态</div>
          <div className="chipRow" style={{ marginLeft: 'auto' }}>
            <Button variant="ghost" onClick={() => navigate({ name: 'queue' })}>
              返回队列
            </Button>
          </div>
        </div>

        <div className="queueMeta" style={{ marginTop: 10 }}>
          <span>
            tracked <Mono>{overview?.summary.tracked ?? '—'}</Mono>
          </span>
          <span>
            ok <Mono>{overview?.summary.ok ?? '—'}</Mono>
          </span>
          <span>
            missing <Mono>{overview?.summary.missing ?? '—'}</Mono>
          </span>
          <span>
            error <Mono>{overview?.summary.error ?? '—'}</Mono>
          </span>
          <span>
            conflict <Mono>{overview?.summary.conflict ?? '—'}</Mono>
          </span>
          <span>
            jobsQueued <Mono>{overview?.jobsQueued ?? '—'}</Mono>
          </span>
          <span>
            jobsRunning <Mono>{overview?.jobsRunning ?? '—'}</Mono>
          </span>
          <span>
            lastAuditAt <Mono>{formatShort(overview?.lastAuditAt)}</Mono>
          </span>
        </div>
        {runningJob ? (
          <div className="muted" style={{ marginTop: 8 }}>
            当前运行任务：<Mono>{runningJob.id}</Mono>
            {runningJob.progress?.message ? ` · ${runningJob.progress.message}` : ''}
          </div>
        ) : null}

        <div className="queueList" style={{ marginTop: 12 }}>
          {phase === 'ready-data' && repos.length === 0 ? <div className="muted">暂无已跟踪仓库</div> : null}
          {repos.map((repo) => {
            const state = normalizeWebhookState(repo.webhookState)
            const showRetryDelete = state === 'error' && (repo.lastOp ?? '') === 'unregister'
            const showRetryRegister = state === 'missing' || state === 'conflict' || (state === 'error' && !showRetryDelete)
            return (
              <div key={repo.fullName} className="queueItem" style={{ cursor: 'default' }}>
                <div className="queueMain">
                  <div className="queueTitle">
                    <Mono>{repo.fullName}</Mono>
                  </div>
                  <div className="queueMeta">
                    <span>
                      state <Mono>{webhookStateLabel(state)}</Mono>
                    </span>
                    <span>
                      hookId <Mono>{repo.hookId != null ? String(repo.hookId) : '-'}</Mono>
                    </span>
                    <span>
                      lastSync <Mono>{formatShort(repo.lastSyncAt)}</Mono>
                    </span>
                    <span>
                      lastAudit <Mono>{formatShort(repo.lastAuditAt)}</Mono>
                    </span>
                    {repo.lastError ? (
                      <span>
                        error <Mono>{repo.lastError}</Mono>
                      </span>
                    ) : null}
                  </div>
                  <div className="chipRow" style={{ marginTop: 8 }}>
                    {repo.webhookJobId ? (
                      <Button variant="ghost" onClick={() => navigate({ name: 'job', jobId: repo.webhookJobId! })}>
                        查看任务
                      </Button>
                    ) : null}

                    {showRetryRegister ? (
                      <Button
                        variant="ghost"
                        onClick={() => {
                          void (async () => {
                            setBusy(true)
                            setError(null)
                            try {
                              await triggerGitHubPackagesWebhookSyncRepo({ fullName: repo.fullName })
                              await refresh('memory', 'user-action')
                            } catch (e: unknown) {
                              setError(errorMessage(e))
                            } finally {
                              setBusy(false)
                            }
                          })()
                        }}
                      >
                        重试注册
                      </Button>
                    ) : null}

                    {showRetryDelete ? (
                      <Button
                        variant="ghost"
                        onClick={() => {
                          void (async () => {
                            setBusy(true)
                            setError(null)
                            try {
                              await deleteGitHubPackagesRepo({ fullName: repo.fullName })
                              await refresh('memory', 'user-action')
                            } catch (e: unknown) {
                              setError(errorMessage(e))
                            } finally {
                              setBusy(false)
                            }
                          })()
                        }}
                      >
                        重试删除
                      </Button>
                    ) : null}

                    <Button
                      variant="danger"
                      onClick={() => {
                        void (async () => {
                          const ok = await confirm({
                            title: '删除跟踪仓库',
                            body: (
                              <div>
                                <div className="modalLead">将创建后台任务反注册 webhook，并移除跟踪：</div>
                                <Mono>{repo.fullName}</Mono>
                              </div>
                            ),
                            confirmText: '删除',
                            cancelText: '取消',
                            confirmVariant: 'danger',
                          })
                          if (!ok) return
                          setBusy(true)
                          setError(null)
                          try {
                            await deleteGitHubPackagesRepo({ fullName: repo.fullName })
                            await refresh('memory', 'user-action')
                          } catch (e: unknown) {
                            setError(errorMessage(e))
                          } finally {
                            setBusy(false)
                          }
                        })()
                      }}
                    >
                      删除
                    </Button>
                  </div>
                </div>
                <div className="queueStatus">
                  <Pill tone={webhookStateTone(state)}>{state}</Pill>
                </div>
              </div>
            )
          })}
        </div>

        <div className="title" style={{ marginTop: 16 }}>最近 GHCR Webhook Jobs</div>
        <div className="queueList">
          {phase === 'ready-data' && recentJobs.length === 0 ? <div className="muted">暂无任务</div> : null}
          {recentJobs.map((job) => (
            <button key={job.id} className="queueItem" onClick={() => navigate({ name: 'job', jobId: job.id })}>
              <div className="queueMain">
                <div className="queueTitle">
                  <Mono>{job.id}</Mono>
                </div>
                <div className="queueMeta">
                  <span>
                    status <Mono>{job.status}</Mono>
                  </span>
                  <span>
                    reason <Mono>{job.reason}</Mono>
                  </span>
                  <span>
                    created <Mono>{formatShort(job.createdAt)}</Mono>
                  </span>
                  <span>
                    started <Mono>{formatShort(job.startedAt)}</Mono>
                  </span>
                </div>
              </div>
              <div className="queueStatus">
                <Pill tone={ghcrJobStatusTone(job.status)}>{job.status}</Pill>
              </div>
            </button>
          ))}
        </div>

      </AsyncDataRegion>
    </div>
  )
}
