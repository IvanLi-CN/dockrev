import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  ApiError,
  createWebPushSubscription,
  deleteGitHubPackagesRepo,
  deleteWebPushSubscription,
  getGitHubPackagesSettings,
  listJobs,
  getNotifications,
  getSettings,
  listGitHubPackagesRepos,
  newJobsEventsSource,
  putGitHubPackagesSettings,
  putNotifications,
  putSettings,
  resolveGitHubPackagesTarget,
  setGitHubPackagesRepoSelected,
  testNotifications,
  apiBaseUrl,
  type GitHubPackagesSettingsResponse,
  type JobListItem,
  type ListGitHubPackagesReposResponse,
  type ResolveGitHubPackagesTargetResponse,
  type NotificationConfig,
  type SettingsResponse,
} from '../api'
import { Button, IconButton, Mono, Switch, TrashIcon } from '../ui'
import { useConfirm } from '../confirm'
import { selfUpgradeBaseUrl } from '../runtimeConfig'
import { useSupervisorHealth } from '../useSupervisorHealth'
import { navigate } from '../routes'

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function base64UrlToUint8Array(base64UrlString: string): Uint8Array {
  const padding = '='.repeat((4 - (base64UrlString.length % 4)) % 4)
  const base64 = (base64UrlString + padding).replace(/-/g, '+').replace(/_/g, '/')
  const raw = atob(base64)
  const out = new Uint8Array(raw.length)
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i)
  return out
}

