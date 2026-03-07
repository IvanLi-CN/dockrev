import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Icon } from '@iconify/react'
import eyeOffOutline from '@iconify-icons/mdi/eye-off-outline'
import eyeOutline from '@iconify-icons/mdi/eye-outline'
import {
  ApiError,
  createWebPushSubscription,
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
  type NotificationTestChannel,
  type ResolveGitHubPackagesTargetResponse,
  type NotificationConfig,
  type PutSettingsInput,
  type SettingsResponse,
} from '../api'
import {
  NotificationChannelCard,
  type NotificationChannelTestState,
} from '../components/NotificationChannelCard'
import { Button, Mono, Switch } from '../ui'
import { useConfirm } from '../confirm'
import { selfUpgradeBaseUrl } from '../runtimeConfig'
import { useSupervisorHealth } from '../useSupervisorHealth'
import { webhookStateDotClass, webhookStateIcon } from '../webhookStatus'
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

function buildSettingsSavePayload(settings: SettingsResponse): PutSettingsInput {
  return {
    backup: settings.backup,
    resourceMonitor: {
      enabled: settings.resourceMonitor.enabled,
      sampleIntervalSeconds: settings.resourceMonitor.sampleIntervalSeconds,
    },
    schedules: settings.schedules,
    instance: {
      // Server treats empty string as "clear".
      publicBaseUrl: settings.instance.publicBaseUrl ?? '',
    },
  }
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
const TELEGRAM_BOT_TOKEN_MASK = '••••••••••••••••'
const TELEGRAM_BOT_TOKEN_PATTERN = /^\d{5,}:[A-Za-z0-9_-]{8,}$/
const GHCR_PREVIEW_LIMIT = 6
const GITHUB_PAT_PREFIXES = ['ghp_', 'github_pat_', 'gho_', 'ghu_', 'ghs_', 'ghr_']
const GHCR_JOB_TYPES = new Set([
  'github_packages_webhook',
  'github_packages_webhook_sync_all',
  'github_packages_webhook_sync_repo',
])
const NOTIFICATION_CHANNEL_LABEL: Record<NotificationTestChannel, string> = {
  email: 'Email',
  webhook: 'Webhook',
  telegram: 'Telegram',
  webPush: 'Web Push',
}
const DEFAULT_NOTIFICATION_EVENTS = {
  update: true,
  newVersion: true,
  ghcrWebhookAnomaly: true,
} as const

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

function isMaskedTelegramBotToken(value: string): boolean {
  return value === TELEGRAM_BOT_TOKEN_MASK || value === PAT_MASK
}

function normalizeNotificationEvents(
  events: NotificationConfig['events'] | undefined,
): NonNullable<NotificationConfig['events']> {
  return {
    update: events?.update ?? DEFAULT_NOTIFICATION_EVENTS.update,
    newVersion: events?.newVersion ?? DEFAULT_NOTIFICATION_EVENTS.newVersion,
    ghcrWebhookAnomaly: events?.ghcrWebhookAnomaly ?? DEFAULT_NOTIFICATION_EVENTS.ghcrWebhookAnomaly,
  }
}

function normalizeNotificationsForUi(input: NotificationConfig): NotificationConfig {
  const rawBotToken = input.telegram.botToken ?? ''
  const hasExplicitBotToken = rawBotToken.trim().length > 0 && !isMaskedTelegramBotToken(rawBotToken)
  const botTokenConfigured = input.telegram.botTokenConfigured ?? hasExplicitBotToken
  const botToken = hasExplicitBotToken ? rawBotToken : botTokenConfigured ? TELEGRAM_BOT_TOKEN_MASK : null
  return {
    ...input,
    telegram: {
      ...input.telegram,
      botToken,
      botTokenConfigured,
    },
    events: normalizeNotificationEvents(input.events),
  }
}

function normalizeNotificationsForSave(input: NotificationConfig): NotificationConfig {
  const botTokenRaw = input.telegram.botToken ?? ''
  const botToken =
    isMaskedTelegramBotToken(botTokenRaw) || botTokenRaw.trim().length === 0 ? null : botTokenRaw
  const chatIdRaw = input.telegram.chatId ?? ''
  const chatId = chatIdRaw.trim().length === 0 ? '' : chatIdRaw.trim()
  return {
    ...input,
    telegram: {
      ...input.telegram,
      botToken,
      chatId,
    },
    events: normalizeNotificationEvents(input.events),
  }
}

function validateNotificationsBeforeSave(
  input: NotificationConfig,
): { fieldPath: string; reason: string; message: string } | null {
  const rawBotToken = input.telegram.botToken ?? ''
  const trimmedBotToken = rawBotToken.trim()
  if (trimmedBotToken.length === 0 || isMaskedTelegramBotToken(trimmedBotToken)) return null

  if (/\s/.test(rawBotToken) || !TELEGRAM_BOT_TOKEN_PATTERN.test(trimmedBotToken)) {
    return {
      fieldPath: 'notifications.telegram.botToken',
      reason: 'telegram_bot_token_invalid',
      message: 'Bot token 格式不合法，请填写形如 123456:AA... 的 Telegram Bot token',
    }
  }

  return null
}

function isGhcrLiveJob(job: JobListItem): boolean {
  if (!GHCR_JOB_TYPES.has(job.type)) return false
  return job.status === 'running' || job.status === 'queued'
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

function readField(details: unknown): string | null {
  if (!details || typeof details !== 'object') return null
  const field = (details as Record<string, unknown>).field
  return typeof field === 'string' ? field : null
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
  if (scope === 'backup') return '系统设置'
  if (scope === 'notifications') return '通知'
  return 'GHCR'
}

function runningTestState(channel: NotificationTestChannel): NotificationChannelTestState {
  const label = NOTIFICATION_CHANNEL_LABEL[channel]
  return {
    phase: 'running',
    steps: [
      { tone: 'running', text: `正在发送 ${label} 测试消息` },
      { tone: 'info', text: '等待渠道响应' },
    ],
    updatedAt: new Date().toISOString(),
  }
}

function successTestState(channel: NotificationTestChannel): NotificationChannelTestState {
  const label = NOTIFICATION_CHANNEL_LABEL[channel]
  return {
    phase: 'success',
    steps: [
      { tone: 'success', text: `${label} 测试请求已发送` },
      { tone: 'success', text: `${label} 渠道返回成功` },
    ],
    updatedAt: new Date().toISOString(),
  }
}

function errorTestState(
  channel: NotificationTestChannel,
  detail: string,
  options?: { requestSent?: boolean },
): NotificationChannelTestState {
  const label = NOTIFICATION_CHANNEL_LABEL[channel]
  const requestSent = options?.requestSent ?? true
  return {
    phase: 'error',
    steps: [
      requestSent
        ? { tone: 'success', text: `${label} 测试请求已发出` }
        : { tone: 'error', text: `${label} 测试请求发送失败` },
      { tone: 'error', text: `${label} 渠道测试失败` },
      { tone: 'error', text: '查看详细错误信息' },
    ],
    updatedAt: new Date().toISOString(),
    errorDetail: detail,
  }
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
  const [telegramBotTokenVisible, setTelegramBotTokenVisible] = useState(false)
  const [telegramBotTokenTouched, setTelegramBotTokenTouched] = useState(false)
  const [telegramBotTokenFocused, setTelegramBotTokenFocused] = useState(false)
  const [githubPackages, setGitHubPackages] = useState<GitHubPackagesSettingsResponse | null>(null)
  const [githubPackagesPat, setGitHubPackagesPat] = useState('')
  const [githubPackagesNewRepo, setGitHubPackagesNewRepo] = useState('')
  const [githubPackagesTrackedRepos, setGitHubPackagesTrackedRepos] = useState<ListGitHubPackagesReposResponse | null>(null)
  const [ghcrLiveJob, setGhcrLiveJob] = useState<JobListItem | null>(null)
  const [ghcrResolvePending, setGhcrResolvePending] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [webPushEndpoint, setWebPushEndpoint] = useState<string | null>(null)
  const [autoSavePhase, setAutoSavePhase] = useState<AutoSavePhase>('idle')
  const [autoSaveIssue, setAutoSaveIssue] = useState<AutoSaveIssue | null>(null)
  const [autoSaveSavingScope, setAutoSaveSavingScope] = useState<SaveScope | null>(null)
  const [autoSaveUpdatedAt, setAutoSaveUpdatedAt] = useState<string | null>(null)
  const [autoSaveQueuedScopes, setAutoSaveQueuedScopes] = useState<SaveScope[]>([])
  const [notificationTestStates, setNotificationTestStates] = useState<
    Partial<Record<NotificationTestChannel, NotificationChannelTestState>>
  >({})
  const [notificationTestRunning, setNotificationTestRunning] = useState<
    Partial<Record<NotificationTestChannel, boolean>>
  >({})

  const settingsRef = useRef<SettingsResponse | null>(null)
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
    settingsRef.current = settings
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
    if (scope === 'backup') {
      if (!settingsRef.current) return null
      return buildSettingsSavePayload(settingsRef.current)
    }
    if (scope === 'notifications') {
      if (!notificationsRef.current) return null
      return normalizeNotificationsForSave(notificationsRef.current)
    }
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
      await putSettings(payload as PutSettingsInput)
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
    let apiField: string | null = null
    if (e instanceof ApiError) {
      reason = readReason(e.details)
      apiField = readField(e.details)
    }
    if (!reason && scope === 'ghcr') reason = 'ghcr_pat_unsaved_or_save_failed'

    const fallback = errorMessage(e)
    let message = `自动保存失败（${mapScopeLabel(scope)}）：${fallback}`
    if (reason === 'cron_invalid') message = 'Cron 表达式不合法，请检查格式（5 段或 6/7 段）'
    else if (reason === 'ghcr_pat_missing') message = '请先填写 GitHub PAT'
    else if (reason === 'ghcr_pat_format_invalid') message = 'PAT 格式不合法，请使用 ghp_ / github_pat_ 等 GitHub token'
    else if (reason === 'ghcr_pat_unsaved_or_save_failed') message = 'PAT 未保存成功，无法解析，请检查网络后重试'
    else if (reason === 'ghcr_pat_invalid_or_scope_insufficient') message = 'PAT 无效或权限不足，请检查 token scope'
    else if (reason === 'telegram_bot_token_invalid') message = 'Bot token 格式不合法，请填写形如 123456:AA... 的 Telegram Bot token'
    else if (reason === 'instance_public_base_url_invalid')
      message = '实例 Public Base URL 格式不合法，请填写 http(s) 的绝对 URL，例如 https://dockrev.example.com/'
    else if (reason === 'github_upstream_timeout') message = 'GitHub 响应超时，请稍后重试'
    else if (reason === 'github_upstream_unavailable') message = 'GitHub 请求失败，请稍后重试'

    return {
      scope,
      fieldPath: apiField ?? fieldPath,
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

        if (scope === 'notifications') {
          const currentNotifications = notificationsRef.current
          if (currentNotifications) {
            const precheckIssue = validateNotificationsBeforeSave(currentNotifications)
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
      settings: SettingsResponse
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

      settingsRef.current = next.settings
      notificationsRef.current = next.notifications
      ghcrRef.current = next.ghcr
      lastSavedHashRef.current.set('backup', JSON.stringify(buildSettingsSavePayload(next.settings)))
      lastSavedHashRef.current.set('notifications', JSON.stringify(normalizeNotificationsForSave(next.notifications)))
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

  const refreshTrackedRepos = useCallback(async () => {
    const [resp, jobs] = await Promise.all([
      listGitHubPackagesRepos({
        page: 1,
        perPage: GHCR_PREVIEW_LIMIT,
        selectedFilter: 'selected',
      }),
      listJobs(),
    ])
    setGitHubPackagesTrackedRepos(resp)
    const liveJob =
      jobs.find((job) => isGhcrLiveJob(job) && job.status === 'running') ??
      jobs.find((job) => isGhcrLiveJob(job)) ??
      null
    setGhcrLiveJob(liveJob)
  }, [])

  const refresh = useCallback(async () => {
    setError(null)
    const rawSettings = await getSettings()
    const nextSettings: SettingsResponse = {
      ...rawSettings,
      instance: rawSettings.instance ?? { publicBaseUrl: null },
    }
    const nextNotifications = normalizeNotificationsForUi(await getNotifications())
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
    setTelegramBotTokenVisible(false)
    setTelegramBotTokenTouched(false)
    setTelegramBotTokenFocused(false)
    setGitHubPackages(nextGhcr)
    setGitHubPackagesPat(nextPat)
    setNotificationTestStates({})
    setNotificationTestRunning({})
    resetAutoSaveBaselines({
      settings: nextSettings,
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

  const updateResourceMonitor = useCallback(
    (
      fieldPath: string,
      updater: (current: SettingsResponse['resourceMonitor']) => SettingsResponse['resourceMonitor'],
      isToggle = false,
    ) => {
      setSettings((prev) => {
        if (!prev) return prev
        return { ...prev, resourceMonitor: updater(prev.resourceMonitor) }
      })
      markFieldDirty('backup', fieldPath, isToggle ? TOGGLE_DEBOUNCE_MS : TEXT_DEBOUNCE_MS)
    },
    [markFieldDirty],
  )

  const updateSchedules = useCallback(
    (
      fieldPath: string,
      updater: (current: SettingsResponse['schedules']) => SettingsResponse['schedules'],
      isToggle = false,
    ) => {
      setSettings((prev) => {
        if (!prev) return prev
        return { ...prev, schedules: updater(prev.schedules) }
      })
      markFieldDirty('backup', fieldPath, isToggle ? TOGGLE_DEBOUNCE_MS : TEXT_DEBOUNCE_MS)
    },
    [markFieldDirty],
  )

  const updateInstance = useCallback(
    (fieldPath: string, updater: (current: SettingsResponse['instance']) => SettingsResponse['instance']) => {
      setSettings((prev) => {
        if (!prev) return prev
        return { ...prev, instance: updater(prev.instance) }
      })
      markFieldDirty('backup', fieldPath, TEXT_DEBOUNCE_MS)
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

  const clearTelegramBotTokenMaskForEdit = useCallback(() => {
    setNotifications((prev) => {
      if (!prev) return prev
      const botToken = prev.telegram.botToken ?? ''
      if (!isMaskedTelegramBotToken(botToken)) return prev
      return {
        ...prev,
        telegram: {
          ...prev.telegram,
          botToken: '',
        },
      }
    })
  }, [])

  const restoreTelegramBotTokenMaskIfNeeded = useCallback(() => {
    setNotifications((prev) => {
      if (!prev) return prev
      const botToken = prev.telegram.botToken ?? ''
      if (botToken.trim().length > 0) return prev
      if (!prev.telegram.botTokenConfigured) return prev
      return {
        ...prev,
        telegram: {
          ...prev.telegram,
          botToken: TELEGRAM_BOT_TOKEN_MASK,
        },
      }
    })
  }, [])

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

  const openGhcrRegistry = useCallback(() => {
    if (busy) return
    void (async () => {
      setBusy(true)
      setError(null)
      let shouldNavigate = true
      try {
        await flushAutoSave(['ghcr'])
      } catch (e: unknown) {
        const message = errorMessage(e)
        shouldNavigate = await confirm({
          title: 'GHCR 配置尚未保存',
          body: (
            <div>
              <div className="modalLead">当前 GHCR 配置保存失败，继续进入维护页可能按旧配置执行注册任务。</div>
              <div className="modalKvGrid">
                <div className="modalKvLabel">错误</div>
                <div className="modalKvValue">
                  <Mono>{message}</Mono>
                </div>
              </div>
            </div>
          ),
          confirmText: '仍然进入',
          cancelText: '留在设置页',
          confirmVariant: 'danger',
          badgeText: '配置未保存',
          badgeTone: 'warn',
        })
        if (!shouldNavigate) setError(message)
      } finally {
        setBusy(false)
      }
      if (shouldNavigate) navigate({ name: 'ghcr-webhook-registry' })
    })()
  }, [busy, confirm, flushAutoSave])

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

  const runNotificationChannelTest = useCallback((channel: NotificationTestChannel) => {
    void (async () => {
      setNotificationTestRunning((prev) => ({ ...prev, [channel]: true }))
      setNotificationTestStates((prev) => ({ ...prev, [channel]: runningTestState(channel) }))
      let requestSent = false
      try {
        const response = await testNotifications({
          message: 'dockrev: test notification',
          channel,
        })
        requestSent = true
        const channelResult = response.results[channel]
        if (!channelResult) {
          throw new Error(`${NOTIFICATION_CHANNEL_LABEL[channel]} 未返回测试结果`)
        }
        if (channelResult.ok) {
          setNotificationTestStates((prev) => ({ ...prev, [channel]: successTestState(channel) }))
          return
        }
        const detail = (channelResult.error ?? '').trim() || '未知错误'
        setNotificationTestStates((prev) => ({
          ...prev,
          [channel]: errorTestState(channel, detail),
        }))
      } catch (e: unknown) {
        setNotificationTestStates((prev) => ({
          ...prev,
          [channel]: errorTestState(channel, errorMessage(e), { requestSent }),
        }))
      } finally {
        setNotificationTestRunning((prev) => ({ ...prev, [channel]: false }))
      }
    })()
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
  const telegramBotTokenIssue =
    autoSaveIssue?.scope === 'notifications' && autoSaveIssue.fieldPath === 'notifications.telegram.botToken'
      ? autoSaveIssue
      : null
  const updateCheckCronIssue =
    autoSaveIssue?.scope === 'backup' && autoSaveIssue.fieldPath === 'schedules.updateCheck.cron'
      ? autoSaveIssue
      : null
  const ghcrWebhookAuditCronIssue =
    autoSaveIssue?.scope === 'backup' && autoSaveIssue.fieldPath === 'schedules.ghcrWebhookAudit.cron'
      ? autoSaveIssue
      : null
  const showTelegramBotTokenEye =
    telegramBotTokenFocused && telegramBotTokenTouched && (notifications.telegram.botToken ?? '').trim().length > 0
  const telegramBotTokenInputClassName = telegramBotTokenIssue ? 'input inputError' : 'input'
  const updateCheckCronInputClassName = updateCheckCronIssue ? 'input inputError' : 'input'
  const ghcrWebhookAuditCronInputClassName = ghcrWebhookAuditCronIssue ? 'input inputError' : 'input'
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
            <div className="title">鉴权（Forward Auth）</div>
            <div className="muted">认证由入口代理负责；Dockrev 按用户/组执行项目侧鉴权（运行时只读）</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">用户头</div>
                <div className="mono">{settings.auth.forwardHeaderName}</div>
              </div>

              <div className="kvRow">
                <div className="label">组头</div>
                <div className="mono">{settings.auth.groupHeaderName}</div>
              </div>

              <div className="kvRow">
                <div className="label">鉴权模式</div>
                <div className="mono">{settings.auth.authorizationMode}</div>
              </div>

              <div className="kvRow">
                <div className="label">允许用户</div>
                <div className="mono">{settings.auth.allowedUserMasked || '-'}</div>
              </div>

              <div className="kvRow">
                <div className="label">允许组</div>
                <div className="mono">{settings.auth.allowedGroupMasked || '-'}</div>
              </div>

              <div className="kvRow">
                <div className="label">当前用户</div>
                <div className="mono">{settings.auth.currentUser || '-'}</div>
              </div>

              <div className="kvRow">
                <div className="label">当前组</div>
                <div className="mono">{settings.auth.currentGroups.length ? settings.auth.currentGroups.join(', ') : '-'}</div>
              </div>

              <div className="kvRow">
                <div className="label">命中方式</div>
                <div className="mono">{settings.auth.matchedBy || '-'}</div>
              </div>

              <div className="kvRow">
                <div className="label">允许匿名（开发环境）</div>
                <div className="muted">{settings.auth.allowAnonymousInDev ? 'on' : 'off'}</div>
              </div>

              <div className="muted" style={{ marginTop: 6 }}>
                该区域由启动配置控制：`DOCKREV_AUTH_FORWARD_HEADER_NAME` / `DOCKREV_AUTH_GROUP_HEADER_NAME` /
                `DOCKREV_AUTH_ALLOWED_USER` / `DOCKREV_AUTH_ALLOWED_GROUP` /
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
            <div className="title">资源监控</div>
            <div className="muted">控制服务详情页历史采样与 1s 实时 SSE 推送。</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">启用资源监控</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={settings.resourceMonitor.enabled}
                    disabled={busy}
                    onChange={(value) =>
                      updateResourceMonitor(
                        'settings.resourceMonitor.enabled',
                        (current) => ({ ...current, enabled: value }),
                        true,
                      )
                    }
                  />
                  <div className="muted">{settings.resourceMonitor.enabled ? 'on' : 'off'}</div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">历史采样频率</div>
                <select
                  className="input"
                  value={String(settings.resourceMonitor.sampleIntervalSeconds)}
                  disabled={busy || !settings.resourceMonitor.enabled}
                  onChange={(event) => {
                    const next = Number(event.target.value)
                    if (![10, 30, 60, 300].includes(next)) return
                    updateResourceMonitor('settings.resourceMonitor.sampleIntervalSeconds', (current) => ({
                      ...current,
                      sampleIntervalSeconds: next as 10 | 30 | 60 | 300,
                    }))
                  }}
                >
                  <option value="10">10 秒</option>
                  <option value="30">30 秒</option>
                  <option value="60">60 秒</option>
                  <option value="300">300 秒</option>
                </select>
              </div>

              <div className="kvRow">
                <div className="label">历史保留</div>
                <div className="muted">{settings.resourceMonitor.retentionDays} 天（固定）</div>
              </div>
            </div>
          </div>

          <div className="card">
            <div className="title">定时任务</div>
            <div className="muted">cron 按服务端本地时区解释（TZ）；5 段表达式会自动补秒=0。</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">定期检查更新</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={settings.schedules.updateCheck.enabled}
                    disabled={busy}
                    onChange={(value) =>
                      updateSchedules(
                        'schedules.updateCheck.enabled',
                        (current) => ({
                          ...current,
                          updateCheck: { ...current.updateCheck, enabled: value },
                        }),
                        true,
                      )
                    }
                  />
                  <div className="muted">{settings.schedules.updateCheck.enabled ? 'on' : 'off'}</div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">Cron（检查更新）</div>
                <div>
                  <input
                    className={updateCheckCronInputClassName}
                    disabled={busy}
                    value={settings.schedules.updateCheck.cron}
                    onChange={(e) =>
                      updateSchedules('schedules.updateCheck.cron', (current) => ({
                        ...current,
                        updateCheck: { ...current.updateCheck, cron: e.target.value },
                      }))
                    }
                    placeholder="*/30 * * * *"
                  />
                  <div className="muted" style={{ marginTop: 6 }}>
                    只创建检查任务（check.all），不自动更新。
                  </div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">Webhook 巡查（GHCR audit_all）</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={settings.schedules.ghcrWebhookAudit.enabled}
                    disabled={busy}
                    onChange={(value) =>
                      updateSchedules(
                        'schedules.ghcrWebhookAudit.enabled',
                        (current) => ({
                          ...current,
                          ghcrWebhookAudit: { ...current.ghcrWebhookAudit, enabled: value },
                        }),
                        true,
                      )
                    }
                  />
                  <div className="muted">{settings.schedules.ghcrWebhookAudit.enabled ? 'on' : 'off'}</div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">Cron（Webhook 巡查）</div>
                <div>
                  <input
                    className={ghcrWebhookAuditCronInputClassName}
                    disabled={busy}
                    value={settings.schedules.ghcrWebhookAudit.cron}
                    onChange={(e) =>
                      updateSchedules('schedules.ghcrWebhookAudit.cron', (current) => ({
                        ...current,
                        ghcrWebhookAudit: { ...current.ghcrWebhookAudit, cron: e.target.value },
                      }))
                    }
                    placeholder="0 3 * * *"
                  />
                  <div className="muted" style={{ marginTop: 6 }}>
                    只巡检与标记 drift，不自动修复。
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div className="card">
            <div className="title">实例 Public Base URL</div>
            <div className="muted">用于在通知中生成可点击的绝对链接（服务详情 / 任务详情）。</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">Public Base URL</div>
                <div>
                  <input
                    className="input"
                    value={settings.instance.publicBaseUrl ?? ''}
                    onChange={(e) =>
                      updateInstance('instance.publicBaseUrl', (current) => ({
                        ...current,
                        publicBaseUrl: e.target.value,
                      }))
                    }
                    placeholder="https://dockrev.example.com/"
                  />
                  <div className="muted" style={{ marginTop: 6 }}>
                    为空表示不配置；保存时会自动补齐尾部 <Mono>/</Mono>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div className="card">
            <div className="title">通知</div>
            <div className="muted">先选择通知事件，再为每个渠道配置发送方式。</div>

            <div className="kv" style={{ marginBottom: 12 }}>
              <div className="kvRow">
                <div className="label">更新完成通知</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={normalizeNotificationEvents(notifications.events).update}
                    disabled={busy}
                    onChange={(value) =>
                      updateNotifications(
                        'notifications.events.update',
                        (current) => ({
                          ...current,
                          events: { ...normalizeNotificationEvents(current.events), update: value },
                        }),
                        true,
                      )
                    }
                  />
                  <div className="muted">{normalizeNotificationEvents(notifications.events).update ? 'on' : 'off'}</div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">发现新版本通知（定时检查）</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={normalizeNotificationEvents(notifications.events).newVersion}
                    disabled={busy}
                    onChange={(value) =>
                      updateNotifications(
                        'notifications.events.newVersion',
                        (current) => ({
                          ...current,
                          events: { ...normalizeNotificationEvents(current.events), newVersion: value },
                        }),
                        true,
                      )
                    }
                  />
                  <div className="muted">{normalizeNotificationEvents(notifications.events).newVersion ? 'on' : 'off'}</div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">GitHub Webhook 异常通知（巡检）</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={normalizeNotificationEvents(notifications.events).ghcrWebhookAnomaly}
                    disabled={busy}
                    onChange={(value) =>
                      updateNotifications(
                        'notifications.events.ghcrWebhookAnomaly',
                        (current) => ({
                          ...current,
                          events: { ...normalizeNotificationEvents(current.events), ghcrWebhookAnomaly: value },
                        }),
                        true,
                      )
                    }
                  />
                  <div className="muted">
                    {normalizeNotificationEvents(notifications.events).ghcrWebhookAnomaly ? 'on' : 'off'}
                  </div>
                </div>
              </div>
            </div>

            <NotificationChannelCard
              channel="email"
              title="Email"
              enabled={notifications.email.enabled}
              busy={busy}
              testState={notificationTestStates.email}
              testRunning={Boolean(notificationTestRunning.email)}
              onRunTest={runNotificationChannelTest}
              onToggleEnabled={(v) =>
                updateNotifications(
                  'notifications.email.enabled',
                  (current) => ({ ...current, email: { ...current.email, enabled: v } }),
                  true,
                )
              }
            >
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
            </NotificationChannelCard>

            <NotificationChannelCard
              channel="webhook"
              title="Webhook"
              enabled={notifications.webhook.enabled}
              busy={busy}
              testState={notificationTestStates.webhook}
              testRunning={Boolean(notificationTestRunning.webhook)}
              onRunTest={runNotificationChannelTest}
              onToggleEnabled={(v) =>
                updateNotifications(
                  'notifications.webhook.enabled',
                  (current) => ({ ...current, webhook: { ...current.webhook, enabled: v } }),
                  true,
                )
              }
            >
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
            </NotificationChannelCard>

            <NotificationChannelCard
              channel="telegram"
              title="Telegram"
              enabled={notifications.telegram.enabled}
              busy={busy}
              testState={notificationTestStates.telegram}
              testRunning={Boolean(notificationTestRunning.telegram)}
              onRunTest={runNotificationChannelTest}
              onToggleEnabled={(v) =>
                updateNotifications(
                  'notifications.telegram.enabled',
                  (current) => ({ ...current, telegram: { ...current.telegram, enabled: v } }),
                  true,
                )
              }
            >
              <div className="kvRow">
                <div className="label">Bot token</div>
                <div className={showTelegramBotTokenEye ? 'inputWithAction' : undefined}>
                  <input
                    className={telegramBotTokenInputClassName}
                    type={telegramBotTokenVisible ? 'text' : 'password'}
                    autoComplete="new-password"
                    value={notifications.telegram.botToken ?? ''}
                    onFocus={() => {
                      setTelegramBotTokenFocused(true)
                      clearTelegramBotTokenMaskForEdit()
                    }}
                    onBlur={() => {
                      setTelegramBotTokenFocused(false)
                      setTelegramBotTokenVisible(false)
                      restoreTelegramBotTokenMaskIfNeeded()
                    }}
                    onChange={(e) => {
                      setTelegramBotTokenTouched(true)
                      updateNotifications('notifications.telegram.botToken', (current) => ({
                        ...current,
                        telegram: {
                          ...current.telegram,
                          botToken: e.target.value,
                          botTokenConfigured:
                            e.target.value.trim().length > 0 ? true : (current.telegram.botTokenConfigured ?? false),
                        },
                      }))
                    }}
                  />
                  {showTelegramBotTokenEye ? (
                    <button
                      type="button"
                      className="inputActionBtn"
                      aria-label={telegramBotTokenVisible ? '隐藏 Bot token' : '显示 Bot token'}
                      title={telegramBotTokenVisible ? '隐藏 Bot token' : '显示 Bot token'}
                      onClick={() => setTelegramBotTokenVisible((prev) => !prev)}
                    >
                      <Icon icon={telegramBotTokenVisible ? eyeOffOutline : eyeOutline} aria-hidden="true" />
                    </button>
                  ) : null}
                </div>
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
            </NotificationChannelCard>

            <NotificationChannelCard
              channel="webPush"
              title="Web Push（Chrome / VAPID）"
              enabled={notifications.webPush.enabled}
              busy={busy}
              testState={notificationTestStates.webPush}
              testRunning={Boolean(notificationTestRunning.webPush)}
              onRunTest={runNotificationChannelTest}
              onToggleEnabled={(v) =>
                updateNotifications(
                  'notifications.webPush.enabled',
                  (current) => ({ ...current, webPush: { ...current.webPush, enabled: v } }),
                  true,
                )
              }
            >
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

              <div className="formActions" style={{ marginTop: 10 }}>
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
            </NotificationChannelCard>

            {error ? <div className="error">{error}</div> : null}
          </div>

          {error ? <div className="error">{error}</div> : null}
        </div>

        <div className="settingsCol">
          <div className="card">
          <div className="title">GitHub Packages（GHCR）Webhook</div>
          <div className="muted">在 GHCR 发布新版本时自动触发 Dockrev 扫描（事件：package.published）</div>
          <div className="muted">添加后会自动创建后台任务注册 webhook；可在 GHCR 维护页查看状态并执行删除/重试。</div>
          {ghcrLiveProgressText ? (
            <div className="muted" style={{ marginTop: 8, display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
              <span>当前 GHCR 任务：{ghcrLiveProgressText}</span>
              <Button variant="ghost" disabled={busy} onClick={openGhcrRegistry}>
                打开 GHCR 维护页
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
                  <div className="muted">
                    预览 {githubPackagesTrackedRepos.repos.length} / {githubPackagesTrackedRepos.filteredTotal}
                  </div>
                  <div style={{ display: 'flex', gap: 10, marginLeft: 'auto' }}>
                    <Button variant="ghost" disabled={busy} onClick={openGhcrRegistry}>
                      查看更多
                      {githubPackagesTrackedRepos.filteredTotal > githubPackagesTrackedRepos.repos.length
                        ? `（+${githubPackagesTrackedRepos.filteredTotal - githubPackagesTrackedRepos.repos.length}）`
                        : ''}
                    </Button>
                  </div>
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
                              <Icon icon={webhookStateIcon(state)} className={dotClass} aria-hidden="true" />
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
                                检测到重复 webhook，请先到 GitHub 手工删除重复项，再到 GHCR 维护页点“重新注册”。
                              </div>
                            ) : null}
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

                {githubPackagesTrackedRepos.filteredTotal > githubPackagesTrackedRepos.repos.length ? (
                  <div className="muted" style={{ marginTop: 10 }}>
                    设置页仅展示前 {GHCR_PREVIEW_LIMIT} 条，更多仓库请点击“查看更多”进入维护页。
                  </div>
                ) : null}
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
