import type { MockRouteContext } from '../context'

export function handleNotificationInboxRoutes(ctx: MockRouteContext): Response | null {
  const { method, nowIso, json, urlPath } = ctx

  if (method === 'GET' && urlPath === '/api/notifications/unread-count') {
    return json({ unreadCount: 3 })
  }

  if (method === 'GET' && urlPath === '/api/notifications/inbox') {
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
