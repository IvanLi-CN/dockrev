import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  createWebPushSubscription,
  deleteWebPushSubscription,
  getNotifications,
  getSettings,
  putNotifications,
  putSettings,
  testNotifications,
  type NotificationConfig,
  type SettingsResponse,
} from '../api'
import { Button, Mono, Pill } from '../ui'
import { getTheme, setTheme, type DockrevTheme } from '../theme'

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  return String(e)
}

function base64UrlToUint8Array(base64UrlString: string): Uint8Array {
  const padding = '='.repeat((4 - (base64UrlString.length % 4)) % 4)
  const base64 = (base64UrlString + padding).replace(/-/g, '+').replace(/_/g, '/')
  const raw = atob(base64)
  const out = new Uint8Array(raw.length)
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i)
  return out
}

function formatBytes(n: number) {
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

export function SettingsPage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const [settings, setSettings] = useState<SettingsResponse | null>(null)
  const [notifications, setNotifications] = useState<NotificationConfig | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [webPushEndpoint, setWebPushEndpoint] = useState<string | null>(null)

  const [theme, setThemeState] = useState<DockrevTheme>(() => getTheme())

  const refresh = useCallback(async () => {
    setError(null)
    setSettings(await getSettings())
    setNotifications(await getNotifications())
  }, [])

  useEffect(() => {
    void refresh().catch((e: unknown) => setError(errorMessage(e)))
  }, [refresh])

  useEffect(() => {
    onTopActions(
      <Button
        variant="ghost"
        disabled={busy}
        onClick={() => {
          void (async () => {
            setBusy(true)
            try {
              await refresh()
            } catch (e: unknown) {
              setError(errorMessage(e))
            } finally {
              setBusy(false)
            }
          })()
        }}
      >
        刷新
      </Button>,
    )
  }, [busy, onTopActions, refresh])

  const canWebPush = useMemo(() => {
    return typeof window !== 'undefined' && 'serviceWorker' in navigator && 'PushManager' in window
  }, [])

  async function ensureSubscription() {
    if (!notifications?.webPush.vapidPublicKey) throw new Error('请先在右侧配置 VAPID Public Key')
    if (!canWebPush) throw new Error('当前环境不支持 Web Push / Service Worker')

    const reg = await navigator.serviceWorker.register('/sw.js')
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

  if (!settings || !notifications) {
    return <div className="muted">加载中…</div>
  }

  return (
    <div className="page twoCol">
      <div className="card">
        <div className="title">系统设置</div>

        <div className="settingsSection">
          <div className="sectionRow">
            <div className="sectionTitle">外观</div>
            <div style={{ marginLeft: 'auto', display: 'flex', gap: 10, alignItems: 'center' }}>
              <Pill tone="muted">
                theme <Mono>{theme}</Mono>
              </Pill>
              <Button
                variant="ghost"
                onClick={() => {
                  const next = theme === 'dockrev-dark' ? 'dockrev-light' : 'dockrev-dark'
                  setTheme(next)
                  setThemeState(next)
                }}
              >
                切换主题
              </Button>
            </div>
          </div>
        </div>

        <div className="settingsSection">
          <div className="sectionTitle">鉴权</div>
          <div className="formGrid">
            <label className="formField formSpan2">
              <span className="label">Forward Header Name</span>
              <input
                className="input"
                value={settings.auth.forwardHeaderName}
                onChange={(e) =>
                  setSettings({ ...settings, auth: { ...settings.auth, forwardHeaderName: e.target.value } })
                }
              />
            </label>
            <label className="formField formSpan2">
              <span className="label">Allow Anonymous In Dev</span>
              <select
                className="input"
                value={String(settings.auth.allowAnonymousInDev)}
                onChange={(e) =>
                  setSettings({ ...settings, auth: { ...settings.auth, allowAnonymousInDev: e.target.value === 'true' } })
                }
              >
                <option value="false">false</option>
                <option value="true">true</option>
              </select>
            </label>
          </div>
        </div>

        <div className="settingsSection">
          <div className="sectionTitle">更新前备份</div>
          <div className="formGrid">
            <label className="formField">
              <span className="label">Enabled</span>
              <select
                className="input"
                value={String(settings.backup.enabled)}
                onChange={(e) => setSettings({ ...settings, backup: { ...settings.backup, enabled: e.target.value === 'true' } })}
              >
                <option value="false">false</option>
                <option value="true">true</option>
              </select>
            </label>
            <label className="formField">
              <span className="label">Require Success (fail-closed)</span>
              <select
                className="input"
                value={String(settings.backup.requireSuccess)}
                onChange={(e) =>
                  setSettings({ ...settings, backup: { ...settings.backup, requireSuccess: e.target.value === 'true' } })
                }
              >
                <option value="false">false</option>
                <option value="true">true</option>
              </select>
            </label>
            <label className="formField formSpan2">
              <span className="label">Base dir</span>
              <input
                className="input"
                value={settings.backup.baseDir}
                onChange={(e) => setSettings({ ...settings, backup: { ...settings.backup, baseDir: e.target.value } })}
              />
            </label>
            <label className="formField formSpan2">
              <span className="label">Skip targets over bytes</span>
              <input
                className="input"
                value={String(settings.backup.skipTargetsOverBytes)}
                onChange={(e) =>
                  setSettings({
                    ...settings,
                    backup: { ...settings.backup, skipTargetsOverBytes: Number(e.target.value) || 0 },
                  })
                }
              />
              <span className="muted">当前：{formatBytes(settings.backup.skipTargetsOverBytes)}</span>
            </label>
            <div className="formActions formSpan2">
              <Button
                variant="primary"
                disabled={busy}
                onClick={() => {
                  void (async () => {
                    setBusy(true)
                    setError(null)
                    try {
                      await putSettings(settings.backup)
                      await refresh()
                    } catch (e: unknown) {
                      setError(errorMessage(e))
                    } finally {
                      setBusy(false)
                    }
                  })()
                }}
              >
                保存
              </Button>
            </div>
          </div>
        </div>

        {error ? <div className="error">{error}</div> : null}
      </div>

      <div className="card">
        <div className="title">Notifications</div>

        <div className="settingsSection">
          <div className="sectionTitle">Email</div>
          <div className="formGrid">
            <label className="formField">
              <span className="label">Enabled</span>
              <select
                className="input"
                value={String(notifications.email.enabled)}
                onChange={(e) =>
                  setNotifications({ ...notifications, email: { ...notifications.email, enabled: e.target.value === 'true' } })
                }
              >
                <option value="false">false</option>
                <option value="true">true</option>
              </select>
            </label>
            <label className="formField formSpan2">
              <span className="label">SMTP URL</span>
              <input
                className="input"
                value={notifications.email.smtpUrl ?? ''}
                onChange={(e) => setNotifications({ ...notifications, email: { ...notifications.email, smtpUrl: e.target.value } })}
                placeholder="smtp://user:pass@smtp.example.com:587"
              />
            </label>
          </div>
        </div>

        <div className="settingsSection">
          <div className="sectionTitle">Webhook</div>
          <div className="formGrid">
            <label className="formField">
              <span className="label">Enabled</span>
              <select
                className="input"
                value={String(notifications.webhook.enabled)}
                onChange={(e) =>
                  setNotifications({
                    ...notifications,
                    webhook: { ...notifications.webhook, enabled: e.target.value === 'true' },
                  })
                }
              >
                <option value="false">false</option>
                <option value="true">true</option>
              </select>
            </label>
            <label className="formField formSpan2">
              <span className="label">URL</span>
              <input
                className="input"
                value={notifications.webhook.url ?? ''}
                onChange={(e) => setNotifications({ ...notifications, webhook: { ...notifications.webhook, url: e.target.value } })}
              />
            </label>
          </div>
        </div>

        <div className="settingsSection">
          <div className="sectionTitle">Telegram</div>
          <div className="formGrid">
            <label className="formField">
              <span className="label">Enabled</span>
              <select
                className="input"
                value={String(notifications.telegram.enabled)}
                onChange={(e) =>
                  setNotifications({
                    ...notifications,
                    telegram: { ...notifications.telegram, enabled: e.target.value === 'true' },
                  })
                }
              >
                <option value="false">false</option>
                <option value="true">true</option>
              </select>
            </label>
            <label className="formField">
              <span className="label">Bot token</span>
              <input
                className="input"
                value={notifications.telegram.botToken ?? ''}
                onChange={(e) =>
                  setNotifications({
                    ...notifications,
                    telegram: { ...notifications.telegram, botToken: e.target.value },
                  })
                }
              />
            </label>
            <label className="formField">
              <span className="label">Chat id</span>
              <input
                className="input"
                value={notifications.telegram.chatId ?? ''}
                onChange={(e) =>
                  setNotifications({
                    ...notifications,
                    telegram: { ...notifications.telegram, chatId: e.target.value },
                  })
                }
              />
            </label>
          </div>
        </div>

        <div className="settingsSection">
          <div className="sectionTitle">Web Push（Chrome / VAPID）</div>
          <div className="formGrid">
            <label className="formField">
              <span className="label">Enabled</span>
              <select
                className="input"
                value={String(notifications.webPush.enabled)}
                onChange={(e) =>
                  setNotifications({
                    ...notifications,
                    webPush: { ...notifications.webPush, enabled: e.target.value === 'true' },
                  })
                }
              >
                <option value="false">false</option>
                <option value="true">true</option>
              </select>
            </label>
            <label className="formField formSpan2">
              <span className="label">VAPID Public Key</span>
              <input
                className="input"
                value={notifications.webPush.vapidPublicKey ?? ''}
                onChange={(e) =>
                  setNotifications({
                    ...notifications,
                    webPush: { ...notifications.webPush, vapidPublicKey: e.target.value },
                  })
                }
              />
            </label>
            <label className="formField formSpan2">
              <span className="label">VAPID Private Key（留空=保持原值）</span>
              <input
                className="input"
                value={notifications.webPush.vapidPrivateKey ?? ''}
                onChange={(e) =>
                  setNotifications({
                    ...notifications,
                    webPush: { ...notifications.webPush, vapidPrivateKey: e.target.value },
                  })
                }
              />
            </label>
            <label className="formField formSpan2">
              <span className="label">VAPID Subject</span>
              <input
                className="input"
                value={notifications.webPush.vapidSubject ?? ''}
                onChange={(e) =>
                  setNotifications({
                    ...notifications,
                    webPush: { ...notifications.webPush, vapidSubject: e.target.value },
                  })
                }
              />
            </label>
          </div>
          <div className="formActions" style={{ marginTop: 10 }}>
            <Button
              variant="primary"
              disabled={busy}
              onClick={() => {
                void (async () => {
                  setBusy(true)
                  setError(null)
                  try {
                    await putNotifications(notifications)
                    await refresh()
                  } catch (e: unknown) {
                    setError(errorMessage(e))
                  } finally {
                    setBusy(false)
                  }
                })()
              }}
            >
              保存通知配置
            </Button>
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() => {
                void (async () => {
                  setBusy(true)
                  setError(null)
                  try {
                    await testNotifications('dockrev: test notification')
                  } catch (e: unknown) {
                    setError(errorMessage(e))
                  } finally {
                    setBusy(false)
                  }
                })()
              }}
            >
              发送测试通知
            </Button>
          </div>

          <div className="formActions" style={{ marginTop: 10 }}>
            <Button
              variant="ghost"
              disabled={busy || !canWebPush}
              onClick={() => {
                void (async () => {
                  setBusy(true)
                  setError(null)
                  try {
                    await ensureSubscription()
                  } catch (e: unknown) {
                    setError(errorMessage(e))
                  } finally {
                    setBusy(false)
                  }
                })()
              }}
              title={canWebPush ? '当前浏览器订阅 Web Push' : '当前环境不支持'}
            >
              订阅本浏览器
            </Button>
            <Button
              variant="ghost"
              disabled={busy || !canWebPush}
              onClick={() => {
                void (async () => {
                  setBusy(true)
                  setError(null)
                  try {
                    await removeSubscription()
                  } catch (e: unknown) {
                    setError(errorMessage(e))
                  } finally {
                    setBusy(false)
                  }
                })()
              }}
            >
              取消订阅本浏览器
            </Button>
            {webPushEndpoint ? (
              <div className="muted">
                endpoint <Mono>{webPushEndpoint.slice(0, 40)}…</Mono>
              </div>
            ) : null}
          </div>
        </div>

        {error ? <div className="error">{error}</div> : null}
      </div>
    </div>
  )
}
