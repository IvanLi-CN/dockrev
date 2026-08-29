import type {
  BackupTargetPolicy,
  IgnoreRule,
  NotificationTestChannel,
  ServiceRepoLinkInferenceResponse,
  ServiceSettings,
} from '../../../../api'
import { imageRepoFromImageRef } from '../../../../imageRepo'
import type { MockRouteContext } from '../context'
import { handleJobStateRoutes } from './jobState'
import { handleLifecycleEventsRoute, projectLifecycleSnapshot } from './lifecycleEvents'
import {
  buildMockReleaseNotesItems,
  buildMockReleaseNotesExternalLinks,
  clampReleaseNotesLimit,
  mockReleaseTagMatchesVersion,
  parseReleaseNotesCursor,
} from '../releaseNotes'
import {
  buildResourceHistorySamples,
  buildResourceHistoryPeaks,
  buildResourceSsePayload,
  isMaskLiteral,
  isNotificationTestChannel,
  isVariableMaskLiteral,
  isValidTelegramBotToken,
  type MockServiceLogEventGateState,
  mockNotificationChannelResult,
  parseResourceWindow,
  toNotificationsResponse,
} from '../shared'

async function waitForMockEventGate(
  gate: string,
  eventGates: MockServiceLogEventGateState,
): Promise<void> {
  if (eventGates.released.has(gate)) return
  eventGates.waiting.add(gate)
  try {
    await new Promise<void>((resolve, reject) => {
      const eventName = `dockrev:release-service-log-events:${gate}`
      const abort = () => reject(new Error('Mock service log event gate was cancelled'))
      const release = () => {
        if (globalThis.__DOCKREV_MOCK_EVENT_GATES__ !== eventGates) return
        eventGates.abortController.signal.removeEventListener('abort', abort)
        eventGates.released.add(gate)
        resolve()
      }
      eventGates.abortController.signal.addEventListener('abort', abort, { once: true })
      globalThis.addEventListener(
        eventName,
        release,
        { once: true, signal: eventGates.abortController.signal },
      )
    })
  } finally {
    eventGates.waiting.delete(gate)
  }
}

