import { autoUpdate,flip,offset,shift,useFloating } from '@floating-ui/react'
import { useCallback,useEffect,useMemo,useRef,useState } from 'react'
import {
ApiError,
apiBaseUrl,
createWebPushSubscription,
deleteWebPushSubscription,
getGitHubPackagesSettings,
getNotifications,
getSettings,
listGitHubPackagesRepos,
listJobs,
putGitHubPackagesSettings,
putNotifications,
putSettings,
testNotifications,
type GitHubPackagesSettingsResponse,
type JobListItem,
type ListGitHubPackagesReposResponse,
type NotificationConfig,
type NotificationTestChannel,
type PutSettingsInput,
type SettingsResponse
} from '../api'
import {
type NotificationChannelTestState
} from '../components/NotificationChannelCard'
import { useConfirm } from '../confirm'
import { derivePublicBaseUrlSuggestion } from '../publicBaseUrlSuggestion'
import { currentRoutePathname,navigate } from '../routes'
import { selfUpgradeBaseUrl } from '../runtimeConfig'
import {
SETTINGS_GHCR_WEBHOOK_ID,
clearRequestedSettingsFocus,
peekRequestedSettingsFocus,
} from '../settingsFocus'
import { Button,Mono } from '../ui'
import { isPwaRuntimeEnabled } from '../pwaStatus'
import { useSupervisorHealth } from '../useSupervisorHealth'
import { useManagementEventBatch } from '../managementEvents'
import {
GHCR_PREVIEW_LIMIT,
PAT_MASK,
SAVE_SCOPE_ORDER,
TELEGRAM_BOT_TOKEN_MASK,
TEXT_DEBOUNCE_MS,
TOGGLE_DEBOUNCE_MS,
base64UrlToUint8Array,
buildSettingsSavePayload,
errorMessage,
errorTestState,
isGhcrLiveJob,
isMaskedPat,
isMaskedSecretLiteral,
isMaskedTelegramBotToken,
mapScopeLabel,
NOTIFICATION_CHANNEL_LABEL,
normalizeNotificationsForSave,
normalizeNotificationsForUi,
readField,
readReason,
readInstancePublicBaseUrlSuggestDismissedFromStorage,
runningTestState,
successTestState,
validateGhcrPatBeforeSave,
validateNotificationsBeforeSave,
writeInstancePublicBaseUrlSuggestDismissedToStorage,
type AutoSaveIssue,
type AutoSavePhase,
type GhcrDraft,
type SaveScope
} from './settings/helpers'

