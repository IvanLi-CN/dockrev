export type NotificationItem = {
  id: string
  kind: string
  title: string
  body: string
  url: string
  sourceJobId?: string | null
  createdAt: string
  readAt?: string | null
}

export type NotificationInboxResponse = {
  items: NotificationItem[]
  nextCursor?: string | null
  unreadCount: number
}

export type NotificationUnreadCountResponse = {
  unreadCount: number
}

export type NotificationReadResponse = {
  notificationId: string
  readAt: string
  unreadCount: number
}

export type NotificationReadAllResponse = {
  readAt: string
  unreadCount: number
}
