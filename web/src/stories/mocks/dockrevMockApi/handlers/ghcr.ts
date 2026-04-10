import type {
  AddGitHubPackagesTargetRequest,
  BulkSetGitHubPackagesReposSelectedRequest,
  GitHubPackagesRepo,
  JobDetail,
  JobListItem,
  ListGitHubPackagesReposResponse,
  PutGitHubPackagesSettingsRequest,
  RemoveGitHubPackagesTargetRequest,
  ResolveGitHubPackagesTargetResponse,
  SetGitHubPackagesRepoSelectedRequest,
  SyncGitHubPackagesWebhooksResponse,
} from '../../../../api'
import type { MockRouteContext } from '../context'

export async function handleGhcrRoutes(ctx: MockRouteContext): Promise<Response | null> {
  const { getBoolean, getString, init, json, method, nowIso, parseJsonBody, state: f, url, urlPath } = ctx

  const recomputeGithubPackagesCounts = () => {
    f.githubPackagesSettings = {
      ...f.githubPackagesSettings,
      reposTotal: f.githubPackagesRepos.length,
      reposSelectedTotal: f.githubPackagesRepos.filter((repo) => repo.selected).length,
    }
  }

  const ensureGhcrRepoDefaults = (repo: GitHubPackagesRepo) => {
    if (!repo.webhookState) repo.webhookState = 'unknown'
    if (repo.webhookJobId === undefined) repo.webhookJobId = null
    if (repo.lastAuditAt === undefined) repo.lastAuditAt = null
    if (repo.lastOp === undefined) repo.lastOp = null
    if (repo.lastError === undefined) repo.lastError = null
    if (repo.hookId === undefined) repo.hookId = null
    if (repo.lastSyncAt === undefined) repo.lastSyncAt = null
    return repo
  }

  type MockGhcrOp = 'register' | 'unregister' | 'audit_all' | 'sync_all' | 'sync_repo'

  const ghcrJobTypeByOp = (op: MockGhcrOp): string => {
    if (op === 'sync_all') return 'github_packages_webhook_sync_all'
    if (op === 'sync_repo') return 'github_packages_webhook_sync_repo'
    return 'github_packages_webhook'
  }

  const ghcrQueuedMessage = (op: MockGhcrOp): string => {
    if (op === 'register') return 'waiting to register webhook'
    if (op === 'unregister') return 'waiting to unregister webhook'
    if (op === 'sync_all') return 'waiting to sync tracked repos'
    if (op === 'sync_repo') return 'waiting to sync repo webhook'
    return 'waiting to audit webhook drift'
  }

  const isPending = (status: string): boolean => status === 'queued' || status === 'running'

  const jobTargets = (job: JobListItem): string[] => {
    const summary = job.summary
    if (!summary || typeof summary !== 'object') return []
    const repos = (summary as { repos?: unknown }).repos
    if (!Array.isArray(repos)) return []
    return repos.filter((value): value is string => typeof value === 'string')
  }

  const findPendingGhcrJob = (jobType: string, target?: string): JobListItem | null => {
    const key = (target ?? '').trim().toLowerCase()
    for (const job of f.jobs) {
      if (job.type !== jobType || !isPending(job.status)) continue
      if (!key) return job
      const hasTarget = jobTargets(job).some((repo) => repo.trim().toLowerCase() === key)
      if (hasTarget) return job
    }
    return null
  }

  const apiError = (status: number, code: string, message: string) =>
    json(
      {
        error: {
          code,
          message,
          details: null,
        },
      },
      { status },
    )

  const newGhcrJob = (op: MockGhcrOp, repoFullNames: string[]): JobListItem => {
    ctx.jobSeqRef.value += 1
    const jobId = `job-ghcr-${ctx.jobSeqRef.value}`
    const createdAt = nowIso(-200)
    const message = ghcrQueuedMessage(op)
    const target = repoFullNames[0] ?? '-'
    return {
      id: jobId,
      type: ghcrJobTypeByOp(op),
      scope: 'all',
      stackId: null,
      serviceId: null,
      status: 'queued',
      createdBy: 'ivan',
      reason: 'ui',
      createdAt,
      startedAt: null,
      finishedAt: null,
      allowArchMismatch: false,
      backupMode: 'inherit',
      summary: {
        op,
        repos: repoFullNames,
        progress: {
          phase: 'queued',
          message,
          current: 0,
          total: repoFullNames.length,
          percent: 0,
          plannedCurrent: 0,
          plannedTotal: repoFullNames.length,
          plannedPercent: 0,
          currentTarget: target,
          updatedAt: createdAt,
        },
      },
      progress: {
        phase: 'queued',
        message,
        current: 0,
        total: repoFullNames.length,
        percent: 0,
        plannedCurrent: 0,
        plannedTotal: repoFullNames.length,
        plannedPercent: 0,
        currentTarget: target,
        updatedAt: createdAt,
      },
    }
  }

  const insertGhcrQueuedJob = (op: MockGhcrOp, repoFullNames: string[]): string => {
    const job = newGhcrJob(op, repoFullNames)
    f.jobs = [job, ...f.jobs]
    const jobDetail: JobDetail = {
      ...job,
      logs: [
        {
          ts: job.createdAt,
          level: 'event',
          msg: JSON.stringify({
            type: 'job_enqueued',
            jobType: job.type,
            op,
            target: repoFullNames[0] ?? null,
            jobId: job.id,
            ts: job.createdAt,
          }),
        },
      ],
      logsLastId: 1,
    }
    f.jobById[job.id] = jobDetail
    return job.id
  }

  const buildGhcrOverview = () => {
    const summary = {
      tracked: 0,
      ok: 0,
      missing: 0,
      error: 0,
      conflict: 0,
      queued: 0,
      running: 0,
      unknown: 0,
    }
    let lastAuditAt: string | null = null
    for (const row of f.githubPackagesRepos) {
      if (!row.selected) continue
      ensureGhcrRepoDefaults(row)
      summary.tracked += 1
      const state = (row.webhookState ?? 'unknown').toLowerCase()
      if (state === 'ok') summary.ok += 1
      else if (state === 'missing') summary.missing += 1
      else if (state === 'error') summary.error += 1
      else if (state === 'conflict') summary.conflict += 1
      else if (state === 'queued') summary.queued += 1
      else if (state === 'running') summary.running += 1
      else summary.unknown += 1
      if (row.lastAuditAt && (!lastAuditAt || row.lastAuditAt > lastAuditAt)) lastAuditAt = row.lastAuditAt
    }

    const ghcrJobs = f.jobs.filter(
      (job) =>
        job.type === 'github_packages_webhook' ||
        job.type === 'github_packages_webhook_sync_all' ||
        job.type === 'github_packages_webhook_sync_repo',
    )
    const jobsQueued = ghcrJobs.filter((job) => job.status === 'queued').length
    const jobsRunning = ghcrJobs.filter((job) => job.status === 'running').length
    const runningJobId = ghcrJobs.find((job) => job.status === 'running')?.id ?? null

    return {
      summary,
      jobsQueued,
      jobsRunning,
      runningJobId,
      lastAuditAt,
    }
  }

  if (method === 'GET' && urlPath === '/api/github-packages/settings') {
    recomputeGithubPackagesCounts()
    return json(f.githubPackagesSettings)
  }

  if (method === 'PUT' && urlPath === '/api/github-packages/settings') {
    const body = typeof init?.body === 'string' ? init.body : ''
    const parsed = body ? (JSON.parse(body) as PutGitHubPackagesSettingsRequest) : null
    if (parsed) {
      let nextPatMasked: string | null = f.githubPackagesSettings.patMasked ?? null
      if (typeof parsed.pat === 'string' && parsed.pat !== '******' && parsed.pat.trim() !== '') {
        nextPatMasked = '******'
      }
      if (Array.isArray(parsed.targets)) {
        f.githubPackagesSettings.targets = parsed.targets.map((target) => ({
          input: target.input,
          kind: 'owner',
          owner: target.input,
          warnings: [],
        }))
      }
      if (Array.isArray(parsed.repos)) {
        const selectedByRepo = new Map(parsed.repos.map((repo) => [repo.fullName, Boolean(repo.selected)]))
        for (const repo of f.githubPackagesRepos) {
          if (selectedByRepo.has(repo.fullName)) repo.selected = Boolean(selectedByRepo.get(repo.fullName))
        }
      }
      f.githubPackagesSettings.enabled = parsed.enabled
      f.githubPackagesSettings.callbackUrl = parsed.callbackUrl
      f.githubPackagesSettings.patMasked = nextPatMasked
      recomputeGithubPackagesCounts()
    }
    return json({ ok: true })
  }

  if (method === 'GET' && urlPath === '/api/github-packages/repos') {
    const params = url?.searchParams ?? new URLSearchParams()
    const page = Math.max(1, Number(params.get('page') ?? '1') || 1)
    const perPage = Math.min(200, Math.max(1, Number(params.get('perPage') ?? '50') || 50))
    const q = (params.get('q') ?? '').trim().toLowerCase()
    const selectedFilter = (params.get('selectedFilter') ?? 'all').trim()

    const matchesQ = (repo: GitHubPackagesRepo) => (q ? repo.fullName.toLowerCase().includes(q) : true)
    const matchesSelected = (repo: GitHubPackagesRepo) => {
      if (selectedFilter === 'selected') return repo.selected
      if (selectedFilter === 'unselected') return !repo.selected
      return true
    }

    const filtered = f.githubPackagesRepos.filter((repo) => matchesQ(repo) && matchesSelected(repo))
    const offset = (page - 1) * perPage
    const items = filtered.slice(offset, offset + perPage)

    const resp: ListGitHubPackagesReposResponse = {
      page,
      perPage,
      total: f.githubPackagesRepos.length,
      filteredTotal: filtered.length,
      selectedTotal: f.githubPackagesRepos.filter((repo) => repo.selected).length,
      repos: items,
    }
    recomputeGithubPackagesCounts()
    return json(resp)
  }

  if (method === 'GET' && urlPath === '/api/github-packages/webhook/overview') {
    return json(buildGhcrOverview())
  }

  if (method === 'POST' && urlPath === '/api/github-packages/repos/selected') {
    const parsed = parseJsonBody(init?.body) as SetGitHubPackagesRepoSelectedRequest | null
    const fullName = getString(parsed?.fullName)?.trim() ?? ''
    const selected = getBoolean(parsed?.selected)
    if (!fullName || selected === null) return json({ error: 'invalid input' }, { status: 400 })

    const row = f.githubPackagesRepos.find((repo) => repo.fullName === fullName)
    if (!row) {
      f.githubPackagesRepos.push(
        ensureGhcrRepoDefaults({
          fullName,
          selected,
          webhookState: selected ? 'queued' : 'unknown',
          webhookJobId: null,
          hookId: null,
          lastSyncAt: null,
          lastAuditAt: null,
          lastOp: selected ? 'register' : null,
          lastError: null,
        }),
      )
    } else {
      ensureGhcrRepoDefaults(row)
      row.selected = selected
    }

    recomputeGithubPackagesCounts()
    let jobId: string | null = null
    if (selected) {
      const target = f.githubPackagesRepos.find((repo) => repo.fullName === fullName)
      if (target) {
        target.webhookState = 'queued'
        target.lastOp = 'register'
        target.lastError = null
        jobId = insertGhcrQueuedJob('register', [fullName])
        target.webhookJobId = jobId
      }
    }
    return json({ ok: true, jobId })
  }

  if (method === 'POST' && urlPath === '/api/github-packages/repos/delete') {
    const parsed = parseJsonBody(init?.body) as { fullName?: unknown } | null
    const fullName = getString(parsed?.fullName)?.trim() ?? ''
    if (!fullName) return json({ error: 'invalid input' }, { status: 400 })

    const row = f.githubPackagesRepos.find((repo) => repo.fullName === fullName)
    if (!row) return json({ error: 'repo is not tracked' }, { status: 404 })

    ensureGhcrRepoDefaults(row)
    row.webhookState = 'queued'
    row.lastOp = 'unregister'
    row.lastError = null
    const jobId = insertGhcrQueuedJob('unregister', [fullName])
    row.webhookJobId = jobId
    recomputeGithubPackagesCounts()
    return json({ ok: true, jobId })
  }

  if (method === 'POST' && urlPath === '/api/github-packages/repos/bulk-selected') {
    const parsed = parseJsonBody(init?.body) as BulkSetGitHubPackagesReposSelectedRequest | null
    const q = (getString(parsed?.q) ?? '').trim().toLowerCase()
    const selectedFilter = (getString(parsed?.selectedFilter) ?? 'all').trim()
    const selected = getBoolean(parsed?.selected)
    if (selected === null) return json({ error: 'invalid input' }, { status: 400 })

    const matchesQ = (repo: GitHubPackagesRepo) => (q ? repo.fullName.toLowerCase().includes(q) : true)
    const matchesSelected = (repo: GitHubPackagesRepo) => {
      if (selectedFilter === 'selected') return repo.selected
      if (selectedFilter === 'unselected') return !repo.selected
      return true
    }

    let affected = 0
    for (const repo of f.githubPackagesRepos) {
      if (!matchesQ(repo) || !matchesSelected(repo)) continue
      if (repo.selected !== selected) {
        repo.selected = selected
        affected += 1
      }
    }
    recomputeGithubPackagesCounts()
    return json({ ok: true, affected })
  }

  if (method === 'POST' && urlPath === '/api/github-packages/targets/add') {
    const parsed = parseJsonBody(init?.body) as AddGitHubPackagesTargetRequest | null
    const inputStr = getString(parsed?.input)?.trim() ?? ''
    if (!inputStr) return json({ error: 'invalid input' }, { status: 400 })
    if (!f.githubPackagesSettings.patMasked) return json({ error: 'pat is required' }, { status: 400 })

    let owner = inputStr
    let repo: string | null = null
    if (inputStr.includes('github.com/')) {
      const match = inputStr.match(/github\.com\/(?:orgs\/)?([^/]+)(?:\/([^/]+))?/i)
      owner = match?.[1] ?? inputStr
      repo = match?.[2]?.replace(/\\.git$/i, '') ?? null
    } else if (inputStr.includes('/')) {
      const parts = inputStr.split('/').filter(Boolean)
      if (parts.length >= 2) {
        owner = parts[0] ?? inputStr
        repo = (parts[1] ?? '').replace(/\\.git$/i, '') || null
      }
    }

    if (!f.githubPackagesSettings.targets.some((target) => target.input === inputStr)) {
      f.githubPackagesSettings.targets.push({
        input: inputStr,
        kind: repo ? 'repo' : 'owner',
        owner,
        warnings: [],
      })
    }

    const before = new Set(f.githubPackagesRepos.map((repoItem) => repoItem.fullName))
    if (repo) {
      const fullName = `${owner}/${repo}`
      if (!before.has(fullName)) {
        f.githubPackagesRepos.push({ fullName, selected: true, hookId: null, lastSyncAt: null, lastError: null })
      }
    } else {
      for (let index = 1; index <= 120; index += 1) {
        const fullName = `${owner}/added-${String(index).padStart(3, '0')}`
        if (!before.has(fullName)) {
          f.githubPackagesRepos.push({ fullName, selected: true, hookId: null, lastSyncAt: null, lastError: null })
        }
      }
    }

    recomputeGithubPackagesCounts()
    const reposAdded = f.githubPackagesRepos.length - before.size
    return json({ ok: true, kind: repo ? 'repo' : 'owner', owner, reposAdded })
  }

  if (method === 'POST' && urlPath === '/api/github-packages/targets/remove') {
    const parsed = parseJsonBody(init?.body) as RemoveGitHubPackagesTargetRequest | null
    const inputStr = getString(parsed?.input)?.trim() ?? ''
    if (!inputStr) return json({ error: 'invalid input' }, { status: 400 })
    f.githubPackagesSettings.targets = f.githubPackagesSettings.targets.filter((target) => target.input !== inputStr)
    recomputeGithubPackagesCounts()
    return json({ ok: true })
  }

  if (method === 'POST' && urlPath === '/api/github-packages/resolve') {
    if (ctx.scenario === 'settings-configured-resolve-slow') {
      await new Promise<void>((resolve) => {
        globalThis.setTimeout(() => resolve(), 900)
      })
    }

    const body = typeof init?.body === 'string' ? init.body : ''
    const parsed = body ? (JSON.parse(body) as { input?: string }) : null
    const inputStr = typeof parsed?.input === 'string' ? parsed.input.trim() : ''
    if (!inputStr) return json({ error: 'invalid input' }, { status: 400 })
    if (!f.githubPackagesSettings.patMasked) return json({ error: 'pat is required' }, { status: 400 })

    const mkOwner = (owner: string): ResolveGitHubPackagesTargetResponse => ({
      kind: 'owner',
      owner,
      repos: f.githubPackagesRepos
        .filter((repo) => repo.fullName.startsWith(`${owner}/`))
        .slice(0, 180)
        .map((repo, index) => {
          const visibility = repo.fullName.includes('private') || index % 9 === 0 ? 'private' : 'public'
          const lastActivityAt = index % 13 === 0 ? null : nowIso(-(index + 1) * 21_600_000)
          const ghcrLinked = index % 4 === 0
          const deployed = index % 7 === 0
          return {
            fullName: repo.fullName,
            selected: repo.selected,
            visibility,
            lastActivityAt,
            ghcrLinked,
            deployed,
          }
        }),
      warnings: [],
    })

    if (inputStr.includes('github.com/')) {
      const match = inputStr.match(/github\.com\/(?:orgs\/)?([^/]+)(?:\/([^/]+))?/i)
      const owner = match?.[1] ?? 'unknown'
      const repo = match?.[2]
      if (repo) {
        const fullName = `${owner}/${repo.replace(/\\.git$/i, '')}`
        const existing = f.githubPackagesRepos.find((item) => item.fullName === fullName)
        const resp: ResolveGitHubPackagesTargetResponse = {
          kind: 'repo',
          owner,
          repos: [
            {
              fullName,
              selected: existing?.selected ?? true,
              visibility: 'unknown',
              lastActivityAt: null,
              ghcrLinked: null,
              deployed: false,
            },
          ],
          warnings: [],
        }
        return json(resp)
      }
      return json(mkOwner(owner))
    }

    return json(mkOwner(inputStr))
  }

  if (method === 'POST' && urlPath === '/api/github-packages/webhook/sync-all') {
    const existing = findPendingGhcrJob('github_packages_webhook_sync_all')
    if (existing) {
      return json({ ok: true, jobId: existing.id, status: existing.status, reused: true })
    }

    const selected = f.githubPackagesRepos
      .filter((repo) => repo.selected)
      .filter(
        (repo) => !(repo.lastOp === 'unregister' && (repo.webhookState === 'queued' || repo.webhookState === 'running')),
      )
      .map((repo) => repo.fullName)
    if (selected.length === 0) return apiError(400, 'invalid_argument', 'no tracked repos selected')

    const jobId = insertGhcrQueuedJob('sync_all', selected)
    return json({ ok: true, jobId, status: 'queued', reused: false })
  }

  if (method === 'POST' && urlPath === '/api/github-packages/webhook/sync-repo') {
    const parsed = parseJsonBody(init?.body) as { fullName?: unknown } | null
    const fullName = getString(parsed?.fullName)?.trim() ?? ''
    if (!fullName) return apiError(400, 'invalid_argument', 'invalid input')

    const row = f.githubPackagesRepos.find((repo) => repo.fullName.toLowerCase() === fullName.toLowerCase())
    if (!row) return apiError(404, 'not_found', 'repo is not tracked')
    if (!row.selected) return apiError(400, 'invalid_argument', 'repo is not selected')
    if (row.lastOp === 'unregister' && (row.webhookState === 'queued' || row.webhookState === 'running')) {
      return apiError(409, 'conflict', 'repo unregister in progress')
    }
    if (row.lastOp === 'register' && (row.webhookState === 'queued' || row.webhookState === 'running') && row.webhookJobId) {
      const legacy = f.jobs.find(
        (job) =>
          job.id === row.webhookJobId &&
          job.type === 'github_packages_webhook' &&
          isPending(job.status) &&
          ((job.summary as { op?: unknown } | undefined)?.op === 'register'),
      )
      if (legacy) {
        return json({ ok: true, jobId: legacy.id, status: legacy.status, reused: true })
      }
    }

    const existing = findPendingGhcrJob('github_packages_webhook_sync_repo', fullName)
    if (existing) {
      return json({ ok: true, jobId: existing.id, status: existing.status, reused: true })
    }

    ensureGhcrRepoDefaults(row)
    row.webhookState = 'queued'
    row.lastOp = 'register'
    row.lastError = null
    const jobId = insertGhcrQueuedJob('sync_repo', [row.fullName])
    row.webhookJobId = jobId
    return json({ ok: true, jobId, status: 'queued', reused: false })
  }

  if (method === 'POST' && urlPath === '/api/github-packages/sync') {
    const parsed = parseJsonBody(init?.body) as { repos?: unknown } | null
    const allow = Array.isArray(parsed?.repos)
      ? new Set(parsed.repos.map((value) => getString(value)?.trim()).filter(Boolean) as string[])
      : null
    const selected = f.githubPackagesRepos.filter((repo) => repo.selected && (!allow || allow.has(repo.fullName)))
    const results = selected.map((repo) => {
      ensureGhcrRepoDefaults(repo)
      repo.webhookState = 'queued'
      repo.lastOp = 'register'
      repo.lastError = null
      const existing = findPendingGhcrJob('github_packages_webhook_sync_repo', repo.fullName)
      const jobId = existing ? existing.id : insertGhcrQueuedJob('sync_repo', [repo.fullName])
      repo.webhookJobId = jobId
      return {
        repo: repo.fullName,
        action: 'queued',
        hookId: null,
        conflictHooks: null,
        message: `jobId=${jobId}`,
      }
    })
    const resp: SyncGitHubPackagesWebhooksResponse = { ok: true, results }
    return json(resp)
  }

  return null
}
