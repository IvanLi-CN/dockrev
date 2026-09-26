import {
  Bell,
  CheckCheck,
  CircleAlert,
  LoaderCircle,
  X,
} from 'lucide-react'
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import {
  getNotificationInbox,
  getNotificationUnreadCount,
  markAllNotificationsRead,
  markNotificationRead,
} from './api'
import type { NotificationItem } from './api/types'
import { setNotificationBadge } from './notificationBadge'

const BROADCAST_CHANNEL_NAME = 'dockrev:notifications'
const COLD_START_ID = 'dockrevNotificationId'
const COLD_START_TARGET = 'dockrevNotificationTarget'
const CLICK_MESSAGE = 'DOCKREV_NOTIFICATION_CLICK'
const CLICK_ACK = 'DOCKREV_NOTIFICATION_CLICK_ACK'

type NotificationBroadcast = {
  type: 'unread-count'
  unreadCount: number
  notificationId?: string
}

type NotificationContextValue = {
  unreadCount: number
  items: NotificationItem[]
  isOpen: boolean
  loading: boolean
  error: string | null
  open: () => void
  close: () => void
  refresh: (withItems?: boolean) => Promise<void>
  read: (item: NotificationItem) => Promise<void>
  readAll: () => Promise<void>
}

const NotificationContext = createContext<NotificationContextValue | null>(null)

function clampUnreadCount(value: number): number {
  return Number.isFinite(value) ? Math.max(0, Math.floor(value)) : 0
}

function broadcastUnreadCount(unreadCount: number, notificationId?: string): void {
  if (typeof BroadcastChannel === 'undefined') return
  const channel = new BroadcastChannel(BROADCAST_CHANNEL_NAME)
  channel.postMessage({ type: 'unread-count', unreadCount, notificationId } satisfies NotificationBroadcast)
  channel.close()
}

function navigateToNotification(target: string): void {
  window.location.assign(target)
}

export function NotificationProvider(props: { children: ReactNode }) {
  const [unreadCount, setUnreadCount] = useState(0)
  const [items, setItems] = useState<NotificationItem[]>([])
  const [isOpen, setIsOpen] = useState(false)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const syncRef = useRef<Promise<void> | null>(null)
  const isOpenRef = useRef(false)

  const applyUnreadCount = useCallback((value: number) => {
    setUnreadCount(clampUnreadCount(value))
  }, [])

  const sync = useCallback(
    async (withItems = false): Promise<void> => {
      if (syncRef.current) return syncRef.current
      const task = (async () => {
        setLoading(true)
        setError(null)
        try {
          if (withItems || isOpenRef.current) {
            const response = await getNotificationInbox({ limit: 50 })
            setItems(response.items)
            applyUnreadCount(response.unreadCount)
          } else {
            const response = await getNotificationUnreadCount()
            applyUnreadCount(response.unreadCount)
          }
        } catch (cause) {
          setError(cause instanceof Error ? cause.message : '通知同步失败')
        } finally {
          setLoading(false)
        }
      })()
      syncRef.current = task
      try {
        await task
      } finally {
        if (syncRef.current === task) syncRef.current = null
      }
    },
    [applyUnreadCount],
  )

  const open = useCallback(() => {
    isOpenRef.current = true
    setIsOpen(true)
    void sync(true)
  }, [sync])

  const close = useCallback(() => {
    isOpenRef.current = false
    setIsOpen(false)
  }, [])

  const read = useCallback(
    async (item: NotificationItem) => {
      const response = await markNotificationRead(item.id)
      setItems((current) =>
        current.map((candidate) =>
          candidate.id === item.id ? { ...candidate, readAt: response.readAt } : candidate,
        ),
      )
      applyUnreadCount(response.unreadCount)
      broadcastUnreadCount(response.unreadCount, item.id)
    },
    [applyUnreadCount],
  )

  const readAll = useCallback(async () => {
    const response = await markAllNotificationsRead()
    setItems((current) => current.map((item) => ({ ...item, readAt: response.readAt })))
    applyUnreadCount(response.unreadCount)
    broadcastUnreadCount(response.unreadCount)
  }, [applyUnreadCount])

  useEffect(() => {
    setNotificationBadge(unreadCount)
  }, [unreadCount])

  useEffect(() => {
    void sync(false)

    const onVisibilityChange = () => {
      if (document.visibilityState === 'visible') void sync(isOpenRef.current)
    }
    const onFocus = () => void sync(isOpenRef.current)
    const onPageShow = () => void sync(isOpenRef.current)
    const onOnline = () => void sync(isOpenRef.current)
    const interval = window.setInterval(() => {
      if (document.visibilityState === 'visible') void sync(isOpenRef.current)
    }, 60_000)
    document.addEventListener('visibilitychange', onVisibilityChange)
    window.addEventListener('focus', onFocus)
    window.addEventListener('pageshow', onPageShow)
    window.addEventListener('online', onOnline)
    return () => {
      window.clearInterval(interval)
      document.removeEventListener('visibilitychange', onVisibilityChange)
      window.removeEventListener('focus', onFocus)
      window.removeEventListener('pageshow', onPageShow)
      window.removeEventListener('online', onOnline)
    }
  }, [sync])

  useEffect(() => {
    if (typeof BroadcastChannel === 'undefined') return
    const channel = new BroadcastChannel(BROADCAST_CHANNEL_NAME)
    const onMessage = (event: MessageEvent<NotificationBroadcast>) => {
      if (event.data?.type !== 'unread-count') return
      applyUnreadCount(event.data.unreadCount)
      if (isOpenRef.current) void sync(true)
    }
    channel.addEventListener('message', onMessage)
    return () => {
      channel.removeEventListener('message', onMessage)
      channel.close()
    }
  }, [applyUnreadCount, sync])

  const acknowledgeClick = useCallback(
    async (notificationId: string, target: string, port?: MessagePort) => {
      try {
        const item = items.find((candidate) => candidate.id === notificationId)
        await markNotificationRead(notificationId)
        const response = await getNotificationUnreadCount()
        applyUnreadCount(response.unreadCount)
        setItems((current) =>
          current.map((candidate) =>
            candidate.id === notificationId
              ? { ...candidate, readAt: new Date().toISOString() }
              : candidate,
          ),
        )
        broadcastUnreadCount(response.unreadCount, notificationId)
        port?.postMessage({ type: CLICK_ACK, ok: true })
        if (item || notificationId) navigateToNotification(target)
      } catch {
        port?.postMessage({ type: CLICK_ACK, ok: false })
      }
    },
    [applyUnreadCount, items],
  )

  useEffect(() => {
    const onServiceWorkerMessage = (event: MessageEvent) => {
      const data = event.data
      if (!data || data.type !== CLICK_MESSAGE) return
      void acknowledgeClick(data.notificationId, data.url, event.ports?.[0])
    }
    navigator.serviceWorker?.addEventListener('message', onServiceWorkerMessage)
    return () => navigator.serviceWorker?.removeEventListener('message', onServiceWorkerMessage)
  }, [acknowledgeClick])

  useEffect(() => {
    const params = new URLSearchParams(window.location.search)
    const notificationId = params.get(COLD_START_ID)
    const target = params.get(COLD_START_TARGET)
    if (!notificationId || !target) return
    void acknowledgeClick(notificationId, target).finally(() => {
      params.delete(COLD_START_ID)
      params.delete(COLD_START_TARGET)
      const query = params.toString()
      window.history.replaceState({}, '', `${window.location.pathname}${query ? `?${query}` : ''}${window.location.hash}`)
    })
  }, [acknowledgeClick])

  const value = useMemo<NotificationContextValue>(
    () => ({
      unreadCount,
      items,
      isOpen,
      loading,
      error,
      open,
      close,
      refresh: sync,
      read,
      readAll,
    }),
    [close, error, isOpen, items, loading, open, read, readAll, sync, unreadCount],
  )

  return <NotificationContext.Provider value={value}>{props.children}</NotificationContext.Provider>
}

