import { apiFetch } from './api'
import type {
  NotificationInboxResponse,
  NotificationReadAllResponse,
  NotificationReadResponse,
  NotificationUnreadCountResponse,
} from './api/notificationTypes'

export async function getNotificationInbox(input?: {
  limit?: number
  cursor?: string | null
}): Promise<NotificationInboxResponse> {
  const params = new URLSearchParams()
  if (input?.limit != null) params.set('limit', String(input.limit))
  if (input?.cursor) params.set('cursor', input.cursor)
  const suffix = params.toString() ? `?${params.toString()}` : ''
  const resp = await apiFetch(`/api/notifications/inbox${suffix}`)
  return (await resp.json()) as NotificationInboxResponse
}

export async function getNotificationUnreadCount(): Promise<NotificationUnreadCountResponse> {
  const resp = await apiFetch('/api/notifications/unread-count')
  return (await resp.json()) as NotificationUnreadCountResponse
}

export async function markNotificationRead(notificationId: string): Promise<NotificationReadResponse> {
  const resp = await apiFetch(`/api/notifications/${encodeURIComponent(notificationId)}/read`, {
    method: 'POST',
    body: '{}',
  })
  return (await resp.json()) as NotificationReadResponse
}

export async function markAllNotificationsRead(): Promise<NotificationReadAllResponse> {
  const resp = await apiFetch('/api/notifications/read-all', {
    method: 'POST',
    body: '{}',
  })
  return (await resp.json()) as NotificationReadAllResponse
}
