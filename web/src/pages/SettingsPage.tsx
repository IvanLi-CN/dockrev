import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  createWebPushSubscription,
  deleteGitHubPackagesRepo,
  deleteWebPushSubscription,
  getGitHubPackagesSettings,
  getNotifications,
  getSettings,
  listGitHubPackagesRepos,
  putGitHubPackagesSettings,
  putNotifications,
  putSettings,
  resolveGitHubPackagesTarget,
  setGitHubPackagesRepoSelected,
  syncGitHubPackagesWebhooks,
  testNotifications,
  apiBaseUrl,
  type GitHubPackagesSettingsResponse,
  type ListGitHubPackagesReposResponse,
  type ResolveGitHubPackagesTargetResponse,
  type SyncGitHubPackagesWebhookResult,
  type NotificationConfig,
  type SettingsResponse,
} from '../api'
import { Button, IconButton, Mono, RefreshIcon, Switch, TrashIcon } from '../ui'
import { useConfirm } from '../confirm'
import { selfUpgradeBaseUrl } from '../runtimeConfig'
import { useSupervisorHealth } from '../useSupervisorHealth'

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

function GitHubPackagesRepoPicker({
  initial,
  onChange,
}: {
  initial: ResolveGitHubPackagesTargetResponse
  onChange: (repos: Array<{ fullName: string; selected: boolean }>) => void
}) {
  const [repos, setRepos] = useState(() => initial.repos.map((r) => ({ ...r })))

  useEffect(() => {
    onChange(repos)
  }, [repos, onChange])

  return (
    <div>
      <div className="modalLead">
        profile <Mono>{initial.owner}</Mono> · 选择要跟踪的仓库
      </div>
      <div className="modalList" style={{ maxHeight: 420, overflowY: 'auto' }}>
        {repos.map((r) => (
          <div key={r.fullName} className="modalListItem">
            <div className="modalListLeft" style={{ minWidth: 0 }}>
              <div className="modalListTitle">
                <span className="mono" style={{ overflowWrap: 'anywhere' }}>
                  {r.fullName}
                </span>
              </div>
            </div>
            <div className="modalListRight">
              <Switch
                checked={r.selected}
                onChange={(v) => {
                  setRepos((prev) => prev.map((x) => (x.fullName === r.fullName ? { ...x, selected: v } : x)))
                }}
              />
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

export function SettingsPage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const confirm = useConfirm()
  const [settings, setSettings] = useState<SettingsResponse | null>(null)
  const [notifications, setNotifications] = useState<NotificationConfig | null>(null)
  const [githubPackages, setGitHubPackages] = useState<GitHubPackagesSettingsResponse | null>(null)
  const [githubPackagesPat, setGitHubPackagesPat] = useState('')
  const [githubPackagesNewRepo, setGitHubPackagesNewRepo] = useState('')
  const [githubPackagesSyncResults, setGitHubPackagesSyncResults] = useState<SyncGitHubPackagesWebhookResult[] | null>(null)
  const [githubPackagesTrackedRepos, setGitHubPackagesTrackedRepos] = useState<ListGitHubPackagesReposResponse | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [webPushEndpoint, setWebPushEndpoint] = useState<string | null>(null)
  const supervisor = useSupervisorHealth()
  const selfUpgradeUrl = useMemo(() => selfUpgradeBaseUrl(), [])

  const refreshTrackedRepos = useCallback(async () => {
    setGitHubPackagesTrackedRepos(
      await listGitHubPackagesRepos({
        page: 1,
        perPage: 200,
        q: null,
        selectedFilter: 'selected',
      }),
    )
  }, [])

  const refresh = useCallback(async () => {
    setError(null)
    setSettings(await getSettings())
    setNotifications(await getNotifications())
    const gh = await getGitHubPackagesSettings()
    const defaultCallbackUrl = (() => {
      if (typeof window === 'undefined') return ''
      const base = apiBaseUrl()
      const resolvedBase = new URL(base || window.location.origin, window.location.origin).toString().replace(/\/$/, '')
      return `${resolvedBase}/api/webhooks/github-packages`
    })()
    const callbackUrl = gh.callbackUrl || defaultCallbackUrl
    setGitHubPackages({ ...gh, callbackUrl })
    setGitHubPackagesPat(gh.patMasked ?? '')
  }, [])

  useEffect(() => {
    void (async () => {
      await refresh()
      await refreshTrackedRepos()
    })().catch((e: unknown) => setError(errorMessage(e)))
  }, [refresh, refreshTrackedRepos])

  useEffect(() => {
    onTopActions(
      <Button
        variant="primary"
        disabled={busy || !settings || !notifications || !githubPackages}
        onClick={() => {
          void (async () => {
            if (!settings || !notifications || !githubPackages) return
            setBusy(true)
            setError(null)
            try {
              await putSettings(settings.backup)
              await putNotifications(notifications)
              await putGitHubPackagesSettings({
                enabled: githubPackages.enabled,
                callbackUrl: githubPackages.callbackUrl,
                pat: githubPackagesPat || null,
              })
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
  }, [busy, githubPackages, githubPackagesPat, notifications, onTopActions, refresh, settings])

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

  if (!settings || !notifications || !githubPackages) {
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
            <div className="title">自我升级</div>
            <div className="muted">Dockrev 更新 Dockrev：由独立 supervisor 提供页面与执行者（默认 {selfUpgradeUrl}）</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">Supervisor 状态</div>
                <div className="muted">
                  {supervisor.state.status === 'ok'
                    ? `ok (${supervisor.state.okAt})`
                    : supervisor.state.status === 'checking'
                      ? 'checking…'
                      : supervisor.state.status === 'offline'
                        ? `offline (${supervisor.state.errorAt})`
                        : 'unknown'}
                </div>
              </div>
              {supervisor.state.status === 'offline' ? (
                <div className="kvRow">
                  <div className="label">原因</div>
                  <div className="muted">
                    <Mono>{supervisor.state.error}</Mono>
                  </div>
                </div>
              ) : null}
            </div>

            <div className="formActions">
              <Button
                variant="primary"
                disabled={busy || supervisor.state.status !== 'ok'}
                title={supervisor.state.status === 'offline' ? '自我升级不可用（supervisor offline）' : undefined}
                onClick={() => {
                  window.location.href = selfUpgradeUrl
                }}
              >
                打开自我升级
              </Button>
              <Button
                variant="ghost"
                disabled={busy || supervisor.state.status === 'checking'}
                onClick={() => {
                  void supervisor.check()
                }}
              >
                重试
              </Button>
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
                    onChange={(e) =>
                      setNotifications({ ...notifications, webhook: { ...notifications.webhook, url: e.target.value } })
                    }
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
                    onChange={(e) =>
                      setNotifications({ ...notifications, telegram: { ...notifications.telegram, botToken: e.target.value } })
                    }
                  />
                </div>
                <div className="kvRow">
                  <div className="label">Chat id</div>
                  <input
                    className="input"
                    value={notifications.telegram.chatId ?? ''}
                    onChange={(e) =>
                      setNotifications({ ...notifications, telegram: { ...notifications.telegram, chatId: e.target.value } })
                    }
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

          {error ? <div className="error">{error}</div> : null}
        </div>

        <div className="settingsCol">
          <div className="card">
          <div className="title">GitHub Packages（GHCR）Webhook</div>
          <div className="muted">在 GHCR 发布新版本时自动触发 Dockrev 扫描（事件：package.published）</div>

          <div className="settingsSection">
            <div className="settingHead">
              <div className="sectionTitle">启用</div>
              <Switch
                checked={githubPackages.enabled}
                disabled={busy}
                onChange={(v) => setGitHubPackages({ ...githubPackages, enabled: v })}
              />
            </div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">GitHub PAT（留空=保持原值）</div>
                <input
                  className="input"
                  value={githubPackagesPat}
                  onChange={(e) => setGitHubPackagesPat(e.target.value)}
                  placeholder="ghp_..."
                />
                <div className="muted" style={{ marginTop: 6 }}>
                  提示：解析 profile/username 与同步 webhook 需要先“保存设置”把 PAT 写入后端。
                </div>
              </div>

              <div className="kvRow">
                <div className="label">Callback URL</div>
                <input
                  className="input"
                  value={githubPackages.callbackUrl}
                  onChange={(e) => setGitHubPackages({ ...githubPackages, callbackUrl: e.target.value })}
                  placeholder="https://dockrev.example.com/api/webhooks/github-packages"
                />
              </div>
            </div>
          </div>

          <div className="settingsSection">
            <div className="settingHead">
              <div className="sectionTitle">Repos</div>
              <div className="muted">{githubPackages.reposSelectedTotal} 个</div>
            </div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">添加 Repo</div>
                <div style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
                  <input
                    className="input"
                    value={githubPackagesNewRepo}
                    onChange={(e) => setGitHubPackagesNewRepo(e.target.value)}
                    placeholder="https://github.com/org/repo 或 org/repo；也可粘贴 profile/org URL 批量选择"
                    style={{ flex: 1 }}
                  />
                  <Button
                    variant="ghost"
                    disabled={busy || !githubPackagesNewRepo.trim()}
                    onClick={() => {
                      void (async () => {
                        setBusy(true)
                        setError(null)
                        setGitHubPackagesSyncResults(null)
                        try {
                          const input = githubPackagesNewRepo.trim()
                          const resolved = await resolveGitHubPackagesTarget(input)
                          if (resolved.kind === 'repo') {
                            const fullName = resolved.repos[0]?.fullName?.trim() ?? ''
                            if (!fullName) throw new Error('resolve returned empty repo')
                            await setGitHubPackagesRepoSelected({ fullName, selected: true })
                            setGitHubPackagesNewRepo('')
                            await refresh()
                            await refreshTrackedRepos()
                            return
                          }
                          if (resolved.kind === 'owner') {
                            let picked = resolved.repos.map((r) => ({ ...r }))
                            const ok = await confirm({
                              title: '选择要跟踪的仓库',
                              body: (
                                <GitHubPackagesRepoPicker
                                  initial={resolved}
                                  onChange={(next) => {
                                    picked = next
                                  }}
                                />
                              ),
                              confirmText: '确认',
                              cancelText: '取消',
                              confirmVariant: 'primary',
                              badgeText: null,
                            })
                            if (!ok) return
                            const selected = picked.filter((r) => r.selected).map((r) => r.fullName)
                            for (const fullName of selected) {
                              await setGitHubPackagesRepoSelected({ fullName, selected: true })
                            }
                            setGitHubPackagesNewRepo('')
                            await refresh()
                            await refreshTrackedRepos()
                            return
                          }
                          throw new Error(`unsupported resolve kind: ${resolved.kind}`)
                        } catch (e: unknown) {
                          setError(errorMessage(e))
                        } finally {
                          setBusy(false)
                        }
                      })()
                    }}
                  >
                    解析并添加
                  </Button>
                </div>
              </div>
            </div>

            {githubPackagesTrackedRepos?.repos?.length ? (
              <div style={{ marginTop: 10, display: 'flex', flexDirection: 'column', gap: 10 }}>
                {githubPackagesTrackedRepos.repos.map((r) => {
                  const dotClass = r.lastError
                    ? 'statusDot statusDotBad'
                    : r.hookId
                      ? 'statusDot statusDotOk'
                      : 'statusDot statusDotWarn'
                  const lastSync = r.lastSyncAt ? r.lastSyncAt : '-'
                  const hookId = r.hookId ? String(r.hookId) : '-'
                  return (
                    <div
                      key={r.fullName}
                      style={{
                        display: 'flex',
                        gap: 10,
                        alignItems: 'center',
                        justifyContent: 'space-between',
                        minWidth: 0,
                      }}
                    >
                      <div style={{ minWidth: 0, flex: '1 1 auto' }}>
                        <div style={{ display: 'flex', gap: 10, alignItems: 'center', minWidth: 0 }}>
                          <span className={dotClass} />
                          <div className="mono" style={{ overflowWrap: 'anywhere' }}>
                            {r.fullName}
                          </div>
                        </div>
                        <div className="muted" style={{ marginTop: 4, overflowWrap: 'anywhere' }}>
                          hookId: {hookId} · lastSyncAt: {lastSync}
                          {r.lastError ? ` · lastError: ${r.lastError}` : null}
                        </div>
                      </div>

                      <div style={{ display: 'flex', gap: 10, alignItems: 'center', flex: '0 0 auto' }}>
                        <IconButton
                          variant="ghost"
                          title="同步状态"
                          disabled={busy || !githubPackages.enabled}
                          onClick={() => {
                            void (async () => {
                              setBusy(true)
                              setError(null)
                              try {
                                const resp = await syncGitHubPackagesWebhooks({ dryRun: false, repos: [r.fullName] })
                                setGitHubPackagesSyncResults(resp.results)
                                await refreshTrackedRepos()
                              } catch (e: unknown) {
                                setError(errorMessage(e))
                              } finally {
                                setBusy(false)
                              }
                            })()
                          }}
                        >
                          <RefreshIcon className="uiIcon" />
                        </IconButton>

                        <IconButton
                          variant="ghost"
                          title="删除"
                          disabled={busy}
                          onClick={() => {
                            void (async () => {
                              const ok = await confirm({
                                title: '删除跟踪仓库',
                                body: (
                                  <div>
                                    <div className="modalLead">将反注册 webhook，并从列表中移除该仓库：</div>
                                    <div className="modalKvGrid">
                                      <div className="modalKvLabel">Repo</div>
                                      <div className="modalKvValue">
                                        <Mono>{r.fullName}</Mono>
                                      </div>
                                    </div>
                                  </div>
                                ),
                                confirmText: '删除',
                                cancelText: '取消',
                                confirmVariant: 'danger',
                                badgeText: '将删除 webhook',
                                badgeTone: 'bad',
                              })
                              if (!ok) return
                              setBusy(true)
                              setError(null)
                              try {
                                await deleteGitHubPackagesRepo({ fullName: r.fullName })
                                setGitHubPackagesSyncResults(null)
                                await refresh()
                                await refreshTrackedRepos()
                              } catch (e: unknown) {
                                setError(errorMessage(e))
                              } finally {
                                setBusy(false)
                              }
                            })()
                          }}
                        >
                          <TrashIcon className="uiIcon" />
                        </IconButton>
                      </div>
                    </div>
                  )
                })}
              </div>
            ) : (
              <div className="muted" style={{ marginTop: 10 }}>
                暂无已跟踪仓库：添加 org/repo，或粘贴 profile/org URL 批量选择
              </div>
            )}
          </div>

            {githubPackagesSyncResults ? (
              <div className="kv" style={{ marginTop: 10 }}>
                {githubPackagesSyncResults.map((r) => (
                  <div className="kvRow" key={`${r.repo}:${r.action}:${r.hookId ?? ''}`}>
                    <div className="label">{r.action}</div>
                    <div style={{ width: '100%' }}>
                      <div className="mono">{r.repo}</div>
                      {r.message ? <div className="muted">{r.message}</div> : null}
                      {r.action === 'conflict' && r.conflictHooks?.length ? (
                        <div style={{ marginTop: 6 }}>
                          <div className="muted">发现重复 webhook（同 callback URL + package 事件）：</div>
                          <div className="muted" style={{ marginTop: 6 }}>
                            {r.conflictHooks.map((h) => (
                              <div key={h.id}>
                                hook {h.id} active={String(h.active)} events=[{h.events.join(', ')}]
                              </div>
                            ))}
                          </div>
                          <Button
                            variant="ghost"
                            disabled={busy}
                            onClick={() => {
                              void (async () => {
                                const hooks = r.conflictHooks ?? []
                                if (hooks.length < 2) return
                                const keep = hooks[0]!
                                const del = hooks.slice(1).map((h) => h.id)
                                const ok = await confirm({
                                  title: '处理重复 webhook',
                                  body: (
                                    <div>
                                      <div className="modalLead">检测到重复 webhook：保留一个，删除其余并重试。</div>
                                      <div className="modalKvGrid">
                                        <div className="modalKvLabel">Repo</div>
                                        <div className="modalKvValue">
                                          <Mono>{r.repo}</Mono>
                                        </div>
                                        <div className="modalKvLabel">Keep</div>
                                        <div className="modalKvValue">
                                          <Mono>{String(keep.id)}</Mono>
                                        </div>
                                        <div className="modalKvLabel">Delete</div>
                                        <div className="modalKvValue">{del.map(String).join(', ')}</div>
                                      </div>
                                    </div>
                                  ),
                                  confirmText: '删除并重试',
                                  cancelText: '取消',
                                  confirmVariant: 'danger',
                                  badgeText: '会删除 webhook',
                                  badgeTone: 'bad',
                                })
                                if (!ok) return
                                setBusy(true)
                                setError(null)
                                try {
                                  const resp = await syncGitHubPackagesWebhooks({
                                    resolveConflicts: [{ repo: r.repo, keepHookId: keep.id, deleteHookIds: del }],
                                    repos: [r.repo],
                                  })
                                  setGitHubPackagesSyncResults(resp.results)
                                  await refresh()
                                } catch (e: unknown) {
                                  setError(errorMessage(e))
                                } finally {
                                  setBusy(false)
                                }
                              })()
                            }}
                          >
                            删除旧的并重试
                          </Button>
                        </div>
                      ) : null}
                    </div>
                  </div>
                ))}
              </div>
            ) : null}

          {error ? <div className="error">{error}</div> : null}
        </div>
        </div>
      </div>
    </div>
  )
}
