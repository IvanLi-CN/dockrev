import type { IgnoreRule, NotificationTestChannel, ServiceRepoLinkInferenceResponse, ServiceSettings } from '../../../../api'
import { imageRepoFromImageRef } from '../../../../imageRepo'
import type { MockRouteContext } from '../context'
import {
  buildResourceHistorySamples,
  buildResourceSsePayload,
  isMaskLiteral,
  isNotificationTestChannel,
  isValidTelegramBotToken,
  mockNotificationChannelResult,
  parseResourceWindow,
  toNotificationsResponse,
} from '../shared'

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
    jobSeqRef,
    json,
    makeMockDebug,
    method,
    normalizeDigestValue,
    nowIso,
    parseJsonBody,
    scenario,
    state: f,
    url,
    urlPath,
    urlPathWithQuery,
    buildMockDigestTagData,
    buildMockDiscoveryTimeline,
    buildMockGitHubReleaseLocateResponse,
    buildMockGitHubReleasesResponse,
  } = ctx

  if (method === 'POST' && urlPath === '/api/discovery/scan') {
    jobSeqRef.value += 1
    const jobId = `job-discovery-${jobSeqRef.value}`
    const startedAt = nowIso(-500)
    const finishedAt = nowIso(-200)
    const scan = {
      startedAt,
      durationMs: 12,
      summary: {
        projectsSeen: 0,
        stacksCreated: 0,
        stacksUpdated: 0,
        stacksSkipped: 0,
        stacksFailed: 0,
        stacksMarkedMissing: 0,
      },
      actions: [],
    }
    const job = {
      id: jobId,
      type: 'discovery',
      scope: 'all',
      stackId: null,
      serviceId: null,
      status: 'success',
      createdBy: 'ivan',
      reason: 'ui',
      createdAt: startedAt,
      startedAt,
      finishedAt,
      allowArchMismatch: false,
      backupMode: 'inherit',
      summary: { scan },
    }
    f.jobs = [job, ...f.jobs]
    f.jobById[jobId] = {
      ...job,
      logs: [{ ts: startedAt, level: 'info', msg: 'discovery scan finished' }],
      logsLastId: 1,
    }
    return json({ jobId })
  }

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

  if (method === 'GET' && urlPath === '/api/jobs') return json({ jobs: f.jobs })

  if (method === 'GET' && urlPath.startsWith('/api/jobs/')) {
    const id = decodeURIComponent(urlPath.split('/').slice(3).join('/'))
    const job = f.jobById[id]
    if (!job) return json({ error: 'not found' }, { status: 404 })
    return json({ job })
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

    if (backup) {
      const enabled = getBoolean(backup.enabled)
      const requireSuccess = getBoolean(backup.requireSuccess)
      const baseDir = getString(backup.baseDir)
      const skipTargetsOverBytes = typeof backup.skipTargetsOverBytes === 'number' ? backup.skipTargetsOverBytes : null
      f.settings.backup = {
        enabled: enabled ?? f.settings.backup.enabled,
        requireSuccess: requireSuccess ?? f.settings.backup.requireSuccess,
        baseDir: baseDir ?? f.settings.backup.baseDir,
        skipTargetsOverBytes: skipTargetsOverBytes ?? f.settings.backup.skipTargetsOverBytes,
      }
    }

    if (resourceMonitor) {
      const enabled = getBoolean(resourceMonitor.enabled)
      const interval =
        typeof resourceMonitor.sampleIntervalSeconds === 'number' ? resourceMonitor.sampleIntervalSeconds : null
      const normalizedInterval =
        interval === 10 || interval === 30 || interval === 60 || interval === 300
          ? (interval as 10 | 30 | 60 | 300)
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

    return json({ ok: true })
  }

  if (method === 'GET' && urlPath === '/api/deploy-check/report') {
    return json(f.deployCheckReport)
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

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/github-releases/locate')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const version = url?.searchParams.get('version')?.trim() ?? ''
    const perPage = Math.max(1, Number(url?.searchParams.get('perPage') ?? '20') || 20)
    const limit = Math.max(1, Number(url?.searchParams.get('limit') ?? '50') || 50)
    return json(buildMockGitHubReleaseLocateResponse(serviceId, version, perPage, limit))
  }

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/github-releases')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const page = Math.max(1, Number(url?.searchParams.get('page') ?? '1') || 1)
    const perPage = Math.max(1, Number(url?.searchParams.get('perPage') ?? '20') || 20)
    return json(buildMockGitHubReleasesResponse(serviceId, page, perPage))
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

    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const found = findService(serviceId)
    if (!found) return json({ error: 'not found' }, { status: 404 })

    const digestNorm = normalizeDigestValue(url?.searchParams.get('digest'))
    const refreshed = debug.lastVersionInferenceRefreshDigest === digestNorm
    const { repoTags, tags } = buildMockDigestTagData(serviceId, found.svc.image.tag, digestNorm, refreshed)

    return json({
      digest: digestNorm,
      tags,
      repoTags,
      scan: {
        repoTagsTotal: repoTags.length,
        repoTagsConsidered: repoTags.length,
        manifestsOk: digestNorm ? repoTags.length : 0,
        manifestsTimeout: 0,
        manifestsError: 0,
      },
    })
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
        : buildResourceHistorySamples(serviceId, parsedWindow.seconds)

    return json({
      serviceId,
      window: parsedWindow.window,
      samples,
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

  if (method === 'GET' && urlPath.startsWith('/api/services/') && urlPath.endsWith('/settings')) {
    const parts = urlPath.split('/').filter(Boolean)
    const serviceId = decodeURIComponent(parts[2])
    const settings = f.serviceSettingsById[serviceId]
    if (!settings) return json({ error: 'not found' }, { status: 404 })
    return json(settings)
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
