import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  addGitHubPackagesTarget,
  bulkSetGitHubPackagesReposSelected,
  createWebPushSubscription,
  deleteWebPushSubscription,
  getGitHubPackagesSettings,
  getNotifications,
  getSettings,
  listGitHubPackagesRepos,
  putGitHubPackagesSettings,
  putNotifications,
  putSettings,
  removeGitHubPackagesTarget,
  setGitHubPackagesRepoSelected,
  syncGitHubPackagesWebhooks,
  testNotifications,
  apiBaseUrl,
  type GitHubPackagesSettingsResponse,
  type ListGitHubPackagesReposResponse,
  type SyncGitHubPackagesWebhookResult,
  type NotificationConfig,
  type SettingsResponse,
} from '../api'
import { Button, Mono, Switch } from '../ui'
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

export function SettingsPage(props: { onTopActions: (node: React.ReactNode) => void }) {
  const { onTopActions } = props
  const [settings, setSettings] = useState<SettingsResponse | null>(null)
  const [notifications, setNotifications] = useState<NotificationConfig | null>(null)
  const [githubPackages, setGitHubPackages] = useState<GitHubPackagesSettingsResponse | null>(null)
  const [githubPackagesPat, setGitHubPackagesPat] = useState('')
  const [githubPackagesNewTarget, setGitHubPackagesNewTarget] = useState('')
  const [githubPackagesSyncResults, setGitHubPackagesSyncResults] = useState<SyncGitHubPackagesWebhookResult[] | null>(null)
  const [githubPackagesRepos, setGitHubPackagesRepos] = useState<ListGitHubPackagesReposResponse | null>(null)
  const [githubPackagesReposPage, setGitHubPackagesReposPage] = useState(1)
  const [githubPackagesReposPerPage, setGitHubPackagesReposPerPage] = useState(20)
  const [githubPackagesReposQ, setGitHubPackagesReposQ] = useState('')
  const [githubPackagesReposSelectedFilter, setGitHubPackagesReposSelectedFilter] = useState<'all' | 'selected' | 'unselected'>(
    'all',
  )
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [webPushEndpoint, setWebPushEndpoint] = useState<string | null>(null)
  const supervisor = useSupervisorHealth()
  const selfUpgradeUrl = useMemo(() => selfUpgradeBaseUrl(), [])

  const refreshRepos = useCallback(
    async (opts?: { page?: number; perPage?: number; q?: string; selectedFilter?: 'all' | 'selected' | 'unselected' }) => {
      const page = opts?.page ?? githubPackagesReposPage
      const perPage = opts?.perPage ?? githubPackagesReposPerPage
      const q = (opts?.q ?? githubPackagesReposQ).trim()
      const selectedFilter = opts?.selectedFilter ?? githubPackagesReposSelectedFilter
      setGitHubPackagesRepos(
        await listGitHubPackagesRepos({
          page,
          perPage,
          q: q || null,
          selectedFilter,
        }),
      )
    },
    [githubPackagesReposPage, githubPackagesReposPerPage, githubPackagesReposQ, githubPackagesReposSelectedFilter],
  )

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
    setGitHubPackagesSyncResults(null)
  }, [])

  useEffect(() => {
    void (async () => {
      await refresh()
      setGitHubPackagesReposPage(1)
      await refreshRepos({ page: 1 })
    })().catch((e: unknown) => setError(errorMessage(e)))
  }, [refresh, refreshRepos])

  useEffect(() => {
    void refreshRepos({ page: githubPackagesReposPage }).catch((e: unknown) => setError(errorMessage(e)))
  }, [githubPackagesReposPage, refreshRepos])

  useEffect(() => {
    const t = window.setTimeout(() => {
      void refreshRepos({ page: 1 }).catch((e: unknown) => setError(errorMessage(e)))
    }, 250)
    return () => window.clearTimeout(t)
  }, [githubPackagesReposPerPage, githubPackagesReposQ, githubPackagesReposSelectedFilter, refreshRepos])

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
              <div className="sectionTitle">Targets</div>
              <div className="muted">{githubPackages.targets.length} 个</div>
            </div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">新增 Target</div>
                <div style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
                  <input
                    className="input"
                    value={githubPackagesNewTarget}
                    onChange={(e) => setGitHubPackagesNewTarget(e.target.value)}
                    placeholder="https://github.com/org/repo 或 https://github.com/org 或 org"
                    style={{ flex: 1 }}
                  />
                  <Button
                    variant="ghost"
                    disabled={busy || !githubPackagesNewTarget.trim()}
                    onClick={() => {
                      void (async () => {
                        setBusy(true)
                        setError(null)
                        try {
                          const input = githubPackagesNewTarget.trim()
                          await addGitHubPackagesTarget({ input })
                          setGitHubPackagesNewTarget('')
                          await refresh()
                          setGitHubPackagesReposPage(1)
                          await refreshRepos({ page: 1 })
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

            {githubPackages.targets.length ? (
              <div className="kv" style={{ marginTop: 10 }}>
                {githubPackages.targets.map((t) => (
                  <div className="kvRow" key={t.input}>
                    <div className="label">Target</div>
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10 }}>
                      <div className="mono">
                        {t.input} <span className="muted">({t.kind}:{t.owner})</span>
                      </div>
                      <Button
                        variant="ghost"
                        disabled={busy}
                        onClick={() => {
                          void (async () => {
                            const ok = window.confirm(
                              `移除 target: ${t.input}\n\n注意：当前版本不会自动删除 repos 记录（仅移除 target）。`,
                            )
                            if (!ok) return
                            setBusy(true)
                            setError(null)
                            try {
                              await removeGitHubPackagesTarget({ input: t.input })
                              await refresh()
                              setGitHubPackagesReposPage(1)
                              await refreshRepos({ page: 1 })
                            } catch (e: unknown) {
                              setError(errorMessage(e))
                            } finally {
                              setBusy(false)
                            }
                          })()
                        }}
                      >
                        移除
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="muted" style={{ marginTop: 10 }}>
                尚未添加 target（可直接粘贴 repo URL / profile URL / username）
              </div>
            )}
          </div>

          <div className="settingsSection">
            <div className="settingHead">
              <div className="sectionTitle">Repos</div>
              <div className="muted">
                {githubPackagesRepos ? (
                  <>
                    {githubPackagesRepos.filteredTotal} / {githubPackagesRepos.total} 个，已选 {githubPackagesRepos.selectedTotal} 个
                  </>
                ) : (
                  '加载中…'
                )}
              </div>
            </div>

            <div className="kv" style={{ marginTop: 10 }}>
              <div className="kvRow">
                <div className="label">筛选</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10, width: '100%' }}>
                  <input
                    className="input"
                    style={{ flex: 1 }}
                    value={githubPackagesReposQ}
                    disabled={busy}
                    onChange={(e) => {
                      setGitHubPackagesReposQ(e.target.value)
                      setGitHubPackagesReposPage(1)
                    }}
                    placeholder="搜索 owner/repo"
                  />
                  <select
                    className="input"
                    value={githubPackagesReposSelectedFilter}
                    disabled={busy}
                    onChange={(e) => {
                      const v = e.target.value as 'all' | 'selected' | 'unselected'
                      setGitHubPackagesReposSelectedFilter(v)
                      setGitHubPackagesReposPage(1)
                    }}
                  >
                    <option value="all">全部</option>
                    <option value="selected">已选</option>
                    <option value="unselected">未选</option>
                  </select>
                  <select
                    className="input"
                    value={String(githubPackagesReposPerPage)}
                    disabled={busy}
                    onChange={(e) => {
                      setGitHubPackagesReposPerPage(Number(e.target.value) || 20)
                      setGitHubPackagesReposPage(1)
                    }}
                  >
                    <option value="20">20/页</option>
                    <option value="50">50/页</option>
                    <option value="100">100/页</option>
                  </select>
                </div>
              </div>
              <div className="kvRow">
                <div className="label">批量</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10, width: '100%', justifyContent: 'space-between' }}>
                  <div className="muted">
                    对“当前筛选结果”批量设置（支持几百条/上千条，不会一次性渲染全部）
                  </div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                    <Button
                      variant="ghost"
                      disabled={busy}
                      onClick={() => {
                        void (async () => {
                          setBusy(true)
                          setError(null)
                          try {
                            await bulkSetGitHubPackagesReposSelected({
                              q: githubPackagesReposQ.trim() || null,
                              selectedFilter: githubPackagesReposSelectedFilter,
                              selected: true,
                            })
                            await refreshRepos()
                            await refresh()
                          } catch (e: unknown) {
                            setError(errorMessage(e))
                          } finally {
                            setBusy(false)
                          }
                        })()
                      }}
                    >
                      全选
                    </Button>
                    <Button
                      variant="ghost"
                      disabled={busy}
                      onClick={() => {
                        void (async () => {
                          setBusy(true)
                          setError(null)
                          try {
                            await bulkSetGitHubPackagesReposSelected({
                              q: githubPackagesReposQ.trim() || null,
                              selectedFilter: githubPackagesReposSelectedFilter,
                              selected: false,
                            })
                            await refreshRepos()
                            await refresh()
                          } catch (e: unknown) {
                            setError(errorMessage(e))
                          } finally {
                            setBusy(false)
                          }
                        })()
                      }}
                    >
                      全不选
                    </Button>
                  </div>
                </div>
              </div>
            </div>

            {githubPackagesRepos?.repos?.length ? (
              <div
                className="kv"
                style={{ marginTop: 10, maxHeight: 420, overflowY: 'auto', paddingRight: 6, overscrollBehavior: 'contain' }}
              >
                {githubPackagesRepos.repos.map((r) => (
                  <div className="kvRow" key={r.fullName} style={{ gridTemplateColumns: '28px 1fr' }}>
                    <div className="label">
                      <input
                        type="checkbox"
                        checked={r.selected}
                        disabled={busy}
                        onChange={(e) => {
                          void (async () => {
                            const selected = e.target.checked
                            setBusy(true)
                            setError(null)
                            try {
                              await setGitHubPackagesRepoSelected({ fullName: r.fullName, selected })
                              await refreshRepos()
                              await refresh()
                            } catch (e: unknown) {
                              setError(errorMessage(e))
                            } finally {
                              setBusy(false)
                            }
                          })()
                        }}
                      />
                    </div>
                    <div style={{ width: '100%' }}>
                      <div className="mono">{r.fullName}</div>
                      {r.hookId ? <div className="muted">hookId: {r.hookId}</div> : null}
                      {r.lastError ? <div className="muted">lastError: {r.lastError}</div> : null}
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="muted" style={{ marginTop: 10 }}>
                repo 列表为空：先添加 target，然后在这里分页浏览与勾选
              </div>
            )}

            {githubPackagesRepos ? (
              <div className="formActions" style={{ marginTop: 10, justifyContent: 'space-between' }}>
                <div className="muted">
                  第 {githubPackagesRepos.page} 页（每页 {githubPackagesRepos.perPage}）
                </div>
                <div style={{ display: 'flex', gap: 10 }}>
                  <Button
                    variant="ghost"
                    disabled={busy || githubPackagesRepos.page <= 1}
                    onClick={() => setGitHubPackagesReposPage((p) => Math.max(1, p - 1))}
                  >
                    上一页
                  </Button>
                  <Button
                    variant="ghost"
                    disabled={
                      busy ||
                      githubPackagesRepos.page >= Math.max(1, Math.ceil(githubPackagesRepos.filteredTotal / githubPackagesRepos.perPage))
                    }
                    onClick={() => setGitHubPackagesReposPage((p) => p + 1)}
                  >
                    下一页
                  </Button>
                </div>
              </div>
            ) : null}

            <div className="formActions" style={{ marginTop: 10 }}>
              <Button
                variant="ghost"
                disabled={busy || !(githubPackagesRepos?.selectedTotal ?? 0)}
                onClick={() => {
                  void (async () => {
                    setBusy(true)
                    setError(null)
                    try {
                      const resp = await syncGitHubPackagesWebhooks({ dryRun: false })
                      setGitHubPackagesSyncResults(resp.results)
                      await refreshRepos()
                      await refresh()
                    } catch (e: unknown) {
                      setError(errorMessage(e))
                    } finally {
                      setBusy(false)
                    }
                  })()
                }}
              >
                同步 webhook
              </Button>
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
                                const ok = window.confirm(`检测到重复 webhook：保留 ${keep.id}，删除其余 ${del.length} 个？`)
                                if (!ok) return
                                setBusy(true)
                                setError(null)
                                try {
                                  const resp = await syncGitHubPackagesWebhooks({
                                    resolveConflicts: [{ repo: r.repo, keepHookId: keep.id, deleteHookIds: del }],
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
    </div>
  )
}
