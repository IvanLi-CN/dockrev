import { ApiError, type JobListItem, type NotificationConfig, type NotificationTestChannel, type PutSettingsInput, type SettingsResponse } from '../../api'
import { type NotificationChannelTestState } from '../../components/NotificationChannelCard'

export function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

export function base64UrlToUint8Array(base64UrlString: string): Uint8Array {
  const padding = '='.repeat((4 - (base64UrlString.length % 4)) % 4)
  const base64 = (base64UrlString + padding).replace(/-/g, '+').replace(/_/g, '/')
  const raw = atob(base64)
  const out = new Uint8Array(raw.length)
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i)
  return out
}

export function formatBytes(n: number) {
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

export function buildSettingsSavePayload(settings: SettingsResponse): PutSettingsInput {
  const octoRillApiKey = (settings.releaseNotes.octoRill.apiKey ?? '').trim()
  const releaseNotesApiKey =
    isMaskedSecretLiteral(octoRillApiKey) ? undefined : octoRillApiKey.length > 0 ? octoRillApiKey : null
  return {
    backup: settings.backup,
    resourceMonitor: {
      enabled: settings.resourceMonitor.enabled,
      sampleIntervalSeconds: settings.resourceMonitor.sampleIntervalSeconds,
    },
    schedules: settings.schedules,
    releaseNotes: {
      provider: settings.releaseNotes.provider,
      octoRill: {
        apiBaseUrl: settings.releaseNotes.octoRill.apiBaseUrl ?? '',
        ...(releaseNotesApiKey !== undefined ? { apiKey: releaseNotesApiKey } : {}),
        defaultView: settings.releaseNotes.octoRill.defaultView,
      },
    },
    instance: {
      publicBaseUrl: settings.instance.publicBaseUrl ?? '',
    },
  }
}

export type SaveScope = 'backup' | 'notifications' | 'ghcr'
export type AutoSavePhase = 'idle' | 'queued' | 'saving' | 'saved' | 'error'
export type AutoSaveIssue = {
  scope: SaveScope
  fieldPath: string
  reason: string
  message: string
  at: string
}

export const SAVE_SCOPE_ORDER: SaveScope[] = ['backup', 'notifications', 'ghcr']
export const TEXT_DEBOUNCE_MS = 400
export const TOGGLE_DEBOUNCE_MS = 120
export const PAT_MASK = '******'
export const TELEGRAM_BOT_TOKEN_MASK = '••••••••••••••••'
const TELEGRAM_BOT_TOKEN_PATTERN = /^\d{5,}:[A-Za-z0-9_-]{8,}$/
export const GHCR_PREVIEW_LIMIT = 6
const GITHUB_PAT_PREFIXES = ['ghp_', 'github_pat_', 'gho_', 'ghu_', 'ghs_', 'ghr_']
const GHCR_JOB_TYPES = new Set([
  'github_packages_webhook',
  'github_packages_webhook_sync_all',
  'github_packages_webhook_sync_repo',
])
export const NOTIFICATION_CHANNEL_LABEL: Record<NotificationTestChannel, string> = {
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

export type GhcrDraft = {
  enabled: boolean
  callbackUrl: string
  pat: string
  hasPersistedPat: boolean
}

export function isMaskedPat(value: string): boolean {
  return value.trim() === PAT_MASK
}

export function isMaskedSecretLiteral(value: string): boolean {
  const trimmed = value.trim()
  return trimmed.length > 0 && /^[*•]+$/.test(trimmed)
}

export function hasExplicitPat(value: string): boolean {
  const trimmed = value.trim()
  return trimmed.length > 0 && !isMaskedPat(trimmed)
}

export function isMaskedTelegramBotToken(value: string): boolean {
  return value === TELEGRAM_BOT_TOKEN_MASK || value === PAT_MASK
}

export function normalizeNotificationEvents(
  events: NotificationConfig['events'] | undefined,
): NonNullable<NotificationConfig['events']> {
  return {
    update: events?.update ?? DEFAULT_NOTIFICATION_EVENTS.update,
    newVersion: events?.newVersion ?? DEFAULT_NOTIFICATION_EVENTS.newVersion,
    ghcrWebhookAnomaly: events?.ghcrWebhookAnomaly ?? DEFAULT_NOTIFICATION_EVENTS.ghcrWebhookAnomaly,
  }
}

export function normalizeNotificationsForUi(input: NotificationConfig): NotificationConfig {
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

export function normalizeNotificationsForSave(input: NotificationConfig): NotificationConfig {
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

export function validateNotificationsBeforeSave(
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

export function isGhcrLiveJob(job: JobListItem): boolean {
  if (!GHCR_JOB_TYPES.has(job.type)) return false
  return job.status === 'running' || job.status === 'queued'
}

export function validateGhcrPatBeforeSave(
  draft: GhcrDraft,
): { fieldPath: string; reason: string; message: string } | null {
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

export function readReason(details: unknown): string | null {
  if (!details || typeof details !== 'object') return null
  const reason = (details as Record<string, unknown>).reason
  return typeof reason === 'string' ? reason : null
}

export function readField(details: unknown): string | null {
  if (!details || typeof details !== 'object') return null
  const field = (details as Record<string, unknown>).field
  return typeof field === 'string' ? field : null
}

export function mapResolveFailure(e: unknown): string {
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

export function mapScopeLabel(scope: SaveScope): string {
  if (scope === 'backup') return '系统设置'
  if (scope === 'notifications') return '通知'
  return 'GHCR'
}

export function runningTestState(channel: NotificationTestChannel): NotificationChannelTestState {
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

export function successTestState(channel: NotificationTestChannel): NotificationChannelTestState {
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

export function errorTestState(
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

export type RepoSelectedFilter = 'all' | 'selected' | 'unselected'
export type RepoScopeFilter = 'all' | 'ghcr_linked' | 'deployed'
export type RepoVisibilityFilter = 'all' | 'public' | 'private'
export type RepoSortKey = 'activity_desc' | 'name_asc'
export type RepoListDensity = 'cozy' | 'compact'
export type RepoVisibility = 'public' | 'private' | 'unknown'
export type RepoPickerItem = {
  fullName: string
  selected: boolean
  visibility: RepoVisibility
  lastActivityAt: string | null
  ghcrLinked: boolean | null
  deployed: boolean
}
const GHCR_PICKER_LIST_DENSITY_STORAGE_KEY = 'dockrev:settings:ghcrPicker:listDensity'
const INSTANCE_PUBLIC_BASE_URL_SUGGEST_DISMISSED_STORAGE_KEY =
  'dockrev:settings:instancePublicBaseUrl:suggestCurrentOriginDismissed'

export function normalizeRepoVisibility(raw: string | undefined): RepoVisibility {
  if (raw === 'public') return 'public'
  if (raw === 'private') return 'private'
  return 'unknown'
}

export function parseActivityMs(raw: string | null): number | null {
  if (!raw) return null
  const ms = Date.parse(raw)
  return Number.isFinite(ms) ? ms : null
}

export function formatRepoActivity(raw: string | null): string {
  const ms = parseActivityMs(raw)
  if (ms === null) return '活动时间未知'
  return `最近活动 ${new Date(ms).toLocaleDateString()}`
}

function normalizeRepoListDensity(raw: string | null): RepoListDensity {
  return raw === 'compact' ? 'compact' : 'cozy'
}

export function normalizeWebhookState(raw: string | null | undefined): string {
  const state = (raw ?? '').trim().toLowerCase()
  if (!state) return 'unknown'
  return state
}

export function webhookStateLabel(state: string): string {
  if (state === 'queued') return '排队中'
  if (state === 'running') return '注册中'
  if (state === 'ok') return '已注册'
  if (state === 'missing') return '缺失'
  if (state === 'error') return '失败'
  if (state === 'conflict') return '冲突'
  return '未知'
}

export function readRepoListDensityFromStorage(): RepoListDensity {
  try {
    return normalizeRepoListDensity(window.localStorage.getItem(GHCR_PICKER_LIST_DENSITY_STORAGE_KEY))
  } catch {
    return 'cozy'
  }
}

export function writeRepoListDensityToStorage(value: RepoListDensity) {
  try {
    window.localStorage.setItem(GHCR_PICKER_LIST_DENSITY_STORAGE_KEY, value)
  } catch {
    // Ignore storage errors (quota/disabled).
  }
}

export function readInstancePublicBaseUrlSuggestDismissedFromStorage(): boolean {
  try {
    return window.localStorage.getItem(INSTANCE_PUBLIC_BASE_URL_SUGGEST_DISMISSED_STORAGE_KEY) === '1'
  } catch {
    return false
  }
}

export function writeInstancePublicBaseUrlSuggestDismissedToStorage(): boolean {
  try {
    window.localStorage.setItem(INSTANCE_PUBLIC_BASE_URL_SUGGEST_DISMISSED_STORAGE_KEY, '1')
    return true
  } catch {
    return false
  }
}