export function useSettingsPageState(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const confirm = useConfirm()
  const [settings, setSettings] = useState<SettingsResponse | null>(null)
  const [notifications, setNotifications] = useState<NotificationConfig | null>(null)
  const [telegramBotTokenVisible, setTelegramBotTokenVisible] = useState(false)
  const [telegramBotTokenTouched, setTelegramBotTokenTouched] = useState(false)
  const [telegramBotTokenFocused, setTelegramBotTokenFocused] = useState(false)
  const [octoRillApiKeyTouched, setOctoRillApiKeyTouched] = useState(false)
  const [octoRillApiKeyFocused, setOctoRillApiKeyFocused] = useState(false)
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
  const [instancePublicBaseUrlSuggestDismissed, setInstancePublicBaseUrlSuggestDismissed] = useState(() =>
    readInstancePublicBaseUrlSuggestDismissedFromStorage(),
  )

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
    else if (reason === 'octo_rill_api_base_url_invalid')
      message = 'OctoRill API Base URL 格式不合法，请填写不含账号密码的 http(s) 绝对 URL'
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
          if (scope === 'backup') {
            const settingsPayload = payload as PutSettingsInput
            const rawApiKey = settingsPayload.releaseNotes?.octoRill?.apiKey
            const apiKey = typeof rawApiKey === 'string' ? rawApiKey.trim() : rawApiKey
            if (typeof apiKey === 'string' && apiKey && !isMaskedSecretLiteral(apiKey)) {
              const apiKeyMask = '•'.repeat(Array.from(apiKey).length)
              setSettings((prev) =>
                prev
                  ? {
                      ...prev,
                      releaseNotes: {
                        ...prev.releaseNotes,
                        octoRill: {
                          ...prev.releaseNotes.octoRill,
                          apiKey: apiKeyMask,
                          apiKeyMasked: apiKeyMask,
                        },
                      },
                    }
                  : prev,
              )
            } else if (apiKey === null || apiKey === '') {
              setSettings((prev) =>
                prev
                  ? {
                      ...prev,
                      releaseNotes: {
                        ...prev.releaseNotes,
                        octoRill: {
                          ...prev.releaseNotes.octoRill,
                          apiKey: '',
                          apiKeyMasked: null,
                        },
                      },
                    }
                  : prev,
              )
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
      releaseNotes: {
        provider: rawSettings.releaseNotes?.provider ?? 'gitHub',
        octoRill: {
          enabled: rawSettings.releaseNotes?.octoRill.enabled ?? false,
          apiBaseUrl: rawSettings.releaseNotes?.octoRill.apiBaseUrl ?? '',
          apiKeyMasked: rawSettings.releaseNotes?.octoRill.apiKeyMasked ?? null,
          apiKey: rawSettings.releaseNotes?.octoRill.apiKeyMasked ?? '',
          defaultView: rawSettings.releaseNotes?.octoRill.defaultView ?? 'smart',
        },
      },
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
    setOctoRillApiKeyTouched(false)
    setOctoRillApiKeyFocused(false)
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

  useManagementEventBatch(({ events, resyncRequired }) => {
    const settingsChanged = resyncRequired || events.some((event) => event.domain === 'settings')
    const trackedReposChanged = resyncRequired || events.some((event) =>
      event.domain === 'github_packages' || event.summary.jobType === 'github_packages_webhook',
    )
    if (settingsChanged) void refresh().catch((error: unknown) => setError(errorMessage(error)))
    if (trackedReposChanged) void refreshTrackedRepos().catch((error: unknown) => setError(errorMessage(error)))
  })

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

  const updateReleaseNotes = useCallback(
    (
      fieldPath: string,
      updater: (current: SettingsResponse['releaseNotes']) => SettingsResponse['releaseNotes'],
      isToggle = false,
    ) => {
      setSettings((prev) => {
        if (!prev) return prev
        return { ...prev, releaseNotes: updater(prev.releaseNotes) }
      })
      markFieldDirty('backup', fieldPath, isToggle ? TOGGLE_DEBOUNCE_MS : TEXT_DEBOUNCE_MS)
    },
    [markFieldDirty],
  )

  const clearOctoRillApiKeyMaskForEdit = useCallback(() => {
    setOctoRillApiKeyTouched(false)
  }, [])

  const restoreOctoRillApiKeyMaskIfNeeded = useCallback(() => {
    if (octoRillApiKeyTouched) return
    setSettings((prev) => {
      if (!prev) return prev
      if ((prev.releaseNotes.octoRill.apiKey ?? '').trim()) return prev
      const mask = prev.releaseNotes.octoRill.apiKeyMasked
      if (!mask) return prev
      return {
        ...prev,
        releaseNotes: {
          ...prev.releaseNotes,
          octoRill: {
            ...prev.releaseNotes.octoRill,
            apiKey: mask,
          },
        },
      }
    })
  }, [octoRillApiKeyTouched])

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
    return (
      isPwaRuntimeEnabled() &&
      typeof window !== 'undefined' &&
      'serviceWorker' in navigator &&
      'PushManager' in window
    )
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

    const reg = await navigator.serviceWorker.ready
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

  const instancePublicBaseUrlValue = settings?.instance.publicBaseUrl ?? ''
  const suggestedPublicBaseUrl =
    typeof window === 'undefined'
      ? null
      : derivePublicBaseUrlSuggestion(currentRoutePathname(), window.location.origin, window.location.pathname)
  const showInstancePublicBaseUrlSuggestBubble =
    Boolean(settings && notifications && githubPackages) &&
    !instancePublicBaseUrlSuggestDismissed &&
    instancePublicBaseUrlValue.trim().length === 0 &&
    suggestedPublicBaseUrl != null
  const {
    refs: instancePublicBaseUrlSuggestRefs,
    floatingStyles: instancePublicBaseUrlSuggestFloatingStyles,
    placement: instancePublicBaseUrlSuggestPlacement,
  } = useFloating({
    open: showInstancePublicBaseUrlSuggestBubble,
    placement: 'bottom-end',
    whileElementsMounted: autoUpdate,
    middleware: [offset(12), flip({ fallbackPlacements: ['top-end'] }), shift({ padding: 12 })],
  })
  const setInstancePublicBaseUrlSuggestReference = useCallback(
    (node: HTMLInputElement | null) => {
      instancePublicBaseUrlSuggestRefs.setReference(node)
    },
    [instancePublicBaseUrlSuggestRefs],
  )
  const setInstancePublicBaseUrlSuggestFloating = useCallback(
    (node: HTMLDivElement | null) => {
      instancePublicBaseUrlSuggestRefs.setFloating(node)
    },
    [instancePublicBaseUrlSuggestRefs],
  )

  useEffect(() => {
    if (!settings || !notifications || !githubPackages) return
    if (peekRequestedSettingsFocus() !== 'ghcr-webhook') return

    const frame = window.requestAnimationFrame(() => {
      const target = document.getElementById(SETTINGS_GHCR_WEBHOOK_ID)
      if (!target) return
      clearRequestedSettingsFocus()
      target.scrollIntoView({ behavior: 'smooth', block: 'start' })
    })

    return () => window.cancelAnimationFrame(frame)
  }, [settings, notifications, githubPackages])

  const fillInstancePublicBaseUrlFromCurrentOrigin = () => {
    if (!settings || !notifications || !githubPackages) return
    if (!suggestedPublicBaseUrl) return
    updateInstance('instance.publicBaseUrl', (current) => ({
      ...current,
      publicBaseUrl: suggestedPublicBaseUrl,
    }))
  }

  const dismissInstancePublicBaseUrlSuggestBubble = () => {
    setInstancePublicBaseUrlSuggestDismissed(true)
    writeInstancePublicBaseUrlSuggestDismissedToStorage()
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
  const octoRillApiBaseUrlIssue =
    autoSaveIssue?.scope === 'backup' && autoSaveIssue.fieldPath === 'releaseNotes.octoRill.apiBaseUrl'
      ? autoSaveIssue
      : null
  const showTelegramBotTokenEye =
    telegramBotTokenFocused && telegramBotTokenTouched && (notifications?.telegram.botToken ?? '').trim().length > 0
  const telegramBotTokenInputClassName = telegramBotTokenIssue ? 'input inputError' : 'input'
  const updateCheckCronInputClassName = updateCheckCronIssue ? 'input inputError' : 'input'
  const ghcrWebhookAuditCronInputClassName = ghcrWebhookAuditCronIssue ? 'input inputError' : 'input'
  const octoRillApiBaseUrlInputClassName = octoRillApiBaseUrlIssue ? 'input inputError' : 'input'
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
  return {
    autoSaveIssue,
    autoSavePhase,
    autoSaveStatusText,
    autoSaveToastClassName,
    busy,
    canWebPush,
    clearOctoRillApiKeyMaskForEdit,
    clearTelegramBotTokenMaskForEdit,
    confirm,
    dismissInstancePublicBaseUrlSuggestBubble,
    error,
    fillInstancePublicBaseUrlFromCurrentOrigin,
    flushAutoSave,
    ghcrLiveProgressText,
    ghcrPatIssue,
    ghcrResolvePending,
    ghcrWebhookAuditCronInputClassName,
    githubPackages,
    githubPackagesNewRepo,
    githubPackagesPat,
    githubPackagesTrackedRepos,
    instancePublicBaseUrlSuggestFloatingStyles,
    instancePublicBaseUrlSuggestPlacement,
    instancePublicBaseUrlValue,
    notificationTestRunning,
    notificationTestStates,
    notifications,
    octoRillApiKeyFocused,
    octoRillApiKeyTouched,
    openGhcrRegistry,
    octoRillApiBaseUrlInputClassName,
    refresh,
    refreshTrackedRepos,
    restoreOctoRillApiKeyMaskIfNeeded,
    restoreTelegramBotTokenMaskIfNeeded,
    runNotificationChannelTest,
    selfUpgradeUrl,
    ensureSubscription,
    removeSubscription,
    setBusy,
    setError,
    setGhcrResolvePending,
    setGitHubPackagesNewRepo,
    setInstancePublicBaseUrlSuggestFloating,
    setInstancePublicBaseUrlSuggestReference,
    setOctoRillApiKeyFocused,
    setOctoRillApiKeyTouched,
    setTelegramBotTokenFocused,
    setTelegramBotTokenTouched,
    setTelegramBotTokenVisible,
    settings,
    showAutoSaveToast,
    showInstancePublicBaseUrlSuggestBubble,
    showTelegramBotTokenEye,
    suggestedPublicBaseUrl,
    supervisor,
    telegramBotTokenInputClassName,
    telegramBotTokenVisible,
    updateBackup,
    updateCheckCronInputClassName,
    updateGhcr,
    updateInstance,
    updateReleaseNotes,
    updateNotifications,
    updateResourceMonitor,
    updateSchedules,
    webPushEndpoint,
  }
}
