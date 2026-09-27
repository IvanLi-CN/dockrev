import type { MockRouteContext } from '../context'

export function handleNotificationInboxRoutes(ctx: MockRouteContext): Response | null {
  const { method, nowIso, json, urlPath } = ctx

  if (method === 'GET' && urlPath === '/api/notifications/unread-count') {
    if (ctx.scenario === 'notification-empty' || ctx.scenario === 'notification-all-read') {
      return json({ unreadCount: 0 })
    }
    if (ctx.scenario === 'notification-error') {
      return json({ message: '通知服务暂时不可用' }, { status: 503 })
    }
    if (ctx.scenario === 'notification-single') {
      return json({ unreadCount: 1 })
    }
    return json({ unreadCount: 3 })
  }

  if (method === 'GET' && urlPath === '/api/notifications/inbox') {
    if (ctx.scenario === 'notification-error') {
      return json({ message: '通知服务暂时不可用' }, { status: 503 })
    }
    if (ctx.scenario === 'notification-empty' || ctx.scenario === 'notification-all-read') {
      return json({ items: [], nextCursor: null, unreadCount: 0 })
    }
    if (ctx.scenario === 'notification-single') {
      return json({
        items: [{
          id: 'story-notification-single',
          kind: 'job_finished',
          title: '任务已成功',
          body: '更新任务已完成，点击查看执行详情。',
          url: '/queue/job_story_single',
          sourceJobId: 'job_story_single',
          createdAt: nowIso(-3 * 60_000),
          readAt: null,
        }],
        nextCursor: null,
        unreadCount: 1,
      })
    }
    return json({
      items: [
        {
          id: 'story-notification-1',
          kind: 'job_finished',
          title: '任务已成功',
          body: '更新任务已完成，点击查看执行详情。',
          url: '/queue/job_story_1',
          sourceJobId: 'job_story_1',
          createdAt: nowIso(-3 * 60_000),
          readAt: null,
        },
        {
          id: 'story-notification-2',
          kind: 'new_version_discovered',
          title: '发现 2 个新版本',
          body: 'api: 1.4.0 -> 1.5.0；worker: 2.0.1 -> 2.0.2',
          url: '/queue/job_story_2',
          sourceJobId: 'job_story_2',
          createdAt: nowIso(-45 * 60_000),
          readAt: null,
        },
        {
          id: 'story-notification-3',
          kind: 'ghcr_webhook_anomaly',
          title: 'GHCR Webhook 发现异常',
          body: 'acme/api [missing]、acme/worker [error]',
          url: '/queue/job_story_3',
          sourceJobId: 'job_story_3',
          createdAt: nowIso(-2 * 60 * 60_000),
          readAt: nowIso(-90 * 60_000),
        },
      ],
      nextCursor: null,
      unreadCount: 2,
    })
  }

  if (method === 'POST' && urlPath.endsWith('/read') && urlPath.startsWith('/api/notifications/')) {
    return json({ notificationId: urlPath.split('/').at(-2), readAt: nowIso(), unreadCount: 1 })
  }

  if (method === 'POST' && urlPath === '/api/notifications/read-all') {
    return json({ readAt: nowIso(), unreadCount: 0 })
  }

  return null
}
