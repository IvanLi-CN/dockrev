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
import { Button, Mono, Switch } from '../ui'

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
        variant="primary"
        disabled={busy || !settings || !notifications}
        onClick={() => {
          void (async () => {
            if (!settings || !notifications) return
            setBusy(true)
            setError(null)
            try {
              await putSettings(settings.backup)
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
        保存设置
      </Button>,
    )
  }, [busy, notifications, onTopActions, refresh, settings])

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
    <div className="page">
      <div className="twoCol">
        <div className="settingsCol">
          <div className="card">
            <div className="title">鉴权（Forward Header）</div>
            <div className="muted">单用户：由反向代理注入 Header；本服务信任来源</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">Header 名称</div>
                <input
                  className="input"
                  value={settings.auth.forwardHeaderName}
                  onChange={(e) => setSettings({ ...settings, auth: { ...settings.auth, forwardHeaderName: e.target.value } })}
                />
              </div>

              <div className="kvRow">
                <div className="label">允许匿名（开发环境）</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={settings.auth.allowAnonymousInDev}
                    disabled={busy}
                    onChange={(v) => setSettings({ ...settings, auth: { ...settings.auth, allowAnonymousInDev: v } })}
                  />
                  <div className="muted">{settings.auth.allowAnonymousInDev ? 'on' : 'off'}</div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">当前用户展示</div>
                <div className="mono">ivan</div>
              </div>
            </div>
          </div>

          <div className="card">
            <div className="title">备份默认策略</div>
            <div className="muted">默认 fail-closed；目标过大可按阈值跳过（force 可覆盖）</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">启用更新前备份</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={settings.backup.enabled}
                    disabled={busy}
                    onChange={(v) => setSettings({ ...settings, backup: { ...settings.backup, enabled: v } })}
                  />
                  <div className="muted">{settings.backup.enabled ? 'on' : 'off'}</div>
                </div>
              </div>
              <div className="kvRow">
                <div className="label">要求备份成功（fail-closed）</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={settings.backup.requireSuccess}
                    disabled={busy}
                    onChange={(v) => setSettings({ ...settings, backup: { ...settings.backup, requireSuccess: v } })}
                  />
                  <div className="muted">{settings.backup.requireSuccess ? 'on' : 'off'}</div>
                </div>
              </div>
              <div className="kvRow">
                <div className="label">备份输出目录</div>
                <input
                  className="input"
                  value={settings.backup.baseDir}
                  onChange={(e) => setSettings({ ...settings, backup: { ...settings.backup, baseDir: e.target.value } })}
                />
              </div>
              <div className="kvRow">
                <div className="label">体积阈值（超过则跳过）</div>
                <div>
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
                  <div className="muted" style={{ marginTop: 6 }}>
                    当前：{formatBytes(settings.backup.skipTargetsOverBytes)}
                  </div>
                </div>
              </div>
            </div>
          </div>

          {error ? <div className="error">{error}</div> : null}
        </div>

        <div className="card">
          <div className="title">通知</div>
          <div className="muted">事件：发现更新 / 版本提示 / 更新成功 / 更新失败 / 备份失败</div>

          <div className="settingsSection">
            <div className="settingHead">
              <div className="sectionTitle">Email</div>
              <Switch
                checked={notifications.email.enabled}
                disabled={busy}
                onChange={(v) => setNotifications({ ...notifications, email: { ...notifications.email, enabled: v } })}
              />
            </div>
            <div className="kv">
              <div className="kvRow">
                <div className="label">SMTP URL</div>
                <input
                  className="input"
                  value={notifications.email.smtpUrl ?? ''}
                  onChange={(e) => setNotifications({ ...notifications, email: { ...notifications.email, smtpUrl: e.target.value } })}
                  placeholder="smtp://user:pass@smtp.example.com:587"
                />
              </div>
            </div>
          </div>

          <div className="settingsSection">
            <div className="settingHead">
              <div className="sectionTitle">Webhook</div>
              <Switch
                checked={notifications.webhook.enabled}
                disabled={busy}
                onChange={(v) => setNotifications({ ...notifications, webhook: { ...notifications.webhook, enabled: v } })}
              />
            </div>
            <div className="kv">
              <div className="kvRow">
                <div className="label">URL</div>
                <input
                  className="input"
                  value={notifications.webhook.url ?? ''}
                  onChange={(e) => setNotifications({ ...notifications, webhook: { ...notifications.webhook, url: e.target.value } })}
                  placeholder="https://hooks.example.com/dockrev"
                />
              </div>
            </div>
          </div>

          <div className="settingsSection">
            <div className="settingHead">
              <div className="sectionTitle">Telegram</div>
              <Switch
                checked={notifications.telegram.enabled}
                disabled={busy}
                onChange={(v) => setNotifications({ ...notifications, telegram: { ...notifications.telegram, enabled: v } })}
              />
            </div>
            <div className="kv">
              <div className="kvRow">
                <div className="label">Bot token</div>
                <input
                  className="input"
                  value={notifications.telegram.botToken ?? ''}
                  onChange={(e) => setNotifications({ ...notifications, telegram: { ...notifications.telegram, botToken: e.target.value } })}
                />
              </div>
              <div className="kvRow">
                <div className="label">Chat id</div>
                <input
                  className="input"
                  value={notifications.telegram.chatId ?? ''}
                  onChange={(e) => setNotifications({ ...notifications, telegram: { ...notifications.telegram, chatId: e.target.value } })}
                />
              </div>
            </div>
          </div>

          <div className="settingsSection">
            <div className="settingHead">
              <div className="sectionTitle">Web Push（Chrome / VAPID）</div>
              <Switch
                checked={notifications.webPush.enabled}
                disabled={busy}
                onChange={(v) => setNotifications({ ...notifications, webPush: { ...notifications.webPush, enabled: v } })}
              />
            </div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">Public Key</div>
                <input
                  className="input"
                  value={notifications.webPush.vapidPublicKey ?? ''}
                  onChange={(e) =>
                    setNotifications({ ...notifications, webPush: { ...notifications.webPush, vapidPublicKey: e.target.value } })
                  }
                />
              </div>
              <div className="kvRow">
                <div className="label">Private Key（留空=保持原值）</div>
                <input
                  className="input"
                  value={notifications.webPush.vapidPrivateKey ?? ''}
                  onChange={(e) =>
                    setNotifications({ ...notifications, webPush: { ...notifications.webPush, vapidPrivateKey: e.target.value } })
                  }
                />
              </div>
              <div className="kvRow">
                <div className="label">Subject</div>
                <input
                  className="input"
                  value={notifications.webPush.vapidSubject ?? ''}
                  onChange={(e) =>
                    setNotifications({ ...notifications, webPush: { ...notifications.webPush, vapidSubject: e.target.value } })
                  }
                />
              </div>
            </div>

            <div className="formActions" style={{ marginTop: 10 }}>
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
                取消订阅
              </Button>
            </div>

            {webPushEndpoint ? (
              <div className="muted" style={{ marginTop: 10 }}>
                endpoint <Mono>{webPushEndpoint.slice(0, 40)}…</Mono>
              </div>
            ) : null}
          </div>

          {error ? <div className="error">{error}</div> : null}
        </div>
      </div>
    </div>
  )
}