export async function handleServiceStateRoutes(ctx: MockRouteContext): Promise<Response | null> {
  const {
    digestSnapshotPendingAttempts,
    findService,
    forcedDigestSnapshotPendingAttempts,
    getBoolean,
    getString,
    ignoreSeqRef,
    init,
    isRecord,
    json,
    makeMockDebug,
    method,
    normalizeDigestValue,
    nowIso,
    parseJsonBody,
    scenario,
    serviceLogEventGates,
    state: f,
    url,
    urlPath,
    urlPathWithQuery,
    buildMockDigestTagData,
    buildMockDiscoveryTimeline,
    buildMockGitHubReleasesDataset,
    buildMockGitHubReleasesResponse,
  } = ctx
  const jobStateResponse = await handleJobStateRoutes(ctx)
  if (jobStateResponse) return jobStateResponse
  const lifecycleEventsResponse = handleLifecycleEventsRoute(ctx)
  if (lifecycleEventsResponse) return lifecycleEventsResponse

  if (method === 'GET' && (urlPathWithQuery === '/api/discovery/projects' || urlPathWithQuery.startsWith('/api/discovery/projects?'))) {
    const query = url?.search
      ? url.search.slice(1)
      : urlPathWithQuery.includes('?')
        ? urlPathWithQuery.split('?')[1]
        : ''
    const params = new URLSearchParams(query)
    const archived = params.get('archived') ?? 'exclude'

    const list = f.discoveredProjects
    let out = list
    if (archived === 'only') out = list.filter((project) => Boolean(project.archived))
    if (archived === 'exclude') out = list.filter((project) => !project.archived)
    return json({ projects: out })
  }

  if (method === 'POST' && urlPath.startsWith('/api/discovery/projects/') && urlPath.endsWith('/archive')) {
    return json({}, { status: 204 })
  }

  if (method === 'POST' && urlPath.startsWith('/api/discovery/projects/') && urlPath.endsWith('/restore')) {
    return json({}, { status: 204 })
  }

  if (method === 'GET' && urlPath === '/api/ignores') return json({ rules: f.ignores })
  if (method === 'POST' && urlPath === '/api/ignores') {
    const parsed = parseJsonBody(init?.body)
    const record = isRecord(parsed) ? parsed : {}
    const scope = isRecord(record.scope) ? record.scope : {}
    const match = isRecord(record.match) ? record.match : {}
    const serviceId = getString(scope.serviceId)
    ignoreSeqRef.value += 1
    const ruleId = `ignore-ui-${ignoreSeqRef.value}`
    const rule: IgnoreRule = {
      id: ruleId,
      enabled: getBoolean(record.enabled) ?? false,
      scope: { type: 'service', serviceId: serviceId ?? 'unknown' },
      match: { kind: getString(match.kind) ?? 'regex', value: getString(match.value) ?? '.*' },
      note: getString(record.note) ?? null,
    }
    f.ignores = [rule, ...f.ignores]
    if (serviceId) {
      const found = findService(serviceId)
      if (found) {
        found.svc.ignore = { matched: true, ruleId, reason: rule.note ?? 'blocked via UI' }
      }
    }
    return json({ ruleId })
  }

  if (method === 'DELETE' && urlPath === '/api/ignores') {
    const parsed = parseJsonBody(init?.body)
    const record = isRecord(parsed) ? parsed : {}
    const ruleId = getString(record.ruleId) ?? ''
    const existing = f.ignores.find((rule) => rule.id === ruleId) ?? null
    f.ignores = f.ignores.filter((rule) => rule.id !== ruleId)
    if (existing) {
      const serviceId = existing.scope.serviceId
      const found = findService(serviceId)
      if (found) {
        const still = f.ignores.find((rule) => rule.scope.serviceId === serviceId) ?? null
        if (still) found.svc.ignore = { matched: true, ruleId: still.id, reason: still.note ?? 'blocked via UI' }
        else found.svc.ignore = null
      }
    }
    return json({ deleted: true })
  }

  if (method === 'GET' && urlPath === '/api/settings') return json(f.settings)

  if (method === 'PUT' && urlPath === '/api/settings') {
    const parsed = parseJsonBody(init?.body)
    const record = isRecord(parsed) ? parsed : null
    const backup = record && isRecord(record.backup) ? record.backup : null
    const resourceMonitor = record && isRecord(record.resourceMonitor) ? record.resourceMonitor : null
    const schedules = record && isRecord(record.schedules) ? record.schedules : null
    const releaseNotes = record && isRecord(record.releaseNotes) ? record.releaseNotes : null

    if (backup) {
      const enabled = getBoolean(backup.enabled)
      const requireSuccess = getBoolean(backup.requireSuccess)
      const skipTargetsOverBytes = typeof backup.skipTargetsOverBytes === 'number' ? backup.skipTargetsOverBytes : null
      f.settings.backup = {
        enabled: enabled ?? f.settings.backup.enabled,
        requireSuccess: requireSuccess ?? f.settings.backup.requireSuccess,
        baseDir: f.settings.backup.baseDir,
        skipTargetsOverBytes: skipTargetsOverBytes ?? f.settings.backup.skipTargetsOverBytes,
        storage: f.settings.backup.storage,
      }
    }

    if (resourceMonitor) {
      const enabled = getBoolean(resourceMonitor.enabled)
      const interval =
        typeof resourceMonitor.sampleIntervalSeconds === 'number' ? resourceMonitor.sampleIntervalSeconds : null
      const normalizedInterval =
        interval === 5 || interval === 10 || interval === 30 || interval === 60 || interval === 300
          ? (interval as 5 | 10 | 30 | 60 | 300)
          : f.settings.resourceMonitor.sampleIntervalSeconds

      f.settings.resourceMonitor = {
        ...f.settings.resourceMonitor,
        enabled: enabled ?? f.settings.resourceMonitor.enabled,
        sampleIntervalSeconds: normalizedInterval,
      }
    }

    if (schedules) {
      const updateCheck = isRecord(schedules.updateCheck) ? schedules.updateCheck : null
      if (updateCheck) {
        const enabled = getBoolean(updateCheck.enabled)
        const cron = getString(updateCheck.cron)
        f.settings.schedules.updateCheck = {
          enabled: enabled ?? f.settings.schedules.updateCheck.enabled,
          cron: cron ?? f.settings.schedules.updateCheck.cron,
        }
      }

      const ghcrWebhookAudit = isRecord(schedules.ghcrWebhookAudit) ? schedules.ghcrWebhookAudit : null
      if (ghcrWebhookAudit) {
        const enabled = getBoolean(ghcrWebhookAudit.enabled)
        const cron = getString(ghcrWebhookAudit.cron)
        f.settings.schedules.ghcrWebhookAudit = {
          enabled: enabled ?? f.settings.schedules.ghcrWebhookAudit.enabled,
          cron: cron ?? f.settings.schedules.ghcrWebhookAudit.cron,
        }
      }
    }

    if (releaseNotes) {
      const provider = getString(releaseNotes.provider)
      if (provider === 'gitHub' || provider === 'octoRill') {
        f.settings.releaseNotes.provider = provider
      }
      const octoRill = isRecord(releaseNotes.octoRill) ? releaseNotes.octoRill : null
      if (octoRill) {
        const enabled = getBoolean(octoRill.enabled)
        const apiBaseUrl = getString(octoRill.apiBaseUrl)
        const apiKey = Object.prototype.hasOwnProperty.call(octoRill, 'apiKey')
          ? octoRill.apiKey === null
            ? null
            : getString(octoRill.apiKey)
          : undefined
        const defaultView = getString(octoRill.defaultView)
        f.settings.releaseNotes.octoRill = {
          ...f.settings.releaseNotes.octoRill,
          enabled: enabled ?? f.settings.releaseNotes.octoRill.enabled,
          apiBaseUrl: apiBaseUrl ?? f.settings.releaseNotes.octoRill.apiBaseUrl,
          apiKeyMasked:
            apiKey === undefined
              ? f.settings.releaseNotes.octoRill.apiKeyMasked
              : apiKey && !isVariableMaskLiteral(apiKey)
                ? '•'.repeat(Array.from(apiKey).length)
                : apiKey === '' || apiKey === null
                  ? null
                  : f.settings.releaseNotes.octoRill.apiKeyMasked,
          apiKey:
            apiKey === undefined
              ? f.settings.releaseNotes.octoRill.apiKey
              : apiKey && !isVariableMaskLiteral(apiKey)
                ? '•'.repeat(Array.from(apiKey).length)
                : apiKey,
          defaultView:
            defaultView === 'original' || defaultView === 'translated' || defaultView === 'smart'
              ? defaultView
              : f.settings.releaseNotes.octoRill.defaultView,
        }
      }
    }

    return json({ ok: true })
  }

  if (method === 'GET' && urlPath === '/api/deploy-check/report') {
    return json(f.deployCheckReport)
  }

  if (method === 'POST' && urlPath === '/api/deploy-check/report/refresh') {
    return json(
      {
        ...f.deployCheckReport,
        status: 'ready',
        refreshing: true,
        retryAfterMs: 450,
      },
      { status: 202 },
    )
  }

  if (method === 'GET' && urlPath === '/api/deploy-welcome') {
    return json(f.deployWelcome)
  }

  if (method === 'PUT' && urlPath === '/api/deploy-welcome') {
    const parsed = parseJsonBody(init?.body)
    const record = isRecord(parsed) ? parsed : {}
    const neverAutoOpen = getBoolean(record.neverAutoOpen) ?? f.deployWelcome.neverAutoOpen
    f.deployWelcome = { neverAutoOpen, updatedAt: nowIso() }
    return json({ ok: true, ...f.deployWelcome })
  }

  if (method === 'GET' && urlPath === '/api/notifications') {
    return json(toNotificationsResponse(f.notifications))
  }

  if (method === 'PUT' && urlPath === '/api/notifications') {
    const parsed = parseJsonBody(init?.body)
    if (isRecord(parsed)) {
      const email = isRecord(parsed.email) ? parsed.email : null
      const webhook = isRecord(parsed.webhook) ? parsed.webhook : null
      const telegram = isRecord(parsed.telegram) ? parsed.telegram : null
      const webPush = isRecord(parsed.webPush) ? parsed.webPush : null
      const telegramHasBotToken = telegram ? Object.prototype.hasOwnProperty.call(telegram, 'botToken') : false
      const telegramBotToken = telegramHasBotToken ? getString(telegram?.botToken) : null
      const telegramBotTokenTrimmed = typeof telegramBotToken === 'string' ? telegramBotToken.trim() : ''
      const shouldReplaceTelegramToken =
        typeof telegramBotToken === 'string' &&
        telegramBotTokenTrimmed.length > 0 &&
        !isMaskLiteral(telegramBotTokenTrimmed)

      if (shouldReplaceTelegramToken && !isValidTelegramBotToken(telegramBotTokenTrimmed)) {
        return json(
          {
            error: {
              code: 'invalid_argument',
              message: 'invalid telegram bot token',
              details: { reason: 'telegram_bot_token_invalid' },
            },
          },
          { status: 400 },
        )
      }

      const telegramChatIdRaw = telegram ? getString(telegram.chatId) : null
      const telegramChatIdTrimmed = typeof telegramChatIdRaw === 'string' ? telegramChatIdRaw.trim() : null
      const hasExistingTelegramToken = (f.notifications.telegram.botToken ?? '').trim().length > 0
      const nextTelegramChatId =
        typeof telegramChatIdTrimmed === 'string'
          ? isMaskLiteral(telegramChatIdTrimmed)
            ? f.notifications.telegram.chatId
            : telegramChatIdTrimmed.length > 0
              ? telegramChatIdTrimmed
              : null
          : f.notifications.telegram.chatId

      f.notifications = {
        email: {
          enabled: (email && getBoolean(email.enabled)) ?? f.notifications.email.enabled,
          smtpUrl: (email && getString(email.smtpUrl)) ?? f.notifications.email.smtpUrl,
        },
        webhook: {
          enabled: (webhook && getBoolean(webhook.enabled)) ?? f.notifications.webhook.enabled,
          url: (webhook && getString(webhook.url)) ?? f.notifications.webhook.url,
        },
        telegram: {
          enabled: (telegram && getBoolean(telegram.enabled)) ?? f.notifications.telegram.enabled,
          botToken: shouldReplaceTelegramToken ? telegramBotToken : f.notifications.telegram.botToken,
          botTokenConfigured:
            shouldReplaceTelegramToken ? true : (f.notifications.telegram.botTokenConfigured ?? hasExistingTelegramToken),
          chatId: nextTelegramChatId,
        },
        webPush: {
          enabled: (webPush && getBoolean(webPush.enabled)) ?? f.notifications.webPush.enabled,
          vapidPublicKey: (webPush && getString(webPush.vapidPublicKey)) ?? f.notifications.webPush.vapidPublicKey,
          vapidPrivateKey: (webPush && getString(webPush.vapidPrivateKey)) ?? f.notifications.webPush.vapidPrivateKey,
          vapidSubject: (webPush && getString(webPush.vapidSubject)) ?? f.notifications.webPush.vapidSubject,
        },
      }
    }
    return json({ ok: true })
  }

  if (method === 'POST' && urlPath === '/api/notifications/test') {
    const parsed = parseJsonBody(init?.body)
    const record = isRecord(parsed) ? parsed : {}
    const hasChannelField = Object.prototype.hasOwnProperty.call(record, 'channel')
    const rawChannel = getString(record.channel)
    if (hasChannelField && record.channel != null && (!rawChannel || !isNotificationTestChannel(rawChannel))) {
      return json(
        {
          error: {
            code: 'invalid_argument',
            message: 'invalid notification test channel',
            details: { reason: 'invalid_notification_test_channel' },
          },
        },
        { status: 400 },
      )
    }

    const channels: NotificationTestChannel[] = []
    if (rawChannel && isNotificationTestChannel(rawChannel)) {
      channels.push(rawChannel)
    } else {
      if (f.notifications.email.enabled) channels.push('email')
      if (f.notifications.webhook.enabled) channels.push('webhook')
      if (f.notifications.telegram.enabled) channels.push('telegram')
      if (f.notifications.webPush.enabled) channels.push('webPush')
    }

    const results = channels.reduce<Record<string, { ok: boolean; error?: string }>>((acc, channel) => {
      acc[channel] = mockNotificationChannelResult(f.notifications, channel)
      return acc
    }, {})
    return json({ ok: true, results })
  }

  if (method === 'POST' && urlPath === '/api/web-push/subscriptions') return json({ ok: true })
  if (method === 'DELETE' && urlPath === '/api/web-push/subscriptions') return json({ ok: true })

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/candidates')) {
    return json({ error: 'not found' }, { status: 404 })
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/release-notes/locate')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const version = (url?.searchParams.get('version') ?? '').trim()
    const limit = clampReleaseNotesLimit(url?.searchParams.get('limit'), { fallback: 20, max: 30 })
    const githubAll = buildMockGitHubReleasesResponse(serviceId, 1, 10_000)
    const stale = buildMockGitHubReleasesDataset(serviceId).stale ?? null
    const provider = f.settings.releaseNotes.provider
    const octoRill = f.settings.releaseNotes.octoRill
    const configured = Boolean(octoRill.apiBaseUrl?.trim() && octoRill.apiKeyMasked)
    const useOctoRill = provider === 'octoRill'
    const source = useOctoRill ? 'octoRill' : 'gitHub'
    const defaultView = useOctoRill ? octoRill.defaultView : 'original'
    const externalLinks = buildMockReleaseNotesExternalLinks(
      githubAll.repo?.htmlUrl,
      githubAll.repo?.fullName,
      octoRill.apiBaseUrl,
    )
    const buildReadyWindowResponse = (
      start: number,
      anchor:
        | {
            status: 'found' | 'outsideWindow' | 'notFound' | 'unavailable'
            version: string
            matchedTag?: string | null
            indexWithinWindow?: number | null
            absoluteIndex?: number | null
            message?: string | null
          }
        | null,
    ) => {
      const maxStart = Math.max(0, githubAll.items.length - limit)
      const boundedStart = Math.min(Math.max(0, start), maxStart)
      const windowItems = githubAll.items.slice(boundedStart, boundedStart + limit)
      return json({
        status: 'ready',
        source,
        repo: githubAll.repo,
        cursor: boundedStart > 0 ? String(boundedStart) : null,
        limit,
        nextCursor: boundedStart + limit < githubAll.items.length ? String(boundedStart + limit) : null,
        previousCursor: boundedStart > 0 ? String(Math.max(0, boundedStart - limit)) : null,
        hasMore: boundedStart + limit < githubAll.items.length,
        defaultView,
        externalLinks,
        items: buildMockReleaseNotesItems(windowItems, source),
        message: githubAll.message,
        stale,
        anchor,
      })
    }

    if (useOctoRill && !configured) {
      return json({
        status: 'upstreamError',
        source: 'octoRill',
        repo: githubAll.repo,
        cursor: null,
        limit,
        nextCursor: null,
        previousCursor: null,
        hasMore: false,
        defaultView: octoRill.defaultView,
        externalLinks,
        items: [],
        message: 'OctoRill API Base URL 或 API Key 未配置完整。',
        stale: null,
        anchor: version
          ? {
              status: 'unavailable' as const,
              version,
              matchedTag: null,
              indexWithinWindow: null,
              absoluteIndex: null,
              message: 'OctoRill API Base URL 或 API Key 未配置完整。',
            }
          : null,
      })
    }

    if (githubAll.status !== 'ready') {
      return json({
        status:
          useOctoRill
            ? 'upstreamError'
            : githubAll.status === 'unsupportedRepo'
              ? 'unsupportedRepo'
              : 'upstreamError',
        source,
        repo: githubAll.repo,
        cursor: null,
        limit,
        nextCursor: null,
        previousCursor: null,
        hasMore: false,
        defaultView,
        externalLinks,
        items: [],
        message:
          githubAll.message ?? (useOctoRill ? 'OctoRill 公开 Release 暂不可用。' : '读取 GitHub Releases 失败，请稍后重试。'),
        stale: null,
        anchor: version
          ? {
              status: 'unavailable' as const,
              version,
              matchedTag: null,
              indexWithinWindow: null,
              absoluteIndex: null,
              message:
                githubAll.message ??
                (useOctoRill ? `OctoRill 未能直接定位 ${version}。` : `暂时无法定位 ${version}。`),
            }
          : null,
      })
    }

    const noVersionAnchor = !version
      ? {
          status: 'unavailable' as const,
          version: '',
          matchedTag: null,
          indexWithinWindow: null,
          absoluteIndex: null,
          message: '未提供需要定位的版本号，当前显示最新窗口。',
        }
      : null
    if (noVersionAnchor) {
      return buildReadyWindowResponse(0, noVersionAnchor)
    }

    const dataset = buildMockGitHubReleasesDataset(serviceId)
    const locateOverride = Object.entries(dataset.locateByVersion ?? {}).find(
      ([candidateVersion]) => mockReleaseTagMatchesVersion(candidateVersion, version),
    )?.[1]
    const allItems = githubAll.items
    const matchIndex = allItems.findIndex((item) => mockReleaseTagMatchesVersion(item.tagName, version))
    const locateStatus = locateOverride?.status ?? (matchIndex >= 0 ? 'found' : 'notFound')

    if (locateStatus === 'found') {
      const absoluteIndex = Math.max(0, locateOverride?.absoluteIndex ?? matchIndex)
      const maxStart = Math.max(0, allItems.length - limit)
      const preferredStart =
        locateOverride?.indexWithinWindow != null
          ? absoluteIndex - locateOverride.indexWithinWindow
          : absoluteIndex - Math.floor(limit / 2)
      const windowStart = Math.min(Math.max(0, preferredStart), maxStart)
      return buildReadyWindowResponse(windowStart, {
        status: 'found',
        version,
        matchedTag: locateOverride?.matchedTag ?? allItems[absoluteIndex]?.tagName ?? version,
        indexWithinWindow:
          locateOverride?.indexWithinWindow ?? Math.max(0, absoluteIndex - windowStart),
        absoluteIndex,
        message: locateOverride?.message ?? null,
      })
    }

    if (useOctoRill) {
      return json({
        status: 'upstreamError',
        source: 'octoRill',
        repo: githubAll.repo,
        cursor: null,
        limit,
        nextCursor: null,
        previousCursor: null,
        hasMore: false,
        defaultView: octoRill.defaultView,
        externalLinks,
        items: [],
        message: locateOverride?.message ?? `OctoRill 未能直接定位 ${version}。`,
        stale: null,
        anchor: {
          status: 'unavailable' as const,
          version,
          matchedTag: locateOverride?.matchedTag ?? null,
          indexWithinWindow: null,
          absoluteIndex: locateOverride?.absoluteIndex ?? null,
          message: locateOverride?.message ?? `OctoRill 未能直接定位 ${version}。`,
        },
      })
    }

    if (locateStatus === 'outsideWindow') {
      return buildReadyWindowResponse(0, {
        status: 'outsideWindow',
        version,
        matchedTag: locateOverride?.matchedTag ?? (matchIndex >= 0 ? allItems[matchIndex]?.tagName ?? version : version),
        indexWithinWindow: null,
        absoluteIndex: locateOverride?.absoluteIndex ?? null,
        message: locateOverride?.message ?? `已定位到 ${version}，但它不在当前锚点窗口内。`,
      })
    }

    if (locateStatus === 'unavailable') {
      return buildReadyWindowResponse(0, {
        status: 'unavailable',
        version,
        matchedTag: locateOverride?.matchedTag ?? null,
        indexWithinWindow: null,
        absoluteIndex: locateOverride?.absoluteIndex ?? null,
        message: locateOverride?.message ?? `暂时无法定位 ${version}，当前显示最新窗口。`,
      })
    }

    return buildReadyWindowResponse(0, {
      status: 'notFound',
      version,
      matchedTag: locateOverride?.matchedTag ?? null,
      indexWithinWindow: null,
      absoluteIndex: locateOverride?.absoluteIndex ?? null,
      message: locateOverride?.message ?? `在发布记录中未找到 ${version}，当前显示最新窗口。`,
    })
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/release-notes')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const limit = clampReleaseNotesLimit(url?.searchParams.get('limit'), { fallback: 20, max: 100 })
    const start = parseReleaseNotesCursor(url?.searchParams.get('cursor'))
    const githubAll = buildMockGitHubReleasesResponse(serviceId, 1, 10_000)
    const stale = buildMockGitHubReleasesDataset(serviceId).stale ?? null
    const provider = f.settings.releaseNotes.provider
    const octoRill = f.settings.releaseNotes.octoRill
    const configured = Boolean(octoRill.apiBaseUrl?.trim() && octoRill.apiKeyMasked)
    const useOctoRill = provider === 'octoRill'
    const source = useOctoRill ? 'octoRill' : 'gitHub'
    const defaultView = useOctoRill ? octoRill.defaultView : 'original'
    const externalLinks = buildMockReleaseNotesExternalLinks(
      githubAll.repo?.htmlUrl,
      githubAll.repo?.fullName,
      octoRill.apiBaseUrl,
    )

    if (useOctoRill && !configured) {
      return json({
        status: 'upstreamError',
        source: 'octoRill',
        repo: githubAll.repo,
        cursor: null,
        limit,
        nextCursor: null,
        previousCursor: null,
        hasMore: false,
        defaultView: octoRill.defaultView,
        externalLinks,
        items: [],
        message: 'OctoRill API Base URL 或 API Key 未配置完整。',
        stale: null,
        anchor: null,
      })
    }

    if (githubAll.status !== 'ready') {
      return json({
        status:
          useOctoRill
            ? 'upstreamError'
            : githubAll.status === 'unsupportedRepo'
              ? 'unsupportedRepo'
              : 'upstreamError',
        source,
        repo: githubAll.repo,
        cursor: null,
        limit,
        nextCursor: null,
        previousCursor: null,
        hasMore: false,
        defaultView,
        externalLinks,
        items: [],
        message:
          githubAll.message ?? (useOctoRill ? 'OctoRill 公开 Release 暂不可用。' : '读取 GitHub Releases 失败，请稍后重试。'),
        stale: null,
        anchor: null,
      })
    }

    const maxStart = Math.max(0, githubAll.items.length - limit)
    const boundedStart = Math.min(Math.max(0, start), maxStart)
    const windowItems = githubAll.items.slice(boundedStart, boundedStart + limit)

    return json({
      status: 'ready',
      source,
      repo: githubAll.repo,
      cursor: boundedStart > 0 ? String(boundedStart) : null,
      limit,
      nextCursor: boundedStart + limit < githubAll.items.length ? String(boundedStart + limit) : null,
      previousCursor: boundedStart > 0 ? String(Math.max(0, boundedStart - limit)) : null,
      hasMore: boundedStart + limit < githubAll.items.length,
      defaultView,
      externalLinks,
      items: buildMockReleaseNotesItems(windowItems, source),
      message: githubAll.message,
      stale,
      anchor: null,
    })
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/new-version-discovery-timeline')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    return json(buildMockDiscoveryTimeline(serviceId))
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/digest-tags-snapshot')) {
    const debug = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
    debug.digestTagsSnapshotCalls += 1
    debug.lastDigestTagsSnapshotUrl = urlPathWithQuery

    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const found = findService(serviceId)
    if (!found) return json({ error: 'not found' }, { status: 404 })

    if (scenario === 'version-tags-popover-snapshot-missing') {
      return json({ error: 'not found' }, { status: 404 })
    }

    const digestNorm = normalizeDigestValue(url?.searchParams.get('digest'))
    const pendingKey = `${serviceId}:${digestNorm || '<missing-digest>'}`
    const forcedPendingAttempts = forcedDigestSnapshotPendingAttempts.get(pendingKey) ?? 0
    if (forcedPendingAttempts > 0) {
      if (forcedPendingAttempts <= 1) forcedDigestSnapshotPendingAttempts.delete(pendingKey)
      else forcedDigestSnapshotPendingAttempts.set(pendingKey, forcedPendingAttempts - 1)
      return json(
        {
          status: 'pending',
          digest: digestNorm,
          retryAfterMs: 450,
        },
        { status: 202 },
      )
    }

    if (
      scenario === 'version-tags-popover-snapshot-pending' ||
      scenario === 'services-inference-pending-candidate-loading'
    ) {
      const attempt = (digestSnapshotPendingAttempts.get(pendingKey) ?? 0) + 1
      digestSnapshotPendingAttempts.set(pendingKey, attempt)
      const maxPendingAttempts = scenario === 'services-inference-pending-candidate-loading' ? 999 : 4
      if (attempt <= maxPendingAttempts) {
        return json(
          {
            status: 'pending',
            digest: digestNorm,
            retryAfterMs: 450,
          },
          { status: 202 },
        )
      }
    }

    const refreshed = debug.lastVersionInferenceRefreshDigest === digestNorm
    const { repoTags, tags } = buildMockDigestTagData(serviceId, found.svc.image.tag, digestNorm, refreshed)
    const considered = Math.min(100, repoTags.length)

    return json({
      digest: digestNorm,
      tags,
      checkedAt: nowIso(-5 * 60 * 1000),
      scan: {
        repoTagsTotal: repoTags.length,
        repoTagsConsidered: considered,
        manifestsOk: digestNorm ? considered : 0,
        manifestsTimeout: 0,
        manifestsError: 0,
      },
    })
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/digest-tags')) {
    const debug = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
    debug.digestTagsCalls += 1
    debug.lastDigestTagsUrl = urlPathWithQuery
    const snapshotResp = await handleServiceStateRoutes({
      ...ctx,
      method: 'GET',
      urlPath: urlPath.replace(/\/digest-tags$/, '/digest-tags-snapshot'),
      urlPathWithQuery: urlPathWithQuery.replace(/\/digest-tags(?=\?)/, '/digest-tags-snapshot').replace(/\/digest-tags$/, '/digest-tags-snapshot'),
    })
    if (snapshotResp) return snapshotResp
  }

  if (method === 'POST' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/version-inference/refresh')) {
    const debug = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
    debug.versionInferenceRefreshCalls += 1
    debug.lastVersionInferenceRefreshUrl = urlPathWithQuery

    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2] ?? '')
    const found = findService(serviceId)
    if (!found) return json({ error: 'not found' }, { status: 404 })

    const parsed = parseJsonBody(init?.body) as { digest?: unknown } | null
    const digestNorm = normalizeDigestValue(getString(parsed?.digest) ?? null)
    debug.lastVersionInferenceRefreshDigest = digestNorm || null

    if (!digestNorm) {
      return json(
        {
          error: {
            code: 'invalid_argument',
            message: 'digest is required',
          },
        },
        { status: 400 },
      )
    }

    const currentDigest = normalizeDigestValue(found.svc.image.digest ?? null)
    const candidateDigest = normalizeDigestValue(found.svc.candidate?.digest ?? null)
    if (digestNorm !== currentDigest && digestNorm !== candidateDigest) {
      return json({ error: 'not found' }, { status: 404 })
    }

    const pendingKey = `${serviceId}:${digestNorm}`
    const reason = forcedDigestSnapshotPendingAttempts.has(pendingKey) ? 'running' : 'force'
    if (reason === 'force') {
      forcedDigestSnapshotPendingAttempts.set(pendingKey, 2)
    }

    return json(
      {
        status: 'pending',
        serviceId,
        imageRepo: imageRepoFromImageRef(found.svc.image.ref) ?? found.svc.image.ref,
        digest: digestNorm,
        reason,
      },
      { status: 202 },
    )
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/resource-usage/history')) {
    const debug = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
    debug.resourceUsageHistoryCalls += 1
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2] ?? '')
    const found = findService(serviceId)
    if (!found) return json({ error: 'not found' }, { status: 404 })

    if (!f.settings.resourceMonitor.enabled) {
      return json(
        {
          error: {
            code: 'conflict',
            message: 'resource monitor disabled',
            details: { reason: 'resource_monitor_disabled' },
          },
        },
        { status: 409 },
      )
    }

    const parsedWindow = parseResourceWindow(url?.searchParams.get('window') ?? null)
    const samples =
      scenario === 'service-detail-resource-monitor-empty'
        ? []
        : buildResourceHistorySamples(serviceId, parsedWindow.seconds, parsedWindow.window)

    const peaks = ['7d', '30d'].includes(parsedWindow.window) ? buildResourceHistoryPeaks(samples) : undefined
    const lifecycleSnapshot = f.serviceLogsByServiceId[serviceId]?.lifecycle
    const lifecycle = lifecycleSnapshot
      ? projectLifecycleSnapshot(
          lifecycleSnapshot,
          serviceId,
          nowIso(-parsedWindow.seconds * 1000),
          nowIso(),
        )
      : null
    return json({
      serviceId,
      window: parsedWindow.window,
      samples,
      ...(peaks ? { resolutionSeconds: parsedWindow.window === '7d' ? 60 : 300, peaks } : {}),
      ...(lifecycle ? { lifecycle } : {}),
    })
  }

  if (method === 'GET' && urlPath === '/api/services/resource-usage/overview') {
    const parsedWindow = parseResourceWindow(url?.searchParams.get('window') ?? null)
    const generatedAt = new Date().toISOString()
    const staleAfterSeconds = Math.max(60, f.settings.resourceMonitor.sampleIntervalSeconds * 2)

    if (scenario === 'overview-resource-monitor-error') {
      return json({ error: { code: 'upstream_error', message: 'resource monitor unavailable' } }, { status: 503 })
    }

    if (!f.settings.resourceMonitor.enabled) {
      return json({
        enabled: false,
        window: parsedWindow.window,
        generatedAt,
        staleAfterSeconds,
        services: [],
      })
    }

    const services = Object.values(f.stackById)
      .filter((stack) => !stack.archived)
      .flatMap((stack) => stack.services.filter((service) => !service.archived))
      .map((service) => {
        const samples =
          scenario === 'service-detail-resource-monitor-empty'
            ? []
            : buildResourceHistorySamples(service.id, parsedWindow.seconds)
        const shiftedSamples =
          scenario === 'overview-resource-monitor-stale'
            ? samples.map((sample) => ({
                ...sample,
                sampledAt: new Date(Date.parse(sample.sampledAt) - 10 * 60 * 1000).toISOString(),
              }))
            : samples
        const latest = shiftedSamples[shiftedSamples.length - 1] ?? null
        const previous = shiftedSamples[shiftedSamples.length - 2] ?? null
        const prevTs = previous ? Date.parse(previous.sampledAt) : Number.NaN
        const nextTs = latest ? Date.parse(latest.sampledAt) : Number.NaN
        const seconds = Number.isFinite(prevTs) && Number.isFinite(nextTs) ? (nextTs - prevTs) / 1000 : 0
        const rate = (prev: number | null | undefined, next: number | null | undefined) =>
          seconds > 0 && prev != null && next != null && next >= prev ? (next - prev) / seconds : null
        const sampledAtMs = latest ? Date.parse(latest.sampledAt) : Number.NaN
        const stale = !Number.isFinite(sampledAtMs) || Date.now() - sampledAtMs > staleAfterSeconds * 1000
        const zeroRateSummary = scenario === 'overview-resource-monitor-zero-rates' && latest !== null

        return {
          serviceId: service.id,
          sampledAt: latest?.sampledAt ?? null,
          cpuPercent: zeroRateSummary ? 25 : latest?.cpuPercent ?? null,
          memUsedBytes: zeroRateSummary ? 0 : latest?.memUsedBytes ?? null,
          memLimitBytes: latest?.memLimitBytes ?? null,
          netRxRateBps: zeroRateSummary ? 0 : rate(previous?.netRxBytes, latest?.netRxBytes),
          netTxRateBps: zeroRateSummary ? 0 : rate(previous?.netTxBytes, latest?.netTxBytes),
          stale,
          sampleCount: shiftedSamples.length,
        }
      })

    return json({
      enabled: true,
      window: parsedWindow.window,
      generatedAt,
      staleAfterSeconds,
      services,
    })
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/resource-usage/events')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2] ?? '')
    const found = findService(serviceId)
    if (!found) return json({ error: 'not found' }, { status: 404 })

    if (!f.settings.resourceMonitor.enabled) {
      return json(
        {
          error: {
            code: 'conflict',
            message: 'resource monitor disabled',
            details: { reason: 'resource_monitor_disabled' },
          },
        },
        { status: 409 },
      )
    }

    const samples = buildResourceHistorySamples(serviceId, 60 * 60)
    const body = buildResourceSsePayload(serviceId, samples, scenario)
    return new Response(body || ': keep-alive\n\n', {
      status: 200,
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'x-accel-buffering': 'no',
      },
    })
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/logs')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2] ?? '')
    const snapshot = f.serviceLogsByServiceId[serviceId]?.snapshot ?? null
    if (!snapshot) return json({ error: 'not found' }, { status: 404 })
    return json(snapshot)
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/logs/events')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2] ?? '')
    const dataset = f.serviceLogsByServiceId[serviceId] ?? null
    if (!dataset) return json({ error: 'not found' }, { status: 404 })
    if (dataset.eventsGate) await waitForMockEventGate(dataset.eventsGate, serviceLogEventGates)
    const eventsPayload = dataset.eventsPayload
    if (dataset.eventsGate) dataset.eventsPayload = undefined
    return new Response(eventsPayload || ': keep-alive\n\n', {
      status: 200,
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'x-accel-buffering': 'no',
      },
    })
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/settings')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const settings = f.serviceSettingsById[serviceId]
    if (!settings) return json({ error: 'not found' }, { status: 404 })
    return json(settings)
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/backup-targets')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const backupTargets = f.serviceBackupTargetsById[serviceId]
    if (!backupTargets) return json({ error: 'not found' }, { status: 404 })
    return json(backupTargets)
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/backup-records')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const backupRecords = f.serviceBackupRecordsById[serviceId]
    if (!backupRecords) return json({ error: 'not found' }, { status: 404 })
    return json(backupRecords)
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/tag-suggestions')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const debug = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
    debug.serviceTagSuggestionCalls += 1
    debug.lastServiceTagSuggestionUrl = urlPath
    const items = f.serviceTagSuggestionsById[serviceId] ?? []
    return json({ items: items.slice(0, 20) })
  }

  if (method === 'PUT' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/compose-tag')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const parsed = parseJsonBody(init?.body)
    const debug = globalThis.__DOCKREV_MOCK_DEBUG__ ?? (globalThis.__DOCKREV_MOCK_DEBUG__ = makeMockDebug())
    debug.lastComposeTagRequest = parsed
    if (!isRecord(parsed) || typeof parsed.tag !== 'string' || !parsed.tag.trim()) {
      return json({ error: { code: 'invalid_argument', message: 'tag is required' } }, { status: 400 })
    }
    const tag = parsed.tag.trim()
    const found = findService(serviceId)
    if (!found) return json({ error: 'not found' }, { status: 404 })
    if (tag === 'compose-error') {
      return json(
        { error: { code: 'invalid_argument', message: 'image uses variable interpolation and cannot be edited safely' } },
        { status: 400 },
      )
    }
    const base = found.svc.image.ref.replace(/(?<!^):[^:/@]+(?:@.*)?$/, '')
    const imageRef = `${base}:${tag}`
    found.svc.image = { ...found.svc.image, ref: imageRef, tag }
    found.svc.candidate = null
    const settings = f.serviceSettingsById[serviceId]
    if (settings) found.svc.settings = settings
    const now = nowIso()
    f.serviceTagSuggestionsById[serviceId] = [
      { tag, lastUsedAt: now, source: 'manual', useCount: 1 },
      ...(f.serviceTagSuggestionsById[serviceId] ?? []).filter((item) => item.tag !== tag),
    ].slice(0, 20)
    return json({ ok: true, tag, imageRef, composeFile: '/srv/app/docker-compose.yml', updatedAt: now })
  }

  if (method === 'PUT' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/settings')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const body = typeof init?.body === 'string' ? init.body : ''
    const parsed = body ? (JSON.parse(body) as ServiceSettings) : null
    if (parsed) {
      f.serviceSettingsById[serviceId] = parsed
      const found = findService(serviceId)
      if (found) found.svc.settings = parsed
    }
    return json({ ok: true })
  }

  if (method === 'PUT' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/backup-targets')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const parsed = parseJsonBody(init?.body)
    if (!isRecord(parsed)) return json({ error: 'invalid json' }, { status: 400 })
    const current = f.serviceBackupTargetsById[serviceId]
    if (!current) return json({ error: 'not found' }, { status: 404 })
    const normalizeCategory = (
      currentItems: typeof current.bindPaths,
      input: unknown,
    ): typeof current.bindPaths =>
      currentItems.map((item) => {
        const requested = Array.isArray(input)
          ? input.find((value) => isRecord(value) && getString(value.key) === item.key)
          : null
        const requestedPolicy = requested ? getString(requested.policy) : null
        return {
          ...item,
          policy: (
            requestedPolicy === 'stop_related_services' || requestedPolicy === 'live_backup' || requestedPolicy === 'disabled'
              ? requestedPolicy
              : item.policy
          ) as BackupTargetPolicy,
        }
      })

    const next = {
      ...current,
      bindPaths: normalizeCategory(current.bindPaths, parsed.bindPaths),
      volumeNames: normalizeCategory(current.volumeNames, parsed.volumeNames),
    }
    f.serviceBackupTargetsById[serviceId] = next
    const settings = f.serviceSettingsById[serviceId]
    if (settings) {
      settings.backupTargets = {
        bindPaths: Object.fromEntries(
          next.bindPaths.map((item) => [
            item.key,
            item.policy === 'stop_related_services'
              ? 'force'
              : item.policy === 'live_backup'
                ? 'inherit'
                : 'skip',
          ]),
        ),
        volumeNames: Object.fromEntries(
          next.volumeNames.map((item) => [
            item.key,
            item.policy === 'stop_related_services'
              ? 'force'
              : item.policy === 'live_backup'
                ? 'inherit'
                : 'skip',
          ]),
        ),
      }
    }
    const found = findService(serviceId)
    if (found && settings) found.svc.settings = settings
    return json({ ok: true })
  }
  if (method === 'POST' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/repo-link/infer')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const found = findService(serviceId)
    if (!found) return json({ error: 'not found' }, { status: 404 })
    const inferred = f.repoLinkInferenceByServiceId[serviceId]
    if (inferred) return json(inferred)
    return json({
      repoUrl: null,
      strategy: 'none',
      reason: 'mock: repo link not inferred',
    } satisfies ServiceRepoLinkInferenceResponse)
  }

  if (method === 'POST' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/archive')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const found = findService(serviceId)
    if (found) found.svc.archived = true
    return json({}, { status: 204 })
  }

  if (method === 'POST' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/restore')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const found = findService(serviceId)
    if (found) found.svc.archived = false
    return json({}, { status: 204 })
  }
  return null
}