export function useNotifications(): NotificationContextValue {
  const value = useContext(NotificationContext)
  if (!value) throw new Error('useNotifications must be used inside NotificationProvider')
  return value
}

function formatNotificationTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(date)
}

export function NotificationCenter() {
  const { unreadCount, items, isOpen, loading, error, open, close, read, readAll } = useNotifications()

  const onItemClick = async (item: NotificationItem) => {
    try {
      await read(item)
      navigateToNotification(item.url)
    } catch {
      // Keep the drawer open when the server cannot confirm the read.
    }
  }

  return (
    <>
      <button
        type="button"
        className="notificationBellButton"
        aria-label={unreadCount > 0 ? `通知，${unreadCount} 条未读` : '通知'}
        aria-expanded={isOpen}
        onClick={() => (isOpen ? close() : open())}
      >
        <Bell aria-hidden="true" size={18} strokeWidth={2.1} />
        {unreadCount > 0 ? (
          <span className="notificationBellBadge">{unreadCount > 99 ? '99+' : unreadCount}</span>
        ) : null}
      </button>
      {isOpen ? (
        <>
          <button type="button" className="notificationDrawerBackdrop" aria-label="关闭通知" onClick={close} />
          <aside className="notificationDrawer" aria-label="通知收件箱">
            <div className="notificationDrawerHeader">
              <div>
                <h2>通知</h2>
                <span>{unreadCount} 条未读</span>
              </div>
              <div className="notificationDrawerActions">
                <button
                  type="button"
                  className="notificationIconButton"
                  aria-label="全部标记已读"
                  title="全部标记已读"
                  onClick={() => void readAll()}
                  disabled={unreadCount === 0 || loading}
                >
                  <CheckCheck size={17} aria-hidden="true" />
                </button>
                <button type="button" className="notificationIconButton" aria-label="关闭通知" title="关闭" onClick={close}>
                  <X size={17} aria-hidden="true" />
                </button>
              </div>
            </div>
            {error ? (
              <div className="notificationState notificationStateError">
                <CircleAlert size={17} aria-hidden="true" />
                <span>{error}</span>
              </div>
            ) : null}
            {loading && items.length === 0 ? (
              <div className="notificationState"><LoaderCircle className="spin" size={18} aria-hidden="true" />加载中…</div>
            ) : items.length === 0 ? (
              <div className="notificationState">暂无通知</div>
            ) : (
              <div className="notificationList">
                {items.map((item) => (
                  <button
                    type="button"
                    key={item.id}
                    className={item.readAt ? 'notificationItem notificationItemRead' : 'notificationItem'}
                    onClick={() => void onItemClick(item)}
                  >
                    <span className="notificationItemDot" aria-hidden="true" />
                    <span className="notificationItemContent">
                      <strong>{item.title}</strong>
                      <span>{item.body}</span>
                      <time dateTime={item.createdAt}>{formatNotificationTime(item.createdAt)}</time>
                    </span>
                  </button>
                ))}
              </div>
            )}
          </aside>
        </>
      ) : null}
    </>
  )
}