function formatBytes(n: number) {
  if (!Number.isFinite(n)) return '-'
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  let v = n
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`
}

type SaveScope = 'backup' | 'notifications' | 'ghcr'
type AutoSavePhase = 'idle' | 'queued' | 'saving' | 'saved' | 'error'
type AutoSaveIssue = {
  scope: SaveScope
  fieldPath: string
  reason: string
  message: string
  at: string
}

const SAVE_SCOPE_ORDER: SaveScope[] = ['backup', 'notifications', 'ghcr']
const TEXT_DEBOUNCE_MS = 400
const TOGGLE_DEBOUNCE_MS = 120
const PAT_MASK = '******'
const GITHUB_PAT_PREFIXES = ['ghp_', 'github_pat_', 'gho_', 'ghu_', 'ghs_', 'ghr_']

type GhcrDraft = {
  enabled: boolean
  callbackUrl: string
  pat: string
  hasPersistedPat: boolean
}

function isMaskedPat(value: string): boolean {
  return value.trim() === PAT_MASK
}

function hasExplicitPat(value: string): boolean {
  const trimmed = value.trim()
  return trimmed.length > 0 && !isMaskedPat(trimmed)
}

function validateGhcrPatBeforeSave(draft: GhcrDraft): { fieldPath: string; reason: string; message: string } | null {
  if (!draft.enabled) return null

  const rawPat = draft.pat
  const trimmedPat = rawPat.trim()
  const explicitPat = hasExplicitPat(trimmedPat)

  if (!explicitPat && !draft.hasPersistedPat) {
    return {
      fieldPath: 'ghcr.pat',
      reason: 'ghcr_pat_missing',
      message: '请先填写 GitHub PAT',
    }
  }

  if (!explicitPat) return null

  if (/\s/.test(rawPat) || !GITHUB_PAT_PREFIXES.some((prefix) => trimmedPat.startsWith(prefix))) {
    return {
      fieldPath: 'ghcr.pat',
      reason: 'ghcr_pat_format_invalid',
      message: 'PAT 格式不合法，请使用 ghp_ / github_pat_ 等 GitHub token',
    }
  }

  return null
}

function readReason(details: unknown): string | null {
  if (!details || typeof details !== 'object') return null
  const reason = (details as Record<string, unknown>).reason
  return typeof reason === 'string' ? reason : null
}

function mapResolveFailure(e: unknown): string {
  if (e instanceof ApiError) {
    const reason = readReason(e.details)
    if (reason === 'ghcr_pat_missing') return '请先填写 GitHub PAT'
    if (reason === 'ghcr_pat_format_invalid') return 'PAT 格式不合法，请使用 ghp_ / github_pat_ 等 GitHub token'
    if (reason === 'ghcr_pat_unsaved_or_save_failed') return 'PAT 未保存成功，无法解析，请检查网络后重试'
    if (reason === 'ghcr_pat_invalid_or_scope_insufficient') return 'PAT 无效或权限不足，请检查 token scope'
    if (reason === 'github_upstream_timeout') return 'GitHub 响应超时，请稍后重试'
    if (reason === 'github_upstream_unavailable') return 'GitHub 请求失败，请稍后重试'
  }
  return errorMessage(e)
}

function mapScopeLabel(scope: SaveScope): string {
  if (scope === 'backup') return '备份'
  if (scope === 'notifications') return '通知'
  return 'GHCR'
}

type RepoSelectedFilter = 'all' | 'selected' | 'unselected'
type RepoVisibilityFilter = 'all' | 'public' | 'private'
type RepoSortKey = 'activity_desc' | 'name_asc'
type RepoListDensity = 'cozy' | 'compact'
type RepoVisibility = 'public' | 'private' | 'unknown'
type RepoPickerItem = {
  fullName: string
  selected: boolean
  visibility: RepoVisibility
  lastActivityAt: string | null
}
const GHCR_PICKER_LIST_DENSITY_STORAGE_KEY = 'dockrev:settings:ghcrPicker:listDensity'

function normalizeRepoVisibility(raw: string | undefined): RepoVisibility {
  if (raw === 'public') return 'public'
  if (raw === 'private') return 'private'
  return 'unknown'
}

function parseActivityMs(raw: string | null): number | null {
  if (!raw) return null
  const ms = Date.parse(raw)
  return Number.isFinite(ms) ? ms : null
}

function formatRepoActivity(raw: string | null): string {
  const ms = parseActivityMs(raw)
  if (ms === null) return '活动时间未知'
  return `最近活动 ${new Date(ms).toLocaleDateString()}`
}

function normalizeRepoListDensity(raw: string | null): RepoListDensity {
  return raw === 'compact' ? 'compact' : 'cozy'
}

function normalizeWebhookState(raw: string | null | undefined): string {
  const state = (raw ?? '').trim().toLowerCase()
  if (!state) return 'unknown'
  return state
}

function webhookStateLabel(state: string): string {
  if (state === 'queued') return '排队中'
  if (state === 'running') return '注册中'
  if (state === 'ok') return '已注册'
  if (state === 'missing') return '缺失'
  if (state === 'error') return '失败'
  if (state === 'conflict') return '冲突'
  return '未知'
}

function webhookStateDotClass(state: string): string {
  if (state === 'ok') return 'statusDot statusDotOk'
  if (state === 'missing' || state === 'queued' || state === 'running') return 'statusDot statusDotWarn'
  if (state === 'error' || state === 'conflict') return 'statusDot statusDotBad'
  return 'statusDot statusDotWarn'
}

function readRepoListDensityFromStorage(): RepoListDensity {
  try {
    return normalizeRepoListDensity(window.localStorage.getItem(GHCR_PICKER_LIST_DENSITY_STORAGE_KEY))
  } catch {
    return 'cozy'
  }
}

function writeRepoListDensityToStorage(value: RepoListDensity) {
  try {
    window.localStorage.setItem(GHCR_PICKER_LIST_DENSITY_STORAGE_KEY, value)
  } catch {
    // Ignore storage errors (quota/disabled).
  }
}

function GitHubPackagesRepoPicker({
  initial,
  onChange,
}: {
  initial: ResolveGitHubPackagesTargetResponse
  onChange: (repos: Array<{ fullName: string; selected: boolean }>) => void
}) {
  const [repos, setRepos] = useState<RepoPickerItem[]>(() =>
    initial.repos.map((r) => ({
      fullName: r.fullName,
      selected: r.selected,
      visibility: normalizeRepoVisibility(r.visibility),
      lastActivityAt: r.lastActivityAt ?? null,
    })),
  )
  const [searchQuery, setSearchQuery] = useState('')
  const [selectedFilter, setSelectedFilter] = useState<RepoSelectedFilter>('all')
  const [visibilityFilter, setVisibilityFilter] = useState<RepoVisibilityFilter>('all')
  const [sortKey, setSortKey] = useState<RepoSortKey>('activity_desc')
  const [listDensity, setListDensity] = useState<RepoListDensity>(() => readRepoListDensityFromStorage())
  const dragSessionRef = useRef<{
    pointerId: number
    targetSelected: boolean
    touched: Set<string>
    captureElement: HTMLButtonElement | null
  } | null>(null)

  const setRepoSelected = useCallback((fullName: string, selected: boolean) => {
    setRepos((prev) => {
      let changed = false
      const next = prev.map((repo) => {
        if (repo.fullName !== fullName || repo.selected === selected) return repo
        changed = true
        return { ...repo, selected }
      })
      return changed ? next : prev
    })
  }, [])

  useEffect(() => {
    onChange(repos.map((r) => ({ fullName: r.fullName, selected: r.selected })))
  }, [repos, onChange])

  const filteredRepos = useMemo(() => {
    const query = searchQuery.trim().toLowerCase()

    const list = repos
      .filter((repo) => {
        if (selectedFilter === 'selected') return repo.selected
        if (selectedFilter === 'unselected') return !repo.selected
        return true
      })
      .filter((repo) => {
        if (visibilityFilter === 'public') return repo.visibility === 'public'
        if (visibilityFilter === 'private') return repo.visibility === 'private'
        return true
      })
      .filter((repo) => {
        if (!query) return true
        return repo.fullName.toLowerCase().includes(query)
      })

    list.sort((a, b) => {
      const byName = a.fullName.localeCompare(b.fullName, undefined, { sensitivity: 'base' })
      if (sortKey === 'name_asc') return byName

      const aActivity = parseActivityMs(a.lastActivityAt)
      const bActivity = parseActivityMs(b.lastActivityAt)
      if (aActivity !== null && bActivity !== null && aActivity !== bActivity) return bActivity - aActivity
      if (aActivity !== null && bActivity === null) return -1
      if (aActivity === null && bActivity !== null) return 1
      return byName
    })

    return list
  }, [repos, searchQuery, selectedFilter, visibilityFilter, sortKey])

  const onWindowPointerMove = useCallback(
    (event: PointerEvent) => {
      const drag = dragSessionRef.current
      if (!drag || drag.pointerId !== event.pointerId) return
      if (event.pointerType === 'mouse' && (event.buttons & 1) === 0) {
        dragSessionRef.current = null
        if (drag.captureElement?.hasPointerCapture(drag.pointerId)) {
          drag.captureElement.releasePointerCapture(drag.pointerId)
        }
        return
      }
      if (event.pointerType === 'touch') event.preventDefault()
      const target = document.elementFromPoint(event.clientX, event.clientY)
      if (!(target instanceof HTMLElement)) return
      const hitNode = target.closest<HTMLElement>('[data-ghcr-picker-switch="true"], [data-ghcr-picker-row="true"]')
      const fullName = hitNode?.dataset.repoFullName
      if (!fullName || drag.touched.has(fullName)) return
      drag.touched.add(fullName)
      setRepoSelected(fullName, drag.targetSelected)
    },
    [setRepoSelected],
  )

  const onWindowPointerEnd = useCallback(
    function handleWindowPointerEnd(event: PointerEvent) {
      const drag = dragSessionRef.current
      if (!drag || drag.pointerId !== event.pointerId) return
      dragSessionRef.current = null
      if (drag.captureElement?.hasPointerCapture(drag.pointerId)) {
        drag.captureElement.releasePointerCapture(drag.pointerId)
      }
      window.removeEventListener('pointermove', onWindowPointerMove)
      window.removeEventListener('pointerup', handleWindowPointerEnd)
      window.removeEventListener('pointercancel', handleWindowPointerEnd)
    },
    [onWindowPointerMove],
  )

  useEffect(() => {
    return () => {
      const drag = dragSessionRef.current
      dragSessionRef.current = null
      if (drag?.captureElement?.hasPointerCapture(drag.pointerId)) {
        drag.captureElement.releasePointerCapture(drag.pointerId)
      }
      window.removeEventListener('pointermove', onWindowPointerMove)
      window.removeEventListener('pointerup', onWindowPointerEnd)
      window.removeEventListener('pointercancel', onWindowPointerEnd)
    }
  }, [onWindowPointerEnd, onWindowPointerMove])

  const onSwitchPointerDown = useCallback(
    (event: React.PointerEvent<HTMLButtonElement>, fullName: string, selected: boolean) => {
      if (event.pointerType === 'mouse' && event.button !== 0) return
      event.preventDefault()

      // Start a drag session where all touched switches are forced to one target state.
      const targetSelected = !selected
      setRepoSelected(fullName, targetSelected)
      const captureElement = event.currentTarget
      try {
        captureElement.setPointerCapture(event.pointerId)
      } catch {
        // Some browsers/input sources may not support pointer capture for this event.
      }
      dragSessionRef.current = {
        pointerId: event.pointerId,
        targetSelected,
        touched: new Set([fullName]),
        captureElement,
      }

      window.addEventListener('pointermove', onWindowPointerMove)
      window.addEventListener('pointerup', onWindowPointerEnd)
      window.addEventListener('pointercancel', onWindowPointerEnd)
    },
    [onWindowPointerEnd, onWindowPointerMove, setRepoSelected],
  )

  const selectedCount = repos.filter((repo) => repo.selected).length
  const listClassName = listDensity === 'compact' ? 'modalList ghcrPickerList ghcrPickerListCompact' : 'modalList ghcrPickerList'

  return (
    <div className="ghcrPickerRoot">
      <div className="modalLead">
        profile <Mono>{initial.owner}</Mono> · 选择要跟踪的仓库
      </div>
      <div className="ghcrPickerLayout">
        <div className="ghcrPickerControls">
          <div className="ghcrPickerField">
            <div className="ghcrPickerFieldLabel">搜索</div>
            <input
              className="input"
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder="搜索 owner/repo"
            />
          </div>
          <div className="ghcrPickerField">
            <div className="ghcrPickerFieldLabel">已添加状态</div>
            <select
              className="select"
              value={selectedFilter}
              onChange={(event) => setSelectedFilter(event.target.value as RepoSelectedFilter)}
              title="按已添加状态筛选"
            >
              <option value="all">全部</option>
              <option value="selected">已添加</option>
              <option value="unselected">未添加</option>
            </select>
          </div>
          <div className="ghcrPickerField">
            <div className="ghcrPickerFieldLabel">可见性</div>
            <select
              className="select"
              value={visibilityFilter}
              onChange={(event) => setVisibilityFilter(event.target.value as RepoVisibilityFilter)}
              title="按可见性筛选"
            >
              <option value="all">全部可见性</option>
              <option value="public">公开</option>
              <option value="private">私有</option>
            </select>
          </div>
          <div className="ghcrPickerField">
            <div className="ghcrPickerFieldLabel">排序方式</div>
            <select
              className="select"
              value={sortKey}
              onChange={(event) => setSortKey(event.target.value as RepoSortKey)}
              title="排序方式"
            >
              <option value="activity_desc">最近活动（新→旧）</option>
              <option value="name_asc">仓库名（A→Z）</option>
            </select>
          </div>
          <div className="ghcrPickerField">
            <div className="ghcrPickerFieldLabel">右侧列表布局</div>
            <button
              type="button"
              className="btn btnGhost ghcrPickerDensityButton"
              aria-pressed={listDensity === 'compact'}
              onClick={() => {
                const next = listDensity === 'compact' ? 'cozy' : 'compact'
                setListDensity(next)
                writeRepoListDensityToStorage(next)
              }}
              title="切换右侧列表布局密度"
            >
              {listDensity === 'compact' ? '紧凑（点击切回宽松）' : '宽松（点击切到紧凑）'}
            </button>
          </div>
          <div className="muted ghcrPickerSummary">
            显示 {filteredRepos.length} / {repos.length} · 已添加 {selectedCount}
          </div>
        </div>
        <div className={listClassName}>
          {filteredRepos.length === 0 ? (
            <div className="ghcrPickerEmpty">没有匹配的仓库</div>
          ) : (
            filteredRepos.map((r) => (
              <div
                key={r.fullName}
                className="modalListItem"
                data-ghcr-picker-row="true"
                data-repo-full-name={r.fullName}
              >
                <div className="modalListLeft" style={{ minWidth: 0 }}>
                  <div className="modalListTitle">
                    <span className="mono" style={{ overflowWrap: 'anywhere' }}>
                      {r.fullName}
                    </span>
                  </div>
                  <div className="ghcrPickerMeta">
                    <span>{r.visibility === 'private' ? '私有' : r.visibility === 'public' ? '公开' : '可见性未知'}</span>
                    <span>{formatRepoActivity(r.lastActivityAt)}</span>
                  </div>
                </div>
                <div className="modalListRight">
                  <button
                    type="button"
                    role="switch"
                    aria-label={`切换 ${r.fullName}`}
                    aria-checked={r.selected}
                    className={r.selected ? 'switch switchButton switchButtonChecked' : 'switch switchButton'}
                    data-ghcr-picker-switch="true"
                    data-repo-full-name={r.fullName}
                    onPointerDown={(event) => onSwitchPointerDown(event, r.fullName, r.selected)}
                    onClick={(event) => {
                      // Pointer interactions are handled in onPointerDown to support drag paint.
                      if (event.detail !== 0) return
                      setRepoSelected(r.fullName, !r.selected)
                    }}
                  >
                    <span className="switchSlider" />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  )
}

export function SettingsPage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const confirm = useConfirm()
  const [settings, setSettings] = useState<SettingsResponse | null>(null)
  const [notifications, setNotifications] = useState<NotificationConfig | null>(null)
  const [githubPackages, setGitHubPackages] = useState<GitHubPackagesSettingsResponse | null>(null)
  const [githubPackagesPat, setGitHubPackagesPat] = useState('')
  const [githubPackagesNewRepo, setGitHubPackagesNewRepo] = useState('')
  const [githubPackagesTrackedRepos, setGitHubPackagesTrackedRepos] = useState<ListGitHubPackagesReposResponse | null>(null)
  const [ghcrLiveJob, setGhcrLiveJob] = useState<JobListItem | null>(null)
  const [githubPackagesTrackedReposPage, setGitHubPackagesTrackedReposPage] = useState(1)
  const [githubPackagesTrackedReposPerPage, setGitHubPackagesTrackedReposPerPage] = useState(50)
  const [githubPackagesTrackedReposQInput, setGitHubPackagesTrackedReposQInput] = useState('')
  const [githubPackagesTrackedReposQ, setGitHubPackagesTrackedReposQ] = useState('')
  const [ghcrResolvePending, setGhcrResolvePending] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [webPushEndpoint, setWebPushEndpoint] = useState<string | null>(null)
  const [autoSavePhase, setAutoSavePhase] = useState<AutoSavePhase>('idle')
  const [autoSaveIssue, setAutoSaveIssue] = useState<AutoSaveIssue | null>(null)
  const [autoSaveSavingScope, setAutoSaveSavingScope] = useState<SaveScope | null>(null)
  const [autoSaveUpdatedAt, setAutoSaveUpdatedAt] = useState<string | null>(null)
  const [autoSaveQueuedScopes, setAutoSaveQueuedScopes] = useState<SaveScope[]>([])

  const backupRef = useRef<SettingsResponse['backup'] | null>(null)
  const notificationsRef = useRef<NotificationConfig | null>(null)
  const ghcrRef = useRef<GhcrDraft | null>(null)
  const queueRef = useRef<SaveScope[]>([])
  const queuedSetRef = useRef<Set<SaveScope>>(new Set())
  const timersRef = useRef<Map<SaveScope, number>>(new Map())
  const pendingFieldsRef = useRef<Map<SaveScope, Set<string>>>(new Map())
  const inFlightScopeRef = useRef<SaveScope | null>(null)
  const runningRef = useRef(false)
  const failedScopesRef = useRef<Set<SaveScope>>(new Set())
  const lastSavedHashRef = useRef<Map<SaveScope, string>>(new Map())
  const waitersRef = useRef<
    Array<{
      scopes: Set<SaveScope>
      resolve: () => void
      reject: (error: Error) => void
    }>
  >([])

  const supervisor = useSupervisorHealth()
  const selfUpgradeUrl = useMemo(() => selfUpgradeBaseUrl(), [])

  useEffect(() => {
    backupRef.current = settings?.backup ?? null
  }, [settings])

  useEffect(() => {
    notificationsRef.current = notifications
  }, [notifications])

  useEffect(() => {
    ghcrRef.current = githubPackages
      ? {
          enabled: githubPackages.enabled,
          callbackUrl: githubPackages.callbackUrl,
          pat: githubPackagesPat,
          hasPersistedPat: Boolean(githubPackages.patMasked),
        }
      : null
  }, [githubPackages, githubPackagesPat])

  useEffect(() => {
    // Debounce to avoid firing requests on every keystroke in the filter.
    const handle = window.setTimeout(() => {
      setGitHubPackagesTrackedReposPage(1)
      setGitHubPackagesTrackedReposQ(githubPackagesTrackedReposQInput)
    }, 250)
    return () => window.clearTimeout(handle)
  }, [githubPackagesTrackedReposQInput])

  const syncQueuedScopesState = useCallback(() => {
    setAutoSaveQueuedScopes([...queueRef.current])
  }, [])

  const isScopeIdle = useCallback((scope: SaveScope) => {
    if (timersRef.current.has(scope)) return false
    if (queuedSetRef.current.has(scope)) return false
    if (inFlightScopeRef.current === scope) return false
    const pending = pendingFieldsRef.current.get(scope)
    return !pending || pending.size === 0
  }, [])

  const settleWaiters = useCallback(() => {
    if (!waitersRef.current.length) return
    const remain: typeof waitersRef.current = []
    for (const waiter of waitersRef.current) {
      const failedScope = SAVE_SCOPE_ORDER.find(
        (scope) => waiter.scopes.has(scope) && failedScopesRef.current.has(scope),
      )
      if (failedScope) {
        const issue = autoSaveIssue
        const message =
          issue && issue.scope === failedScope
            ? issue.message
            : `自动保存失败（${mapScopeLabel(failedScope)}），请修正后重试。`
        waiter.reject(new Error(message))
        continue
      }
      const allIdle = SAVE_SCOPE_ORDER.filter((scope) => waiter.scopes.has(scope)).every((scope) => isScopeIdle(scope))
      if (allIdle) {
        waiter.resolve()
        continue
      }
      remain.push(waiter)
    }
    waitersRef.current = remain
  }, [autoSaveIssue, isScopeIdle])

  const buildScopePayload = useCallback((scope: SaveScope): unknown => {
    if (scope === 'backup') return backupRef.current
    if (scope === 'notifications') return notificationsRef.current
    const ghcr = ghcrRef.current
    if (!ghcr) return null
    const pat = ghcr.pat.trim()
    return {
      enabled: ghcr.enabled,
      callbackUrl: ghcr.callbackUrl,
      pat: pat ? pat : null,
    }
  }, [])

  const persistScopePayload = useCallback(async (scope: SaveScope, payload: unknown) => {
    if (scope === 'backup') {
      await putSettings(payload as SettingsResponse['backup'])
      return
    }
    if (scope === 'notifications') {
      await putNotifications(payload as NotificationConfig)
      return
    }
    await putGitHubPackagesSettings(
      payload as {
        enabled: boolean
        callbackUrl: string
        pat: string | null
      },
    )
  }, [])

  const buildAutoSaveIssue = useCallback((scope: SaveScope, fieldPath: string, e: unknown): AutoSaveIssue => {
    let reason: string | null = null
    if (e instanceof ApiError) reason = readReason(e.details)
    if (!reason && scope === 'ghcr') reason = 'ghcr_pat_unsaved_or_save_failed'

    const fallback = errorMessage(e)
    let message = `自动保存失败（${mapScopeLabel(scope)}）：${fallback}`
    if (reason === 'ghcr_pat_missing') message = '请先填写 GitHub PAT'
    else if (reason === 'ghcr_pat_format_invalid') message = 'PAT 格式不合法，请使用 ghp_ / github_pat_ 等 GitHub token'
    else if (reason === 'ghcr_pat_unsaved_or_save_failed') message = 'PAT 未保存成功，无法解析，请检查网络后重试'
    else if (reason === 'ghcr_pat_invalid_or_scope_insufficient') message = 'PAT 无效或权限不足，请检查 token scope'
    else if (reason === 'github_upstream_timeout') message = 'GitHub 响应超时，请稍后重试'
    else if (reason === 'github_upstream_unavailable') message = 'GitHub 请求失败，请稍后重试'

    return {
      scope,
      fieldPath,
      reason: reason ?? 'autosave_failed',
      message,
      at: new Date().toISOString(),
    }
  }, [])

  const runAutoSaveQueue = useCallback(async () => {
    if (runningRef.current) return
    runningRef.current = true

    try {
      while (queueRef.current.length > 0) {
        const scope = queueRef.current.shift()!
        queuedSetRef.current.delete(scope)
        syncQueuedScopesState()

        const pendingFields = pendingFieldsRef.current.get(scope)
        if (!pendingFields || pendingFields.size === 0) {
          settleWaiters()
          continue
        }

        const payload = buildScopePayload(scope)
        if (!payload) {
          settleWaiters()
          continue
        }

        if (scope === 'ghcr') {
          const ghcrDraft = ghcrRef.current
          if (ghcrDraft) {
            const precheckIssue = validateGhcrPatBeforeSave(ghcrDraft)
            if (precheckIssue) {
              failedScopesRef.current.add(scope)
              setAutoSavePhase('error')
              setAutoSaveIssue({
                scope,
                fieldPath: precheckIssue.fieldPath,
                reason: precheckIssue.reason,
                message: precheckIssue.message,
                at: new Date().toISOString(),
              })
              settleWaiters()
              continue
            }
          }
        }

        const payloadHash = JSON.stringify(payload)
        if (lastSavedHashRef.current.get(scope) === payloadHash) {
          pendingFieldsRef.current.set(scope, new Set())
          failedScopesRef.current.delete(scope)
          setAutoSavePhase(queueRef.current.length > 0 ? 'queued' : 'saved')
          setAutoSaveUpdatedAt(new Date().toISOString())
          settleWaiters()
          continue
        }

        const submittedFields = new Set(pendingFields)
        pendingFieldsRef.current.set(scope, new Set())
        inFlightScopeRef.current = scope
        setAutoSaveSavingScope(scope)
        setAutoSavePhase('saving')
        setAutoSaveIssue((prev) => (prev?.scope === scope ? null : prev))

        try {
          await persistScopePayload(scope, payload)
          failedScopesRef.current.delete(scope)
          lastSavedHashRef.current.set(scope, payloadHash)
          if (scope === 'ghcr') {
            const ghcrPayload = payload as { enabled: boolean; callbackUrl: string; pat: string | null }
            if (ghcrPayload.pat && !isMaskedPat(ghcrPayload.pat)) {
              setGitHubPackages((prev) => (prev ? { ...prev, patMasked: PAT_MASK } : prev))
            }
          }
          setAutoSaveUpdatedAt(new Date().toISOString())
          setAutoSavePhase(queueRef.current.length > 0 ? 'queued' : 'saved')
        } catch (e: unknown) {
          const currentFields = pendingFieldsRef.current.get(scope) ?? new Set<string>()
          for (const field of submittedFields) currentFields.add(field)
          pendingFieldsRef.current.set(scope, currentFields)
          failedScopesRef.current.add(scope)
          setAutoSavePhase('error')
          setAutoSaveIssue(buildAutoSaveIssue(scope, Array.from(submittedFields)[0] ?? '', e))
        } finally {
          inFlightScopeRef.current = null
          setAutoSaveSavingScope(null)
          settleWaiters()
        }
      }
    } finally {
      runningRef.current = false
      settleWaiters()
    }
  }, [
    buildAutoSaveIssue,
    buildScopePayload,
    persistScopePayload,
    settleWaiters,
    syncQueuedScopesState,
  ])

  const enqueueScope = useCallback(
    (scope: SaveScope) => {
      if (queuedSetRef.current.has(scope)) return
      queuedSetRef.current.add(scope)
      queueRef.current.push(scope)
      setAutoSavePhase('queued')
      syncQueuedScopesState()
      void runAutoSaveQueue()
    },
    [runAutoSaveQueue, syncQueuedScopesState],
  )

  const markFieldDirty = useCallback(
    (scope: SaveScope, fieldPath: string, debounceMs: number) => {
      failedScopesRef.current.delete(scope)
      const fields = pendingFieldsRef.current.get(scope) ?? new Set<string>()
      fields.add(fieldPath)
      pendingFieldsRef.current.set(scope, fields)

      const existing = timersRef.current.get(scope)
      if (typeof existing === 'number') window.clearTimeout(existing)
      const timer = window.setTimeout(() => {
        timersRef.current.delete(scope)
        enqueueScope(scope)
        settleWaiters()
      }, debounceMs)
      timersRef.current.set(scope, timer)
      setAutoSavePhase('queued')
      settleWaiters()
    },
    [enqueueScope, settleWaiters],
  )

  const flushAutoSave = useCallback(
    async (scopes?: SaveScope[]) => {
      const target = new Set(scopes ?? SAVE_SCOPE_ORDER)

      for (const scope of target) {
        const timer = timersRef.current.get(scope)
        if (typeof timer === 'number') {
          window.clearTimeout(timer)
          timersRef.current.delete(scope)
        }
        const pendingFields = pendingFieldsRef.current.get(scope)
        if (pendingFields && pendingFields.size > 0) enqueueScope(scope)
      }

      void runAutoSaveQueue()

      const failedScope = SAVE_SCOPE_ORDER.find(
        (scope) => target.has(scope) && failedScopesRef.current.has(scope),
      )
      if (failedScope) {
        const issue = autoSaveIssue
        const message =
          issue && issue.scope === failedScope
            ? issue.message
            : `自动保存失败（${mapScopeLabel(failedScope)}），请修正后重试。`
        throw new Error(message)
      }

      const allIdle = SAVE_SCOPE_ORDER.filter((scope) => target.has(scope)).every((scope) => isScopeIdle(scope))
      if (allIdle) return

      await new Promise<void>((resolve, reject) => {
        waitersRef.current.push({ scopes: target, resolve, reject })
      })
    },
    [autoSaveIssue, enqueueScope, isScopeIdle, runAutoSaveQueue],
  )

  const resetAutoSaveBaselines = useCallback(
    (next: {
      backup: SettingsResponse['backup']
      notifications: NotificationConfig
      ghcr: GhcrDraft
    }) => {
      for (const timer of timersRef.current.values()) window.clearTimeout(timer)
      timersRef.current.clear()
      queueRef.current = []
      queuedSetRef.current.clear()
      pendingFieldsRef.current.clear()
      failedScopesRef.current.clear()
      waitersRef.current = []
      inFlightScopeRef.current = null
      setAutoSaveSavingScope(null)
      setAutoSaveIssue(null)
      setAutoSavePhase('idle')
      setAutoSaveUpdatedAt(null)
      syncQueuedScopesState()

      backupRef.current = next.backup
      notificationsRef.current = next.notifications
      ghcrRef.current = next.ghcr
      lastSavedHashRef.current.set('backup', JSON.stringify(next.backup))
      lastSavedHashRef.current.set('notifications', JSON.stringify(next.notifications))
      lastSavedHashRef.current.set(
        'ghcr',
        JSON.stringify({
          enabled: next.ghcr.enabled,
          callbackUrl: next.ghcr.callbackUrl,
          pat: next.ghcr.pat || null,
        }),
      )
    },
    [syncQueuedScopesState],
  )

  const refreshTrackedRepos = useCallback(
    async (opts?: { page?: number; perPage?: number; q?: string }) => {
      const page = opts?.page ?? githubPackagesTrackedReposPage
      const perPage = opts?.perPage ?? githubPackagesTrackedReposPerPage
      const q = (opts?.q ?? githubPackagesTrackedReposQ).trim()
      const [resp, jobs] = await Promise.all([
        listGitHubPackagesRepos({
          page,
          perPage,
          q: q ? q : null,
          selectedFilter: 'selected',
        }),
        listJobs(),
      ])
      setGitHubPackagesTrackedRepos(resp)
      const liveJob =
        jobs.find((job) => job.type === 'github_packages_webhook' && (job.status === 'running' || job.status === 'queued')) ??
        null
      setGhcrLiveJob(liveJob)

      // If a deletion makes the current page out-of-range, clamp to the last page.
      const maxPage = Math.max(1, Math.ceil(resp.filteredTotal / resp.perPage))
      if (resp.page > maxPage) setGitHubPackagesTrackedReposPage(maxPage)
    },
    [githubPackagesTrackedReposPage, githubPackagesTrackedReposPerPage, githubPackagesTrackedReposQ],
  )

  const refresh = useCallback(async () => {
    setError(null)
    const nextSettings = await getSettings()
    const nextNotifications = await getNotifications()
    const gh = await getGitHubPackagesSettings()
    const defaultCallbackUrl = (() => {
      if (typeof window === 'undefined') return ''
      const base = apiBaseUrl()
      const resolvedBase = new URL(base || window.location.origin, window.location.origin).toString().replace(/\/$/, '')
      return `${resolvedBase}/api/webhooks/github-packages`
    })()
    const callbackUrl = gh.callbackUrl || defaultCallbackUrl
    const nextGhcr = { ...gh, callbackUrl }
    const nextPat = gh.patMasked ?? ''

    setSettings(nextSettings)
    setNotifications(nextNotifications)
    setGitHubPackages(nextGhcr)
    setGitHubPackagesPat(nextPat)
    resetAutoSaveBaselines({
      backup: nextSettings.backup,
      notifications: nextNotifications,
      ghcr: {
        enabled: nextGhcr.enabled,
        callbackUrl: nextGhcr.callbackUrl,
        pat: nextPat,
        hasPersistedPat: Boolean(nextGhcr.patMasked),
      },
    })
  }, [resetAutoSaveBaselines])

  useEffect(() => {
    void (async () => {
      await refresh()
    })().catch((e: unknown) => setError(errorMessage(e)))
  }, [refresh])

  useEffect(() => {
    void refreshTrackedRepos().catch((e: unknown) => setError(errorMessage(e)))
  }, [refreshTrackedRepos])

  useEffect(() => {
    let closed = false
    let es: EventSource | null = null
    let refreshTimer: number | null = null
    let reconnectTimer: number | null = null
    let pollTimer: number | null = null
    let errorStreak = 0
    let lastEventId = 0

    const clearRefreshTimer = () => {
      if (refreshTimer != null) window.clearTimeout(refreshTimer)
      refreshTimer = null
    }
    const clearReconnectTimer = () => {
      if (reconnectTimer != null) window.clearTimeout(reconnectTimer)
      reconnectTimer = null
    }
    const stopPolling = () => {
      if (pollTimer != null) window.clearInterval(pollTimer)
      pollTimer = null
    }

    const scheduleRefresh = (delayMs: number) => {
      if (refreshTimer != null) return
      refreshTimer = window.setTimeout(() => {
        refreshTimer = null
        void refreshTrackedRepos().catch((e: unknown) => setError(errorMessage(e)))
      }, delayMs)
    }

    const startPolling = () => {
      if (pollTimer != null) return
      pollTimer = window.setInterval(() => {
        void refreshTrackedRepos().catch((e: unknown) => setError(errorMessage(e)))
      }, 10_000)
    }

    const trackEventId = (evt: Event) => {
      const idRaw = (evt as MessageEvent).lastEventId
      if (typeof idRaw !== 'string') return
      const parsed = Number.parseInt(idRaw, 10)
      if (Number.isFinite(parsed) && parsed > 0) lastEventId = parsed
    }

    const connect = () => {
      if (closed) return
      es = newJobsEventsSource(lastEventId > 0 ? { afterId: lastEventId } : undefined)

      es.addEventListener('open', () => {
        errorStreak = 0
        stopPolling()
        scheduleRefresh(0)
      })

      es.addEventListener('job_event', (evt: Event) => {
        trackEventId(evt)
        scheduleRefresh(250)
      })

      es.addEventListener('job_events_error', () => {
        scheduleRefresh(0)
      })

      es.onerror = () => {
        errorStreak += 1
        scheduleRefresh(0)
        if (errorStreak < 3) return
        es?.close()
        es = null
        startPolling()
        if (reconnectTimer != null) return
        reconnectTimer = window.setTimeout(() => {
          reconnectTimer = null
          connect()
        }, 3000)
      }
    }

    connect()

    return () => {
      closed = true
      clearRefreshTimer()
      clearReconnectTimer()
      stopPolling()
      es?.close()
    }
  }, [refreshTrackedRepos])

  useEffect(() => {
    onTopActions(
      <Button
        variant="primary"
        disabled={busy || !settings || !notifications || !githubPackages}
        onClick={() => {
          void (async () => {
            if (!settings || !notifications || !githubPackages) return
            setBusy(true)
            setError(null)
            try {
              await flushAutoSave()
            } catch (e: unknown) {
              setError(errorMessage(e))
            } finally {
              setBusy(false)
            }
          })()
        }}
      >
        立即重试保存全部
      </Button>,
    )
  }, [busy, flushAutoSave, githubPackages, notifications, onTopActions, settings])

  useEffect(() => {
    if (autoSavePhase !== 'saved') return
    const handle = window.setTimeout(() => {
      setAutoSavePhase((prev) => (prev === 'saved' ? 'idle' : prev))
    }, 1800)
    return () => window.clearTimeout(handle)
  }, [autoSavePhase, autoSaveUpdatedAt])

  const updateBackup = useCallback(
    (fieldPath: string, updater: (backup: SettingsResponse['backup']) => SettingsResponse['backup'], isToggle = false) => {
      setSettings((prev) => {
        if (!prev) return prev
        return { ...prev, backup: updater(prev.backup) }
      })
      markFieldDirty('backup', fieldPath, isToggle ? TOGGLE_DEBOUNCE_MS : TEXT_DEBOUNCE_MS)
    },
    [markFieldDirty],
  )

  const updateNotifications = useCallback(
    (fieldPath: string, updater: (current: NotificationConfig) => NotificationConfig, isToggle = false) => {
      setNotifications((prev) => {
        if (!prev) return prev
        return updater(prev)
      })
      markFieldDirty('notifications', fieldPath, isToggle ? TOGGLE_DEBOUNCE_MS : TEXT_DEBOUNCE_MS)
    },
    [markFieldDirty],
  )

  const updateGhcr = useCallback(
    (
      fieldPath: string,
      updater: (current: { enabled: boolean; callbackUrl: string; pat: string }) => {
        enabled: boolean
        callbackUrl: string
        pat: string
      },
      isToggle = false,
    ) => {
      if (!githubPackages) return
      const next = updater({
        enabled: githubPackages.enabled,
        callbackUrl: githubPackages.callbackUrl,
        pat: githubPackagesPat,
      })
      setGitHubPackages((prev) => (prev ? { ...prev, enabled: next.enabled, callbackUrl: next.callbackUrl } : prev))
      setGitHubPackagesPat(next.pat)
      markFieldDirty('ghcr', fieldPath, isToggle ? TOGGLE_DEBOUNCE_MS : TEXT_DEBOUNCE_MS)
    },
    [githubPackages, githubPackagesPat, markFieldDirty],
  )

  useEffect(() => {
    const timers = timersRef.current
    return () => {
      for (const timer of timers.values()) window.clearTimeout(timer)
      timers.clear()
      for (const waiter of waitersRef.current) {
        waiter.reject(new Error('页面已离开，自动保存已取消'))
      }
      waitersRef.current = []
    }
  }, [])

  const canWebPush = useMemo(() => {
    return typeof window !== 'undefined' && 'serviceWorker' in navigator && 'PushManager' in window
  }, [])

  async function ensureSubscription() {
    if (!notifications?.webPush.vapidPublicKey) throw new Error('请先在右侧配置 VAPID Public Key')
    if (!canWebPush) throw new Error('当前环境不支持 Web Push / Service Worker')

    const reg = await navigator.serviceWorker.register('/sw.js')
    const keyBytes = base64UrlToUint8Array(notifications.webPush.vapidPublicKey)
    const appServerKey = keyBytes.buffer.slice(
      keyBytes.byteOffset,
      keyBytes.byteOffset + keyBytes.byteLength,
    ) as ArrayBuffer
    const sub =
      (await reg.pushManager.getSubscription()) ??
      (await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: appServerKey,
      }))

    const json = sub.toJSON()
    if (!json.endpoint || !json.keys?.p256dh || !json.keys?.auth) throw new Error('Push subscription 缺少字段')
    await createWebPushSubscription({ endpoint: json.endpoint, keys: { p256dh: json.keys.p256dh, auth: json.keys.auth } })
    setWebPushEndpoint(json.endpoint)
  }

  async function removeSubscription() {
    if (!canWebPush) throw new Error('当前环境不支持 Web Push / Service Worker')
    const reg = await navigator.serviceWorker.ready
    const sub = await reg.pushManager.getSubscription()
    if (!sub) return
    const endpoint = sub.endpoint
    await sub.unsubscribe()
    await deleteWebPushSubscription(endpoint)
    setWebPushEndpoint(null)
  }

  if (!settings || !notifications || !githubPackages) {
    return <div className="muted">加载中…</div>
  }

  const githubPackagesTrackedMaxPage = githubPackagesTrackedRepos
    ? Math.max(1, Math.ceil(githubPackagesTrackedRepos.filteredTotal / githubPackagesTrackedRepos.perPage))
    : 1

  const autoSaveStatusText =
    autoSavePhase === 'saving'
      ? `自动保存中：${mapScopeLabel(autoSaveSavingScope ?? 'backup')}`
      : autoSavePhase === 'queued'
        ? autoSaveQueuedScopes.length
          ? `自动保存排队中：${autoSaveQueuedScopes.map(mapScopeLabel).join('、')}`
          : '自动保存排队中'
        : autoSavePhase === 'saved'
          ? autoSaveUpdatedAt
            ? `已自动保存（${new Date(autoSaveUpdatedAt).toLocaleTimeString()}）`
            : '已自动保存'
          : autoSavePhase === 'error'
            ? '自动保存失败'
            : '自动保存已就绪'

  const ghcrPatIssue = autoSaveIssue?.scope === 'ghcr' && autoSaveIssue.fieldPath.includes('pat') ? autoSaveIssue : null
  const ghcrLiveProgressText = (() => {
    if (!ghcrLiveJob) return null
    const p = ghcrLiveJob.progress
    const parts: string[] = [`job ${ghcrLiveJob.id}`, ghcrLiveJob.status]
    if (p) {
      parts.push(`${p.phase}`)
      parts.push(`${p.current}/${p.total || '-'}`)
      if (typeof p.percent === 'number' && Number.isFinite(p.percent)) {
        parts.push(`${Math.max(0, Math.min(100, Math.round(p.percent)))}%`)
      }
      if (p.currentTarget) parts.push(p.currentTarget)
      if (p.message) parts.push(p.message)
    }
    return parts.join(' · ')
  })()

  const autoSaveToastClassName =
    autoSavePhase === 'error'
      ? 'autoSaveToast autoSaveToastBad'
      : autoSavePhase === 'saving' || autoSavePhase === 'queued'
        ? 'autoSaveToast autoSaveToastWarn'
        : 'autoSaveToast autoSaveToastOk'

  const showAutoSaveToast = autoSavePhase !== 'idle'

  return (
    <div className="page">
      <div className="twoCol">
        <div className="settingsCol">
          <div className="card">
            <div className="title">鉴权（Forward Header）</div>
            <div className="muted">单用户：由反向代理注入 Header；本服务信任来源（运行时只读）</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">Header 名称</div>
                <div className="mono">{settings.auth.forwardHeaderName}</div>
              </div>

              <div className="kvRow">
                <div className="label">允许匿名（开发环境）</div>
                <div className="muted">{settings.auth.allowAnonymousInDev ? 'on' : 'off'}</div>
              </div>

              <div className="kvRow">
                <div className="label">当前用户展示</div>
                <div className="mono">ivan</div>
              </div>
              <div className="muted" style={{ marginTop: 6 }}>
                该区域由启动配置控制：`DOCKREV_AUTH_FORWARD_HEADER_NAME` /
                `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV`，修改后需重启服务生效。
              </div>
            </div>
          </div>

          <div className="card">
            <div className="title">自我升级</div>
            <div className="muted">Dockrev 更新 Dockrev：由独立 supervisor 提供页面与执行者（默认 {selfUpgradeUrl}）</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">Supervisor 状态</div>
                <div className="muted">
                  {supervisor.state.status === 'ok'
                    ? `ok (${supervisor.state.okAt})`
                    : supervisor.state.status === 'checking'
                      ? 'checking…'
                      : supervisor.state.status === 'offline'
                        ? `offline (${supervisor.state.errorAt})`
                        : 'unknown'}
                </div>
              </div>
              {supervisor.state.status === 'offline' ? (
                <div className="kvRow">
                  <div className="label">原因</div>
                  <div className="muted">
                    <Mono>{supervisor.state.error}</Mono>
                  </div>
                </div>
              ) : null}
            </div>

            <div className="formActions">
              <Button
                variant="primary"
                disabled={busy || supervisor.state.status !== 'ok'}
                title={supervisor.state.status === 'offline' ? '自我升级不可用（supervisor offline）' : undefined}
                onClick={() => {
                  window.location.href = selfUpgradeUrl
                }}
              >
                打开自我升级
              </Button>
              <Button
                variant="ghost"
                disabled={busy || supervisor.state.status === 'checking'}
                onClick={() => {
                  void supervisor.check()
                }}
              >
                重试
              </Button>
            </div>
          </div>

          <div className="card">
            <div className="title">部署检查</div>
            <div className="muted">手动打开部署检查清单页，不会修改“自动打开”偏好。</div>

            <div className="formActions">
              <Button
                variant="primary"
                disabled={busy}
                onClick={() => {
                  navigate({ name: 'deploy-check' })
                }}
              >
                打开部署检查页
              </Button>
            </div>
          </div>

          <div className="card">
            <div className="title">备份默认策略</div>
            <div className="muted">默认 fail-closed；目标过大可按阈值跳过（force 可覆盖）</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">启用更新前备份</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={settings.backup.enabled}
                    disabled={busy}
                    onChange={(v) =>
                      updateBackup('backup.enabled', (backup) => ({ ...backup, enabled: v }), true)
                    }
                  />
                  <div className="muted">{settings.backup.enabled ? 'on' : 'off'}</div>
                </div>
              </div>
              <div className="kvRow">
                <div className="label">要求备份成功（fail-closed）</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={settings.backup.requireSuccess}
                    disabled={busy}
                    onChange={(v) =>
                      updateBackup('backup.requireSuccess', (backup) => ({ ...backup, requireSuccess: v }), true)
                    }
                  />
                  <div className="muted">{settings.backup.requireSuccess ? 'on' : 'off'}</div>
                </div>
              </div>
              <div className="kvRow">
                <div className="label">备份输出目录</div>
                <input
                  className="input"
                  value={settings.backup.baseDir}
                  onChange={(e) =>
                    updateBackup('backup.baseDir', (backup) => ({ ...backup, baseDir: e.target.value }))
                  }
                />
              </div>
              <div className="kvRow">
                <div className="label">体积阈值（超过则跳过）</div>
                <div>
                  <input
                    className="input"
                    value={String(settings.backup.skipTargetsOverBytes)}
                    onChange={(e) =>
                      updateBackup('backup.skipTargetsOverBytes', (backup) => ({
                        ...backup,
                        skipTargetsOverBytes: Number(e.target.value) || 0,
                      }))
                    }
                  />
                  <div className="muted" style={{ marginTop: 6 }}>
                    当前：{formatBytes(settings.backup.skipTargetsOverBytes)}
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div className="card">
            <div className="title">通知</div>
            <div className="muted">事件：发现更新 / 版本提示 / 更新成功 / 更新失败 / 备份失败</div>

            <div className="settingsSection">
              <div className="settingHead">
                <div className="sectionTitle">Email</div>
                <Switch
                  checked={notifications.email.enabled}
                  disabled={busy}
                  onChange={(v) =>
                    updateNotifications(
                      'notifications.email.enabled',
                      (current) => ({ ...current, email: { ...current.email, enabled: v } }),
                      true,
                    )
                  }
                />
              </div>
              <div className="kv">
                <div className="kvRow">
                  <div className="label">SMTP URL</div>
                  <input
                    className="input"
                    value={notifications.email.smtpUrl ?? ''}
                    onChange={(e) =>
                      updateNotifications('notifications.email.smtpUrl', (current) => ({
                        ...current,
                        email: { ...current.email, smtpUrl: e.target.value },
                      }))
                    }
                    placeholder="smtp://user:pass@smtp.example.com:587"
                  />
                </div>
              </div>
            </div>

            <div className="settingsSection">
              <div className="settingHead">
                <div className="sectionTitle">Webhook</div>
                <Switch
                  checked={notifications.webhook.enabled}
                  disabled={busy}
                  onChange={(v) =>
                    updateNotifications(
                      'notifications.webhook.enabled',
                      (current) => ({ ...current, webhook: { ...current.webhook, enabled: v } }),
                      true,
                    )
                  }
                />
              </div>
              <div className="kv">
                <div className="kvRow">
                  <div className="label">URL</div>
                  <input
                    className="input"
                    value={notifications.webhook.url ?? ''}
                    onChange={(e) =>
                      updateNotifications('notifications.webhook.url', (current) => ({
                        ...current,
                        webhook: { ...current.webhook, url: e.target.value },
                      }))
                    }
                    placeholder="https://hooks.example.com/dockrev"
                  />
                </div>
              </div>
            </div>

            <div className="settingsSection">
              <div className="settingHead">
                <div className="sectionTitle">Telegram</div>
                <Switch
                  checked={notifications.telegram.enabled}
                  disabled={busy}
                  onChange={(v) =>
                    updateNotifications(
                      'notifications.telegram.enabled',
                      (current) => ({ ...current, telegram: { ...current.telegram, enabled: v } }),
                      true,
                    )
                  }
                />
              </div>
              <div className="kv">
                <div className="kvRow">
                  <div className="label">Bot token</div>
                  <input
                    className="input"
                    value={notifications.telegram.botToken ?? ''}
                    onChange={(e) =>
                      updateNotifications('notifications.telegram.botToken', (current) => ({
                        ...current,
                        telegram: { ...current.telegram, botToken: e.target.value },
                      }))
                    }
                  />
                </div>
                <div className="kvRow">
                  <div className="label">Chat id</div>
                  <input
                    className="input"
                    value={notifications.telegram.chatId ?? ''}
                    onChange={(e) =>
                      updateNotifications('notifications.telegram.chatId', (current) => ({
                        ...current,
                        telegram: { ...current.telegram, chatId: e.target.value },
                      }))
                    }
                  />
                </div>
              </div>
            </div>

            <div className="settingsSection">
              <div className="settingHead">
                <div className="sectionTitle">Web Push（Chrome / VAPID）</div>
                <Switch
                  checked={notifications.webPush.enabled}
                  disabled={busy}
                  onChange={(v) =>
                    updateNotifications(
                      'notifications.webPush.enabled',
                      (current) => ({ ...current, webPush: { ...current.webPush, enabled: v } }),
                      true,
                    )
                  }
                />
              </div>

              <div className="kv">
                <div className="kvRow">
                  <div className="label">Public Key</div>
                  <input
                    className="input"
                    value={notifications.webPush.vapidPublicKey ?? ''}
                    onChange={(e) =>
                      updateNotifications('notifications.webPush.vapidPublicKey', (current) => ({
                        ...current,
                        webPush: { ...current.webPush, vapidPublicKey: e.target.value },
                      }))
                    }
                  />
                </div>
                <div className="kvRow">
                  <div className="label">Private Key（留空=保持原值）</div>
                  <input
                    className="input"
                    value={notifications.webPush.vapidPrivateKey ?? ''}
                    onChange={(e) =>
                      updateNotifications('notifications.webPush.vapidPrivateKey', (current) => ({
                        ...current,
                        webPush: { ...current.webPush, vapidPrivateKey: e.target.value },
                      }))
                    }
                  />
                </div>
                <div className="kvRow">
                  <div className="label">Subject</div>
                  <input
                    className="input"
                    value={notifications.webPush.vapidSubject ?? ''}
                    onChange={(e) =>
                      updateNotifications('notifications.webPush.vapidSubject', (current) => ({
                        ...current,
                        webPush: { ...current.webPush, vapidSubject: e.target.value },
                      }))
                    }
                  />
                </div>
              </div>

              <div className="formActions" style={{ marginTop: 10 }}>
                <Button
                  variant="ghost"
                  disabled={busy}
                  onClick={() => {
                    void (async () => {
                      setBusy(true)
                      setError(null)
                      try {
                        await testNotifications('dockrev: test notification')
                      } catch (e: unknown) {
                        setError(errorMessage(e))
                      } finally {
                        setBusy(false)
                      }
                    })()
                  }}
                >
                  发送测试通知
                </Button>
                <Button
                  variant="ghost"
                  disabled={busy || !canWebPush}
                  onClick={() => {
                    void (async () => {
                      setBusy(true)
                      setError(null)
                      try {
                        await ensureSubscription()
                      } catch (e: unknown) {
                        setError(errorMessage(e))
                      } finally {
                        setBusy(false)
                      }
                    })()
                  }}
                  title={canWebPush ? '当前浏览器订阅 Web Push' : '当前环境不支持'}
                >
                  订阅本浏览器
                </Button>
                <Button
                  variant="ghost"
                  disabled={busy || !canWebPush}
                  onClick={() => {
                    void (async () => {
                      setBusy(true)
                      setError(null)
                      try {
                        await removeSubscription()
                      } catch (e: unknown) {
                        setError(errorMessage(e))
                      } finally {
                        setBusy(false)
                      }
                    })()
                  }}
                >
                  取消订阅
                </Button>
              </div>

              {webPushEndpoint ? (
                <div className="muted" style={{ marginTop: 10 }}>
                  endpoint <Mono>{webPushEndpoint.slice(0, 40)}…</Mono>
                </div>
              ) : null}
            </div>

            {error ? <div className="error">{error}</div> : null}
          </div>

          {error ? <div className="error">{error}</div> : null}
        </div>

        <div className="settingsCol">
          <div className="card">
          <div className="title">GitHub Packages（GHCR）Webhook</div>
          <div className="muted">在 GHCR 发布新版本时自动触发 Dockrev 扫描（事件：package.published）</div>
          <div className="muted">添加后会自动创建后台任务注册 webhook；可在更新队列 / GHCR Webhook 页面查看进度。</div>
          {ghcrLiveProgressText ? (
            <div className="muted" style={{ marginTop: 8, display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
              <span>当前 GHCR 任务：{ghcrLiveProgressText}</span>
              <Button variant="ghost" onClick={() => navigate({ name: 'ghcr-webhooks' })}>
                打开 GHCR Webhook 页面
              </Button>
            </div>
          ) : null}

          <div className="settingsSection">
            <div className="settingHead">
              <div className="sectionTitle">启用</div>
              <Switch
                checked={githubPackages.enabled}
                disabled={busy}
                onChange={(v) =>
                  updateGhcr('ghcr.enabled', (current) => ({ ...current, enabled: v }), true)
                }
              />
            </div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">GitHub PAT（留空=保持原值）</div>
                <input
                  className={ghcrPatIssue ? 'input inputError' : 'input'}
                  value={githubPackagesPat}
                  onChange={(e) =>
                    updateGhcr('ghcr.pat', (current) => ({ ...current, pat: e.target.value }))
                  }
                  placeholder="ghp_..."
                />
              </div>

              <div className="kvRow">
                <div className="label">Callback URL</div>
                <input
                  className="input"
                  value={githubPackages.callbackUrl}
                  onChange={(e) =>
                    updateGhcr('ghcr.callbackUrl', (current) => ({ ...current, callbackUrl: e.target.value }))
                  }
                  placeholder="https://dockrev.example.com/api/webhooks/github-packages"
                />
              </div>
            </div>
          </div>

          <div className="settingsSection">
            <div className="settingHead">
              <div className="sectionTitle">Repos</div>
              <div className="muted">{githubPackages.reposSelectedTotal} 个</div>
            </div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">添加 Repo</div>
                <div style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
                  <input
                    className="input"
                    value={githubPackagesNewRepo}
                    onChange={(e) => setGitHubPackagesNewRepo(e.target.value)}
                    placeholder="https://github.com/org/repo 或 org/repo；也可粘贴 profile/org URL 批量选择"
                    style={{ flex: 1 }}
                  />
                  <Button
                    variant="ghost"
                    disabled={busy || ghcrResolvePending || !githubPackagesNewRepo.trim()}
                    onClick={() => {
                      if (busy || ghcrResolvePending) return
                      setGhcrResolvePending(true)
                      void (async () => {
                        setBusy(true)
                        setError(null)
                        try {
                          const input = githubPackagesNewRepo.trim()
                          if (!input) throw new Error('请先输入 owner/repo 或 profile 链接')
                          await flushAutoSave(['ghcr'])
                          const resolved = await resolveGitHubPackagesTarget(input)
                          if (resolved.kind === 'repo') {
                            const fullName = resolved.repos[0]?.fullName?.trim() ?? ''
                            if (!fullName) throw new Error('resolve returned empty repo')
                            await setGitHubPackagesRepoSelected({ fullName, selected: true })
                            setGitHubPackagesNewRepo('')
                            await refresh()
                            await refreshTrackedRepos()
                            return
                          }
                          if (resolved.kind === 'owner') {
                            let picked: Array<{ fullName: string; selected: boolean }> = resolved.repos.map((r) => ({
                              fullName: r.fullName,
                              selected: r.selected,
                            }))
                            const ok = await confirm({
                              title: '选择要跟踪的仓库',
                              body: (
                                <GitHubPackagesRepoPicker
                                  initial={resolved}
                                  onChange={(next) => {
                                    picked = next
                                  }}
                                />
                              ),
                              cardClassName: 'ghcrPickerDialogCard',
                              bodyClassName: 'ghcrPickerDialogBody',
                              confirmText: '确认',
                              cancelText: '取消',
                              confirmVariant: 'primary',
                              badgeText: null,
                            })
                            if (!ok) return
                            // Apply both selections and deselections, but only for repos whose
                            // selection state changed in the picker to reduce API calls.
                            const before = new Map(resolved.repos.map((r) => [r.fullName, r.selected] as const))
                            const changed = picked.filter((r) => before.get(r.fullName) !== r.selected)
                            for (const r of changed) {
                              await setGitHubPackagesRepoSelected({ fullName: r.fullName, selected: r.selected })
                            }
                            setGitHubPackagesNewRepo('')
                            await refresh()
                            await refreshTrackedRepos()
                            return
                          }
                          throw new Error(`unsupported resolve kind: ${resolved.kind}`)
                        } catch (e: unknown) {
                          setError(mapResolveFailure(e))
                        } finally {
                          setBusy(false)
                          setGhcrResolvePending(false)
                        }
                      })()
                    }}
                  >
                    {ghcrResolvePending ? (
                      <span className="btnInlineLoading">
                        <span className="btnInlineSpinner" aria-hidden="true" />
                        <span>解析中…</span>
                      </span>
                    ) : (
                      '解析并添加'
                    )}
                  </Button>
                </div>
              </div>
            </div>

            {githubPackagesTrackedRepos ? (
              <div style={{ marginTop: 10 }}>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10, alignItems: 'center' }}>
                  <input
                    className="input"
                    value={githubPackagesTrackedReposQInput}
                    onChange={(e) => setGitHubPackagesTrackedReposQInput(e.target.value)}
                    placeholder="搜索 owner/repo"
                    disabled={busy}
                    style={{ flex: '1 1 260px', minWidth: 220 }}
                  />
                  <select
                    className="select"
                    value={githubPackagesTrackedReposPerPage}
                    disabled={busy}
                    onChange={(e) => {
                      const next = Math.max(1, Number(e.target.value) || 50)
                      setGitHubPackagesTrackedReposPerPage(next)
                      setGitHubPackagesTrackedReposPage(1)
                    }}
                  >
                    <option value={20}>20/页</option>
                    <option value={50}>50/页</option>
                    <option value={100}>100/页</option>
                    <option value={200}>200/页</option>
                  </select>
                  <Button variant="ghost" onClick={() => navigate({ name: 'ghcr-webhook-inbox' })}>
                    收件箱
                  </Button>
                </div>

                {githubPackagesTrackedRepos.repos.length ? (
                  <div
                    style={{
                      marginTop: 10,
                      maxHeight: 420,
                      overflowY: 'auto',
                      paddingRight: 6,
                      overscrollBehavior: 'contain',
                      display: 'flex',
                      flexDirection: 'column',
                      gap: 10,
                    }}
                    >
                    {githubPackagesTrackedRepos.repos.map((r) => {
                      const state = normalizeWebhookState(r.webhookState)
                      const dotClass = webhookStateDotClass(state)
                      const lastSync = r.lastSyncAt ? r.lastSyncAt : '-'
                      const lastAudit = r.lastAuditAt ? r.lastAuditAt : '-'
                      const hookId = r.hookId ? String(r.hookId) : '-'
                      const isInFlight = state === 'queued' || state === 'running'
                      const isUnregisterInFlight = isInFlight && (r.lastOp ?? '') === 'unregister'
                      const showRetryDelete = state === 'error' && (r.lastOp ?? '') === 'unregister'
                      const showRetryRegister =
                        state === 'missing' || state === 'conflict' || (state === 'error' && !showRetryDelete)
                      return (
                        <div
                          key={r.fullName}
                          style={{
                            display: 'flex',
                            gap: 10,
                            alignItems: 'center',
                            justifyContent: 'space-between',
                            minWidth: 0,
                          }}
                        >
                          <div style={{ minWidth: 0, flex: '1 1 auto' }}>
                            <div style={{ display: 'flex', gap: 10, alignItems: 'center', minWidth: 0 }}>
                              <span className={dotClass} />
                              <div className="mono" style={{ overflowWrap: 'anywhere' }}>
                                {r.fullName}
                              </div>
                            </div>
                            <div className="muted" style={{ marginTop: 4, overflowWrap: 'anywhere' }}>
                              状态: {webhookStateLabel(state)} · hookId: {hookId} · lastSyncAt: {lastSync} · lastAuditAt:{' '}
                              {lastAudit}
                              {r.lastError ? ` · lastError: ${r.lastError}` : null}
                            </div>
                            {state === 'conflict' ? (
                              <div className="muted" style={{ marginTop: 4 }}>
                                检测到重复 webhook，请先到 GitHub 手工删除重复项后再点“重试注册”。
                              </div>
                            ) : null}
                          </div>

                          <div style={{ display: 'flex', gap: 10, alignItems: 'center', flex: '0 0 auto' }}>
                            {isInFlight && r.webhookJobId ? (
                              <Button variant="ghost" onClick={() => navigate({ name: 'job', jobId: r.webhookJobId! })}>
                                查看任务
                              </Button>
                            ) : null}

                            {showRetryRegister ? (
                              <Button
                                variant="ghost"
                                disabled={busy}
                                onClick={() => {
                                  void (async () => {
                                    setBusy(true)
                                    setError(null)
                                    try {
                                      await flushAutoSave(['ghcr'])
                                      await setGitHubPackagesRepoSelected({ fullName: r.fullName, selected: true })
                                      await refreshTrackedRepos()
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
                                disabled={busy}
                                onClick={() => {
                                  void (async () => {
                                    setBusy(true)
                                    setError(null)
                                    try {
                                      await deleteGitHubPackagesRepo({ fullName: r.fullName })
                                      await refreshTrackedRepos()
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

                            <IconButton
                              variant="danger"
                              title="删除"
                              disabled={busy || isUnregisterInFlight}
                              onClick={() => {
                                void (async () => {
                                  const ok = await confirm({
                                    title: '删除跟踪仓库',
                                    body: (
                                      <div>
                                        <div className="modalLead">将反注册 webhook，并从列表中移除该仓库：</div>
                                        <div className="modalKvGrid">
                                          <div className="modalKvLabel">Repo</div>
                                          <div className="modalKvValue">
                                            <Mono>{r.fullName}</Mono>
                                          </div>
                                        </div>
                                      </div>
                                    ),
                                    confirmText: '删除',
                                    cancelText: '取消',
                                    confirmVariant: 'danger',
                                    badgeText: '将删除 webhook',
                                    badgeTone: 'bad',
                                  })
                                  if (!ok) return
                                  setBusy(true)
                                  setError(null)
                                  try {
                                    await deleteGitHubPackagesRepo({ fullName: r.fullName })
                                    await refreshTrackedRepos()
                                  } catch (e: unknown) {
                                    setError(errorMessage(e))
                                  } finally {
                                    setBusy(false)
                                  }
                                })()
                              }}
                            >
                              <TrashIcon className="uiIcon" />
                            </IconButton>
                          </div>
                        </div>
                      )
                    })}
                  </div>
                ) : (
                  <div className="muted" style={{ marginTop: 10 }}>
                    暂无已跟踪仓库
                  </div>
                )}

                <div className="formActions" style={{ marginTop: 10, justifyContent: 'space-between' }}>
                  <div className="muted">
                    第 {githubPackagesTrackedRepos.page} 页（每页 {githubPackagesTrackedRepos.perPage}）
                  </div>
                  <div style={{ display: 'flex', gap: 10 }}>
                    <Button
                      variant="ghost"
                      disabled={busy || githubPackagesTrackedRepos.page <= 1}
                      onClick={() => setGitHubPackagesTrackedReposPage((p) => Math.max(1, p - 1))}
                    >
                      上一页
                    </Button>
                    <Button
                      variant="ghost"
                      disabled={busy || githubPackagesTrackedRepos.page >= githubPackagesTrackedMaxPage}
                      onClick={() =>
                        setGitHubPackagesTrackedReposPage((p) => Math.min(githubPackagesTrackedMaxPage, p + 1))
                      }
                    >
                      下一页
                    </Button>
                  </div>
                </div>
              </div>
            ) : (
              <div className="muted" style={{ marginTop: 10 }}>
                加载中…
              </div>
            )}
          </div>

          {error ? <div className="error">{error}</div> : null}
          </div>
        </div>
      </div>
      {showAutoSaveToast ? (
        <div className={autoSaveToastClassName} role="status" aria-live="polite">
          <div>{autoSaveStatusText}</div>
          {autoSavePhase === 'error' && autoSaveIssue ? (
            <div className="autoSaveToastDetail">{autoSaveIssue.message}</div>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}
