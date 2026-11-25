import type { MockRouteContext } from '../context'

export function handleNotificationInboxRoutes(ctx: MockRouteContext): Response | null {
  const { method, nowIso, json, urlPath } = ctx
  const notificationNowIso = ctx.scenario === 'notification-details'
    ? (offsetMs = 0) => new Date(Date.parse('2026-10-01T12:00:00+08:00') + offsetMs).toISOString()
    : nowIso
  const notificationReadAll = ctx.state.notificationReadAll
  const initiallyUnread = ctx.scenario === 'notification-single'
    ? 1
    : ctx.scenario === 'notification-details'
      ? 4
      : 2
  const unreadCount = notificationReadAll
    ? 0
    : Math.max(0, initiallyUnread - ctx.state.notificationReadIds.size)

  if (method === 'GET' && urlPath === '/api/notifications/unread-count') {
    if (ctx.scenario === 'notification-empty' || ctx.scenario === 'notification-all-read') {
      return json({ unreadCount: 0 })
    }
    if (ctx.scenario === 'notification-error') {
      return json({ message: '通知服务暂时不可用' }, { status: 503 })
    }
    if (ctx.scenario === 'notification-single') {
      return json({ unreadCount })
    }
    if (ctx.scenario === 'notification-details') return json({ unreadCount })
    return json({ unreadCount })
  }

  if (method === 'GET' && urlPath === '/api/notifications/inbox') {
    if (ctx.scenario === 'notification-error') {
      return json({ message: '通知服务暂时不可用' }, { status: 503 })
    }
    if (ctx.scenario === 'notification-empty' || ctx.scenario === 'notification-all-read') {
      return json({ items: [], nextCursor: null, unreadCount: 0 })
    }
    if (ctx.scenario === 'notification-single') {
      const id = 'story-notification-single'
      return json({
        items: [{
          id,
          kind: 'job_finished',
          title: '更新任务已成功',
          body: '服务「支付 API」的更新操作已完成。',
          url: '/queue/job_story_single',
          sourceJobId: 'job_story_single',
          createdAt: notificationNowIso(-3 * 60_000),
          readAt: ctx.state.notificationReadIds.has(id) ? notificationNowIso() : null,
        }],
        nextCursor: null,
        unreadCount,
      })
    }
    if (ctx.scenario === 'notification-details') {
      const items = [
        {
          id: 'story-notification-task',
          kind: 'job_finished',
          title: '更新任务已成功',
          body: '服务「支付 API」的更新操作已完成。',
          url: '/queue/job_story_task',
          sourceJobId: 'job_story_task',
          createdAt: notificationNowIso(-3 * 60_000),
          readAt: null,
        },
        {
          id: 'story-notification-service-version',
          kind: 'new_version_discovered',
          title: '发现 1 个新版本',
          body: '支付 API：2.4.0 -> 2.5.0',
          url: '/services/stack_prod/service_api',
          sourceJobId: 'job_story_service_version',
          createdAt: notificationNowIso(-12 * 60_000),
          readAt: null,
        },
        {
          id: 'story-notification-aggregate-version',
          kind: 'new_version_discovered',
          title: '发现 3 个新版本',
          body: '支付 API：2.4.0 -> 2.5.0\n后台 Worker：1.8.2 -> 1.9.0\n另有 1 个服务有新版本。',
          url: '/queue/job_story_aggregate',
          sourceJobId: 'job_story_aggregate',
          createdAt: notificationNowIso(-45 * 60_000),
          readAt: null,
        },
        {
          id: 'story-notification-ghcr',
          kind: 'ghcr_webhook_anomaly',
          title: 'GHCR Webhook 有 2 项异常',
          body: 'acme/api：未在 GHCR 找到关联仓库\nacme/worker：Webhook 检查失败',
          url: '/queue/job_story_ghcr',
          sourceJobId: 'job_story_ghcr',
          createdAt: notificationNowIso(-2 * 60 * 60_000),
          readAt: null,
        },
      ].map((item) => ({
        ...item,
        readAt: notificationReadAll || ctx.state.notificationReadIds.has(item.id)
          ? notificationNowIso()
          : item.readAt,
      }))
      return json({ items, nextCursor: null, unreadCount })
    }
    const items = [
      {
        id: 'story-notification-1',
        kind: 'job_finished',
        title: '更新任务已成功',
        body: '服务「支付 API」的更新操作已完成。',
        url: '/queue/job_story_1',
        sourceJobId: 'job_story_1',
        createdAt: nowIso(-3 * 60_000),
        readAt: null,
      },
      {
        id: 'story-notification-2',
        kind: 'new_version_discovered',
        title: '发现 3 个新版本',
        body: '支付 API：1.4.0 -> 1.5.0\n后台 Worker：2.0.1 -> 2.0.2\n另有 1 个服务有新版本。',
        url: '/queue/job_story_2',
        sourceJobId: 'job_story_2',
        createdAt: nowIso(-45 * 60_000),
        readAt: null,
      },
      {
        id: 'story-notification-3',
        kind: 'ghcr_webhook_anomaly',
        title: 'GHCR Webhook 有 2 项异常',
        body: 'acme/api：未在 GHCR 找到关联仓库\nacme/worker：Webhook 检查失败',
        url: '/queue/job_story_3',
        sourceJobId: 'job_story_3',
        createdAt: nowIso(-2 * 60 * 60_000),
        readAt: nowIso(-90 * 60_000),
      },
    ]
    return json({
      items: items.map((item) => ({
        ...item,
        readAt: notificationReadAll || ctx.state.notificationReadIds.has(item.id)
          ? nowIso()
          : item.readAt,
      })),
      nextCursor: null,
      unreadCount,
    })
  }

  if (method === 'POST' && urlPath.endsWith('/read') && urlPath.startsWith('/api/notifications/')) {
    const notificationId = urlPath.split('/').at(-2) ?? ''
    ctx.state.notificationReadIds.add(notificationId)
    const unreadAfterRead = notificationReadAll
      ? 0
      : Math.max(0, initiallyUnread - ctx.state.notificationReadIds.size)
    return json({ notificationId, readAt: notificationNowIso(), unreadCount: unreadAfterRead })
  }

  if (method === 'POST' && urlPath === '/api/notifications/read-all') {
    ctx.state.notificationReadAll = true
    return json({ readAt: notificationNowIso(), unreadCount: 0 })
  }

  return null
}
