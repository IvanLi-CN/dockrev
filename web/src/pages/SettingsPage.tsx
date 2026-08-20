import { FloatingPortal } from '@floating-ui/react'
import eyeOffOutline from '@iconify-icons/mdi/eye-off-outline'
import eyeOutline from '@iconify-icons/mdi/eye-outline'
import { Icon } from '@iconify/react'
import {
resolveGitHubPackagesTarget,
setGitHubPackagesRepoSelected
} from '../api'
import {
NotificationChannelCard
} from '../components/NotificationChannelCard'
import { SettingsMobileIdentity } from '../components/SettingsMobileIdentity'
import { SettingsMobileNavigation } from '../components/SettingsMobileNavigation'
import { navigate } from '../routes'
import type { SettingsSection } from '../routes'
import {
SETTINGS_GHCR_WEBHOOK_ID
} from '../settingsFocus'
import { Button,Input,Mono,SelectField,Switch } from '../ui'
import { AsyncDataRegion, AsyncDataSkeleton } from '../components/AsyncDataRegion'
import { webhookStateDotClass,webhookStateIcon } from '../webhookStatus'
import { GitHubPackagesRepoPicker } from './settings/GitHubPackagesRepoPicker'
import {
GHCR_PREVIEW_LIMIT,
errorMessage,
formatBytes,
isMaskedSecretLiteral,
mapResolveFailure,
normalizeNotificationEvents,
normalizeWebhookState,
webhookStateLabel
} from './settings/helpers'
import { useSettingsPageState } from './useSettingsPageState'
export function SettingsPage(props: { section?: SettingsSection; onTopActions: (node: React.ReactNode) => void }) {
  const {
    autoSaveIssue,
    autoSavePhase,
    autoSaveStatusText,
    autoSaveToastClassName,
    busy,
    canWebPush,
    clearOctoRillApiKeyMaskForEdit,
    clearTelegramBotTokenMaskForEdit,
    confirm,
    dismissInstancePublicBaseUrlSuggestBubble,
    error,
    fillInstancePublicBaseUrlFromCurrentOrigin,
    flushAutoSave,
    ghcrLiveProgressText,
    ghcrPatIssue,
    ghcrResolvePending,
    ghcrWebhookAuditCronInputClassName,
    githubPackages,
    githubPackagesLoadError,
    githubPackagesLoadPhase,
    githubPackagesNewRepo,
    githubPackagesPat,
    githubPackagesTrackedRepos,
    instancePublicBaseUrlSuggestFloatingStyles,
    instancePublicBaseUrlSuggestPlacement,
    instancePublicBaseUrlValue,
    notificationTestRunning,
    notificationTestStates,
    notifications,
    notificationsLoadError,
    notificationsLoadPhase,
    openGhcrRegistry,
    octoRillApiBaseUrlInputClassName,
    octoRillApiKeyFocused,
    octoRillApiKeyTouched,
    refresh,
    refreshTrackedRepos,
    restoreOctoRillApiKeyMaskIfNeeded,
    restoreTelegramBotTokenMaskIfNeeded,
    runNotificationChannelTest,
    selfUpgradeUrl,
    ensureSubscription,
    removeSubscription,
    setBusy,
    setError,
    setGhcrResolvePending,
    setGitHubPackagesNewRepo,
    setInstancePublicBaseUrlSuggestFloating,
    setInstancePublicBaseUrlSuggestReference,
    setOctoRillApiKeyFocused,
    setOctoRillApiKeyTouched,
    setTelegramBotTokenFocused,
    setTelegramBotTokenTouched,
    setTelegramBotTokenVisible,
    settings,
    settingsLoadError,
    settingsLoadPhase,
    showAutoSaveToast,
    showInstancePublicBaseUrlSuggestBubble,
    showTelegramBotTokenEye,
    suggestedPublicBaseUrl,
    trackedReposLoadError,
    trackedReposLoadPhase,
    trackedReposLoadSource,
    trackedReposLoadTrigger,
    supervisor,
    telegramBotTokenInputClassName,
    telegramBotTokenVisible,
    updateBackup,
    updateCheckCronInputClassName,
    updateGhcr,
    updateInstance,
    updateReleaseNotes,
    updateNotifications,
    updateResourceMonitor,
    updateSchedules,
    webPushEndpoint,
    loadSource,
    loadTrigger,
  } = useSettingsPageState(props)
  if (!settings) {
    return (
      <div className="page settingsPage" data-mobile-settings-section={props.section ?? 'index'}>
        <AsyncDataRegion
          error={settingsLoadError}
          hasData={false}
          label="正在加载系统设置"
          onRetry={() => void refresh({ source: 'memory', trigger: 'user-action' })}
          phase={settingsLoadPhase}
          skeleton={<AsyncDataSkeleton className="settingsLoadingSkeleton" lines={10} />}
          source={loadSource}
          trigger={loadTrigger}
        />
      </div>
    )
  }
  return (
    <div className="page settingsPage" data-mobile-settings-section={props.section ?? 'index'}>
      {!props.section ? <SettingsMobileIdentity auth={settings.auth} /> : null}
      <SettingsMobileNavigation section={props.section} />
      <div className="twoCol settingsContentGrid">
        <div className="settingsCol">
          <AsyncDataRegion
            className="settingsCoreRegion"
            error={settingsLoadError}
            hasData
            label="正在刷新基础设置"
            onRetry={() => void refresh({ source: 'memory', trigger: 'user-action', domains: ['settings'] })}
            phase={settingsLoadPhase}
            source={loadSource}
            trigger={loadTrigger}
          >
          <div className="card settingsSectionCard" data-settings-section="account" data-mobile-active={props.section === 'account' || undefined}>
            <div className="title">鉴权（Forward Auth）</div>
            <div className="muted">认证由入口代理负责；Dockrev 按用户/组执行项目侧鉴权（运行时只读）</div>
            <div className="kv">
              <div className="kvRow">
                <div className="label">用户头</div>
                <div className="mono">{settings.auth.forwardHeaderName}</div>
              </div>
              <div className="kvRow">
                <div className="label">组头</div>
                <div className="mono">{settings.auth.groupHeaderName}</div>
              </div>
              <div className="kvRow">
                <div className="label">鉴权模式</div>
                <div className="mono">{settings.auth.authorizationMode}</div>
              </div>
              <div className="kvRow">
                <div className="label">允许用户</div>
                <div className="mono">{settings.auth.allowedUserMasked || '-'}</div>
              </div>
              <div className="kvRow">
                <div className="label">允许组</div>
                <div className="mono">{settings.auth.allowedGroupMasked || '-'}</div>
              </div>

              <div className="kvRow">
                <div className="label">当前用户</div>
                <div className="mono">{settings.auth.currentUser || '-'}</div>
              </div>

              <div className="kvRow">
                <div className="label">当前组</div>
                <div className="mono">{settings.auth.currentGroups.length ? settings.auth.currentGroups.join(', ') : '-'}</div>
              </div>

              <div className="kvRow">
                <div className="label">命中方式</div>
                <div className="mono">{settings.auth.matchedBy || '-'}</div>
              </div>

              <div className="kvRow">
                <div className="label">允许匿名（开发环境）</div>
                <div className="muted">{settings.auth.allowAnonymousInDev ? 'on' : 'off'}</div>
              </div>

              <div className="muted" style={{ marginTop: 6 }}>
                该区域由启动配置控制：`DOCKREV_AUTH_FORWARD_HEADER_NAME` / `DOCKREV_AUTH_GROUP_HEADER_NAME` /
                `DOCKREV_AUTH_ALLOWED_USER` / `DOCKREV_AUTH_ALLOWED_GROUP` /
                `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV`，修改后需重启服务生效。
              </div>
            </div>
          </div>

          <div className="card settingsSectionCard" data-settings-section="maintenance" data-mobile-active={props.section === 'maintenance' || undefined}>
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

          <div className="card settingsSectionCard" data-settings-section="maintenance" data-mobile-active={props.section === 'maintenance' || undefined}>
            <div className="title">部署检查</div>
            <div className="muted">手动打开部署检查清单页，不会修改“自动打开”偏好。</div>

            <div className="formActions">
              <Button
                variant="primary"
                disabled={busy}
                onClick={() => {
                  navigate({ name: 'deploy-check' })
                }}
              >
                打开部署检查页
              </Button>
            </div>
          </div>

          <div className="card settingsSectionCard" data-settings-section="backup" data-mobile-active={props.section === 'backup' || undefined}>
            <div className="title">备份默认策略</div>
            <div className="muted">默认 fail-closed；目标过大可按阈值跳过（force 可覆盖）</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">启用更新前备份</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={settings.backup.enabled}
                    disabled={busy}
                    onChange={(v) =>
                      updateBackup('backup.enabled', (backup) => ({ ...backup, enabled: v }), true)
                    }
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
                    onChange={(v) =>
                      updateBackup('backup.requireSuccess', (backup) => ({ ...backup, requireSuccess: v }), true)
                    }
                  />
                  <div className="muted">{settings.backup.requireSuccess ? 'on' : 'off'}</div>
                </div>
              </div>
              <div className="kvRow">
                <div className="label">备份输出目录</div>
                <div>
                  <div className="mono">{settings.backup.storage.logicalPath}</div>
                  <div className="muted" style={{ marginTop: 6 }}>
                    {settings.backup.storage.resolvedLocation} · {settings.backup.storage.mode} ·{' '}
                    {settings.backup.storage.writable ? '可写' : '不可写'}
                  </div>
                  {settings.backup.storage.diagnostic ? (
                    <div className="muted" style={{ marginTop: 6 }}>{settings.backup.storage.diagnostic}</div>
                  ) : null}
                </div>
              </div>
              <div className="kvRow">
                <div className="label">体积阈值（超过则跳过）</div>
                <div>
                  <Input
                    className="input"
                    value={String(settings.backup.skipTargetsOverBytes)}
                    onChange={(e) =>
                      updateBackup('backup.skipTargetsOverBytes', (backup) => ({
                        ...backup,
                        skipTargetsOverBytes: Number(e.target.value) || 0,
                      }))
                    }
                  />
                  <div className="muted" style={{ marginTop: 6 }}>
                    当前：{formatBytes(settings.backup.skipTargetsOverBytes)}
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div className="card settingsSectionCard" data-settings-section="monitoring" data-mobile-active={props.section === 'monitoring' || undefined}>
            <div className="title">资源监控</div>
            <div className="muted">控制全局协调的历史采样周期，以及单服务 1s 实时 SSE 推送。</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">启用资源监控</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={settings.resourceMonitor.enabled}
                    disabled={busy}
                    onChange={(value) =>
                      updateResourceMonitor(
                        'settings.resourceMonitor.enabled',
                        (current) => ({ ...current, enabled: value }),
                        true,
                      )
                    }
                  />
                  <div className="muted">{settings.resourceMonitor.enabled ? 'on' : 'off'}</div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">历史采样频率（全局周期）</div>
                <div>
                  <SelectField
                    className="input"
                    disabled={busy || !settings.resourceMonitor.enabled}
                    onChange={(value) => {
                      const next = Number(value)
                      if (![5, 10, 30, 60, 300].includes(next)) return
                      updateResourceMonitor('settings.resourceMonitor.sampleIntervalSeconds', (current) => ({
                        ...current,
                        sampleIntervalSeconds: next as 5 | 10 | 30 | 60 | 300,
                      }))
                    }}
                    options={[
                      { value: '5', label: '5 秒' },
                      { value: '10', label: '10 秒' },
                      { value: '30', label: '30 秒' },
                      { value: '60', label: '60 秒' },
                      { value: '300', label: '300 秒' },
                    ]}
                    value={String(settings.resourceMonitor.sampleIntervalSeconds)}
                  />
                  <div className="muted" style={{ marginTop: 6 }}>
                    每个周期只发现一次运行容器；历史与活跃 SSE 复用项目采集，过期周期会跳过且不补跑。
                  </div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">历史保留</div>
                <div className="muted">{settings.resourceMonitor.retentionDays} 天（固定）</div>
              </div>
            </div>
          </div>

          <div className="card settingsSectionCard" data-settings-section="schedules" data-mobile-active={props.section === 'schedules' || undefined}>
            <div className="title">定时任务</div>
            <div className="muted">cron 按服务端本地时区解释（TZ）；5 段表达式会自动补秒=0。</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">定期检查更新</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={settings.schedules.updateCheck.enabled}
                    disabled={busy}
                    onChange={(value) =>
                      updateSchedules(
                        'schedules.updateCheck.enabled',
                        (current) => ({
                          ...current,
                          updateCheck: { ...current.updateCheck, enabled: value },
                        }),
                        true,
                      )
                    }
                  />
                  <div className="muted">{settings.schedules.updateCheck.enabled ? 'on' : 'off'}</div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">Cron（检查更新）</div>
                <div>
                  <Input
                    className={updateCheckCronInputClassName}
                    disabled={busy}
                    value={settings.schedules.updateCheck.cron}
                    onChange={(e) =>
                      updateSchedules('schedules.updateCheck.cron', (current) => ({
                        ...current,
                        updateCheck: { ...current.updateCheck, cron: e.target.value },
                      }))
                    }
                    placeholder="*/30 * * * *"
                  />
                  <div className="muted" style={{ marginTop: 6 }}>
                    只创建检查任务（check.all），不自动更新。
                  </div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">Webhook 巡查（GHCR audit_all）</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={settings.schedules.ghcrWebhookAudit.enabled}
                    disabled={busy}
                    onChange={(value) =>
                      updateSchedules(
                        'schedules.ghcrWebhookAudit.enabled',
                        (current) => ({
                          ...current,
                          ghcrWebhookAudit: { ...current.ghcrWebhookAudit, enabled: value },
                        }),
                        true,
                      )
                    }
                  />
                  <div className="muted">{settings.schedules.ghcrWebhookAudit.enabled ? 'on' : 'off'}</div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">Cron（Webhook 巡查）</div>
                <div>
                  <Input
                    className={ghcrWebhookAuditCronInputClassName}
                    disabled={busy}
                    value={settings.schedules.ghcrWebhookAudit.cron}
                    onChange={(e) =>
                      updateSchedules('schedules.ghcrWebhookAudit.cron', (current) => ({
                        ...current,
                        ghcrWebhookAudit: { ...current.ghcrWebhookAudit, cron: e.target.value },
                      }))
                    }
                    placeholder="0 3 * * *"
                  />
                  <div className="muted" style={{ marginTop: 6 }}>
                    只巡检与标记 drift，不自动修复。
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div className="card settingsSectionCard" data-settings-section="release-notes" data-mobile-active={props.section === 'release-notes' || undefined}>
            <div className="title">OctoRill 更新日志</div>
            <div className="muted">统一 release notes 数据源由这里全局决定；版本页和发布抽屉只服从这里的设置，不会在别处自动切换或回退。</div>

            <div className="kv">
              <div className="settingsFieldRow">
                <div className="label settingsFieldLabel">数据源</div>
                <div className="settingsFieldControl">
                  <SelectField
                    className="input"
                    disabled={busy}
                    value={settings.releaseNotes.provider}
                    onChange={(value) =>
                      updateReleaseNotes(
                        'releaseNotes.provider',
                        (current) => ({
                          ...current,
                          provider: value,
                        }),
                        true,
                      )
                    }
                    options={[
                      { value: 'gitHub', label: 'GitHub Releases' },
                      { value: 'octoRill', label: 'OctoRill' },
                    ]}
                  />
                </div>
                <div className="muted settingsFieldHelp">
                  设成啥就用啥；若所选源失败，只会显示同源旧结果或错误态，不会跨源补位。
                </div>
              </div>

              <div className="settingsFieldRow">
                <div className="label settingsFieldLabel">API Base URL</div>
                <div className="settingsFieldControl">
                  <Input
                    className={octoRillApiBaseUrlInputClassName}
                    disabled={busy}
                    value={settings.releaseNotes.octoRill.apiBaseUrl ?? ''}
                    onChange={(e) =>
                      updateReleaseNotes('releaseNotes.octoRill.apiBaseUrl', (current) => ({
                        ...current,
                        octoRill: { ...current.octoRill, apiBaseUrl: e.target.value },
                      }))
                    }
                    placeholder="https://octo.example.com"
                  />
                </div>
                <div className="muted settingsFieldHelp">
                  保存时会去掉尾部 <Mono>/</Mono>；不要填写带账号密码的 URL。
                </div>
              </div>

              <div className="settingsFieldRow">
                <div className="label settingsFieldLabel">API Key</div>
                <div className="settingsFieldControl">
                  <Input
                    className="input"
                    disabled={busy}
                    type="password"
                    autoComplete="new-password"
                    value={
                      octoRillApiKeyFocused &&
                      !octoRillApiKeyTouched &&
                      isMaskedSecretLiteral(settings.releaseNotes.octoRill.apiKey ?? '')
                        ? ''
                        : settings.releaseNotes.octoRill.apiKey ?? ''
                    }
                    onFocus={() => {
                      setOctoRillApiKeyFocused(true)
                      setOctoRillApiKeyTouched(false)
                      clearOctoRillApiKeyMaskForEdit()
                    }}
                    onBlur={() => {
                      setOctoRillApiKeyFocused(false)
                      restoreOctoRillApiKeyMaskIfNeeded()
                    }}
                    onChange={(e) => {
                      setOctoRillApiKeyTouched(true)
                      updateReleaseNotes('releaseNotes.octoRill.apiKey', (current) => ({
                        ...current,
                        octoRill: { ...current.octoRill, apiKey: e.target.value },
                      }))
                    }}
                    placeholder="orill_ak_..."
                  />
                </div>
                <div className="muted settingsFieldHelp">
                  已保存时显示等长圆点掩码；清空后自动保存会删除当前 key。
                </div>
              </div>

              <div className="settingsFieldRow">
                <div className="label settingsFieldLabel">默认视图</div>
                <div className="settingsFieldControl">
                  <SelectField
                    className="input"
                    disabled={busy || settings.releaseNotes.provider !== 'octoRill'}
                    value={settings.releaseNotes.octoRill.defaultView}
                    onChange={(value) =>
                      updateReleaseNotes('releaseNotes.octoRill.defaultView', (current) => ({
                        ...current,
                        octoRill: { ...current.octoRill, defaultView: value },
                      }))
                    }
                    options={[
                      { value: 'smart', label: '润色' },
                      { value: 'translated', label: '翻译' },
                      { value: 'original', label: '原文' },
                    ]}
                  />
                </div>
                <div className="muted settingsFieldHelp">
                  {settings.releaseNotes.provider === 'octoRill'
                    ? '仅 OctoRill 数据源会使用这里的默认阅读视图。'
                    : '当前选择 GitHub Releases，阅读视图固定为原文。'}
                </div>
              </div>
            </div>
          </div>

          <div className="card settingsSectionCard" data-settings-section="integrations" data-mobile-active={props.section === 'integrations' || undefined}>
            <div className="title">实例 Public Base URL</div>
            <div className="muted">用于在通知中生成可点击的绝对链接（服务详情 / 任务详情）。</div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">Public Base URL</div>
                <div>
                  <Input
                    ref={setInstancePublicBaseUrlSuggestReference}
                    className="input"
                    value={instancePublicBaseUrlValue}
                    onChange={(e) =>
                      updateInstance('instance.publicBaseUrl', (current) => ({
                        ...current,
                        publicBaseUrl: e.target.value,
                      }))
                    }
                    placeholder="https://dockrev.example.com/"
                  />
                  {showInstancePublicBaseUrlSuggestBubble && suggestedPublicBaseUrl ? (
                    <FloatingPortal>
                      <div
                        ref={setInstancePublicBaseUrlSuggestFloating}
                        className="settingsInlineSuggestionBubble"
                        style={instancePublicBaseUrlSuggestFloatingStyles}
                        role="status"
                        aria-live="polite"
                        data-placement={instancePublicBaseUrlSuggestPlacement}
                        data-settings-public-base-url-suggestion="visible"
                      >
                        <div className="settingsInlineSuggestionText">
                          <span>是否使用当前地址</span>
                          <Mono>{suggestedPublicBaseUrl}</Mono>
                          <span>？</span>
                        </div>
                        <div className="settingsInlineSuggestionActions">
                          <Button variant="primary" disabled={busy} onClick={fillInstancePublicBaseUrlFromCurrentOrigin}>
                            自动填入
                          </Button>
                          <Button disabled={busy} onClick={dismissInstancePublicBaseUrlSuggestBubble}>
                            不
                          </Button>
                        </div>
                      </div>
                    </FloatingPortal>
                  ) : null}
                  <div className="muted" style={{ marginTop: 6 }}>
                    为空表示不配置；保存时会自动补齐尾部 <Mono>/</Mono>
                  </div>
                </div>
              </div>
            </div>
          </div>

          </AsyncDataRegion>

          {notifications ? (
          <AsyncDataRegion
            className="settingsNotificationsRegion"
            error={notificationsLoadError}
            hasData
            label="正在刷新通知设置"
            onRetry={() => void refresh({ source: 'memory', trigger: 'user-action', domains: ['notifications'] })}
            phase={notificationsLoadPhase}
            source={loadSource}
            trigger={loadTrigger}
          >
          <div className="card settingsSectionCard" data-settings-section="notifications" data-mobile-active={props.section === 'notifications' || undefined}>
            <div className="title">通知</div>
            <div className="muted">先选择通知事件，再为每个渠道配置发送方式。</div>

            <div className="kv" style={{ marginBottom: 12 }}>
              <div className="kvRow">
                <div className="label">更新完成通知</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={normalizeNotificationEvents(notifications.events).update}
                    disabled={busy}
                    onChange={(value) =>
                      updateNotifications(
                        'notifications.events.update',
                        (current) => ({
                          ...current,
                          events: { ...normalizeNotificationEvents(current.events), update: value },
                        }),
                        true,
                      )
                    }
                  />
                  <div className="muted">{normalizeNotificationEvents(notifications.events).update ? 'on' : 'off'}</div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">发现新版本通知（定时检查）</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={normalizeNotificationEvents(notifications.events).newVersion}
                    disabled={busy}
                    onChange={(value) =>
                      updateNotifications(
                        'notifications.events.newVersion',
                        (current) => ({
                          ...current,
                          events: { ...normalizeNotificationEvents(current.events), newVersion: value },
                        }),
                        true,
                      )
                    }
                  />
                  <div className="muted">{normalizeNotificationEvents(notifications.events).newVersion ? 'on' : 'off'}</div>
                </div>
              </div>

              <div className="kvRow">
                <div className="label">GitHub Webhook 异常通知（巡检）</div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                  <Switch
                    checked={normalizeNotificationEvents(notifications.events).ghcrWebhookAnomaly}
                    disabled={busy}
                    onChange={(value) =>
                      updateNotifications(
                        'notifications.events.ghcrWebhookAnomaly',
                        (current) => ({
                          ...current,
                          events: { ...normalizeNotificationEvents(current.events), ghcrWebhookAnomaly: value },
                        }),
                        true,
                      )
                    }
                  />
                  <div className="muted">
                    {normalizeNotificationEvents(notifications.events).ghcrWebhookAnomaly ? 'on' : 'off'}
                  </div>
                </div>
              </div>
            </div>

            <NotificationChannelCard
              channel="email"
              title="Email"
              enabled={notifications.email.enabled}
              busy={busy}
              testState={notificationTestStates.email}
              testRunning={Boolean(notificationTestRunning.email)}
              onRunTest={runNotificationChannelTest}
              onToggleEnabled={(v) =>
                updateNotifications(
                  'notifications.email.enabled',
                  (current) => ({ ...current, email: { ...current.email, enabled: v } }),
                  true,
                )
              }
            >
              <div className="kvRow">
                <div className="label">SMTP URL</div>
                <Input
                  className="input"
                  value={notifications.email.smtpUrl ?? ''}
                  onChange={(e) =>
                    updateNotifications('notifications.email.smtpUrl', (current) => ({
                      ...current,
                      email: { ...current.email, smtpUrl: e.target.value },
                    }))
                  }
                  placeholder="smtp://user:pass@smtp.example.com:587"
                />
              </div>
            </NotificationChannelCard>

            <NotificationChannelCard
              channel="webhook"
              title="Webhook"
              enabled={notifications.webhook.enabled}
              busy={busy}
              testState={notificationTestStates.webhook}
              testRunning={Boolean(notificationTestRunning.webhook)}
              onRunTest={runNotificationChannelTest}
              onToggleEnabled={(v) =>
                updateNotifications(
                  'notifications.webhook.enabled',
                  (current) => ({ ...current, webhook: { ...current.webhook, enabled: v } }),
                  true,
                )
              }
            >
              <div className="kvRow">
                <div className="label">URL</div>
                <Input
                  className="input"
                  value={notifications.webhook.url ?? ''}
                  onChange={(e) =>
                    updateNotifications('notifications.webhook.url', (current) => ({
                      ...current,
                      webhook: { ...current.webhook, url: e.target.value },
                    }))
                  }
                  placeholder="https://hooks.example.com/dockrev"
                />
              </div>
            </NotificationChannelCard>

            <NotificationChannelCard
              channel="telegram"
              title="Telegram"
              enabled={notifications.telegram.enabled}
              busy={busy}
              testState={notificationTestStates.telegram}
              testRunning={Boolean(notificationTestRunning.telegram)}
              onRunTest={runNotificationChannelTest}
              onToggleEnabled={(v) =>
                updateNotifications(
                  'notifications.telegram.enabled',
                  (current) => ({ ...current, telegram: { ...current.telegram, enabled: v } }),
                  true,
                )
              }
            >
              <div className="kvRow">
                <div className="label">Bot token</div>
                <div className={showTelegramBotTokenEye ? 'inputWithAction' : undefined}>
                  <Input
                    className={telegramBotTokenInputClassName}
                    type={telegramBotTokenVisible ? 'text' : 'password'}
                    autoComplete="new-password"
                    value={notifications.telegram.botToken ?? ''}
                    onFocus={() => {
                      setTelegramBotTokenFocused(true)
                      clearTelegramBotTokenMaskForEdit()
                    }}
                    onBlur={() => {
                      setTelegramBotTokenFocused(false)
                      setTelegramBotTokenVisible(false)
                      restoreTelegramBotTokenMaskIfNeeded()
                    }}
                    onChange={(e) => {
                      setTelegramBotTokenTouched(true)
                      updateNotifications('notifications.telegram.botToken', (current) => ({
                        ...current,
                        telegram: {
                          ...current.telegram,
                          botToken: e.target.value,
                          botTokenConfigured:
                            e.target.value.trim().length > 0 ? true : (current.telegram.botTokenConfigured ?? false),
                        },
                      }))
                    }}
                  />
                  {showTelegramBotTokenEye ? (
                    <button
                      type="button"
                      className="inputActionBtn"
                      aria-label={telegramBotTokenVisible ? '隐藏 Bot token' : '显示 Bot token'}
                      title={telegramBotTokenVisible ? '隐藏 Bot token' : '显示 Bot token'}
                      onClick={() => setTelegramBotTokenVisible((prev) => !prev)}
                    >
                      <Icon icon={telegramBotTokenVisible ? eyeOffOutline : eyeOutline} aria-hidden="true" />
                    </button>
                  ) : null}
                </div>
              </div>
              <div className="kvRow">
                <div className="label">Chat id</div>
                <Input
                  className="input"
                  value={notifications.telegram.chatId ?? ''}
                  onChange={(e) =>
                    updateNotifications('notifications.telegram.chatId', (current) => ({
                      ...current,
                      telegram: { ...current.telegram, chatId: e.target.value },
                    }))
                  }
                />
              </div>
            </NotificationChannelCard>

            <NotificationChannelCard
              channel="webPush"
              title="Web Push（Chrome / VAPID）"
              enabled={notifications.webPush.enabled}
              busy={busy}
              testState={notificationTestStates.webPush}
              testRunning={Boolean(notificationTestRunning.webPush)}
              onRunTest={runNotificationChannelTest}
              onToggleEnabled={(v) =>
                updateNotifications(
                  'notifications.webPush.enabled',
                  (current) => ({ ...current, webPush: { ...current.webPush, enabled: v } }),
                  true,
                )
              }
            >
              <div className="kvRow">
                <div className="label">Public Key</div>
                <Input
                  className="input"
                  value={notifications.webPush.vapidPublicKey ?? ''}
                  onChange={(e) =>
                    updateNotifications('notifications.webPush.vapidPublicKey', (current) => ({
                      ...current,
                      webPush: { ...current.webPush, vapidPublicKey: e.target.value },
                    }))
                  }
                />
              </div>
              <div className="kvRow">
                <div className="label">Private Key（留空=保持原值）</div>
                <Input
                  className="input"
                  value={notifications.webPush.vapidPrivateKey ?? ''}
                  onChange={(e) =>
                    updateNotifications('notifications.webPush.vapidPrivateKey', (current) => ({
                      ...current,
                      webPush: { ...current.webPush, vapidPrivateKey: e.target.value },
                    }))
                  }
                />
              </div>
              <div className="kvRow">
                <div className="label">Subject</div>
                <Input
                  className="input"
                  value={notifications.webPush.vapidSubject ?? ''}
                  onChange={(e) =>
                    updateNotifications('notifications.webPush.vapidSubject', (current) => ({
                      ...current,
                      webPush: { ...current.webPush, vapidSubject: e.target.value },
                    }))
                  }
                />
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
                  取消订阅
                </Button>
              </div>

              {webPushEndpoint ? (
                <div className="muted" style={{ marginTop: 10 }}>
                  endpoint <Mono>{webPushEndpoint.slice(0, 40)}…</Mono>
                </div>
              ) : null}
            </NotificationChannelCard>

            {error ? <div className="error">{error}</div> : null}
          </div>

          </AsyncDataRegion>
          ) : (
            <AsyncDataRegion
              className="settingsNotificationsRegion"
              error={notificationsLoadError}
              hasData={false}
              label="正在加载通知设置"
              onRetry={() => void refresh({ source: 'memory', trigger: 'user-action', domains: ['notifications'] })}
              phase={notificationsLoadPhase}
              skeleton={<AsyncDataSkeleton className="settingsLoadingSkeleton" lines={5} />}
              source={loadSource}
              trigger={loadTrigger}
            />
          )}

        </div>

        <div className="settingsCol">
          {githubPackages ? (
          <AsyncDataRegion
            className="settingsGhcrRegion"
            error={githubPackagesLoadError}
            hasData
            label="正在刷新 GHCR 设置"
            onRetry={() => void refresh({ source: 'memory', trigger: 'user-action', domains: ['githubPackages'] })}
            phase={githubPackagesLoadPhase}
            source={loadSource}
            trigger={loadTrigger}
          >
          <div className="card settingsSectionCard" data-settings-section="integrations" data-mobile-active={props.section === 'integrations' || undefined} id={SETTINGS_GHCR_WEBHOOK_ID}>
          <div className="title">GitHub Packages（GHCR）Webhook</div>
          <div className="muted">在 GHCR 发布新版本时自动触发 Dockrev 扫描（事件：package.published）</div>
          <div className="muted">添加后会自动创建后台任务注册 webhook；可在 GHCR 维护页查看状态并执行删除/重试。</div>
          {ghcrLiveProgressText ? (
            <div className="muted" style={{ marginTop: 8, display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
              <span>当前 GHCR 任务：{ghcrLiveProgressText}</span>
              <Button variant="ghost" disabled={busy} onClick={openGhcrRegistry}>
                打开 GHCR 维护页
              </Button>
            </div>
          ) : null}

          <div className="settingsSection">
            <div className="settingHead">
              <div className="sectionTitle">启用</div>
              <Switch
                checked={githubPackages.enabled}
                disabled={busy}
                onChange={(v) =>
                  updateGhcr('ghcr.enabled', (current) => ({ ...current, enabled: v }), true)
                }
              />
            </div>

            <div className="kv">
              <div className="kvRow">
                <div className="label">GitHub PAT（留空=保持原值）</div>
                <Input
                  className={ghcrPatIssue ? 'input inputError' : 'input'}
                  value={githubPackagesPat}
                  onChange={(e) =>
                    updateGhcr('ghcr.pat', (current) => ({ ...current, pat: e.target.value }))
                  }
                  placeholder="ghp_..."
                />
              </div>

              <div className="kvRow">
                <div className="label">Callback URL</div>
                <Input
                  className="input"
                  value={githubPackages.callbackUrl}
                  onChange={(e) =>
                    updateGhcr('ghcr.callbackUrl', (current) => ({ ...current, callbackUrl: e.target.value }))
                  }
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
                  <Input
                    className="input"
                    value={githubPackagesNewRepo}
                    onChange={(e) => setGitHubPackagesNewRepo(e.target.value)}
                    placeholder="https://github.com/org/repo 或 org/repo；也可粘贴 profile/org URL 批量选择"
                    style={{ flex: 1 }}
                  />
                  <Button
                    variant="ghost"
                    disabled={busy || ghcrResolvePending || !githubPackagesNewRepo.trim()}
                    onClick={() => {
                      if (busy || ghcrResolvePending) return
                      setGhcrResolvePending(true)
                      void (async () => {
                        setBusy(true)
                        setError(null)
                        try {
                          const input = githubPackagesNewRepo.trim()
                          if (!input) throw new Error('请先输入 owner/repo 或 profile 链接')
                          await flushAutoSave(['ghcr'])
                          const resolved = await resolveGitHubPackagesTarget(input)
                          if (resolved.kind === 'repo') {
                            const fullName = resolved.repos[0]?.fullName?.trim() ?? ''
                            if (!fullName) throw new Error('resolve returned empty repo')
                            await setGitHubPackagesRepoSelected({ fullName, selected: true })
                            setGitHubPackagesNewRepo('')
                            await refresh({ source: 'memory', trigger: 'user-action' })
                            await refreshTrackedRepos({ source: 'memory', trigger: 'user-action' })
                            return
                          }
                          if (resolved.kind === 'owner') {
                            let picked: Array<{ fullName: string; selected: boolean }> = resolved.repos.map((r) => ({
                              fullName: r.fullName,
                              selected: r.selected,
                            }))
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
                              cardClassName: 'ghcrPickerDialogCard',
                              bodyClassName: 'ghcrPickerDialogBody',
                              confirmText: '确认',
                              cancelText: '取消',
                              confirmVariant: 'primary',
                              badgeText: null,
                            })
                            if (!ok) return
                            // Apply both selections and deselections, but only for repos whose
                            // selection state changed in the picker to reduce API calls.
                            const before = new Map(resolved.repos.map((r) => [r.fullName, r.selected] as const))
                            const changed = picked.filter((r) => before.get(r.fullName) !== r.selected)
                            for (const r of changed) {
                              await setGitHubPackagesRepoSelected({ fullName: r.fullName, selected: r.selected })
                            }
                            setGitHubPackagesNewRepo('')
                            await refresh({ source: 'memory', trigger: 'user-action' })
                            await refreshTrackedRepos({ source: 'memory', trigger: 'user-action' })
                            return
                          }
                          throw new Error(`unsupported resolve kind: ${resolved.kind}`)
                        } catch (e: unknown) {
                          setError(mapResolveFailure(e))
                        } finally {
                          setBusy(false)
                          setGhcrResolvePending(false)
                        }
                      })()
                    }}
                  >
                    {ghcrResolvePending ? (
                      <span className="btnInlineLoading">
                        <span className="btnInlineSpinner" aria-hidden="true" />
                        <span>解析中…</span>
                      </span>
                    ) : (
                      '解析并添加'
                    )}
                  </Button>
                </div>
              </div>
            </div>

            <AsyncDataRegion
              className="settingsGhcrPreviewRegion"
              error={trackedReposLoadError}
              hasData={githubPackagesTrackedRepos !== null}
              label="正在刷新已跟踪仓库"
              onRetry={() => void refreshTrackedRepos({ source: 'memory', trigger: 'user-action' })}
              phase={trackedReposLoadPhase}
              skeleton={<AsyncDataSkeleton className="settingsGhcrPreviewSkeleton" lines={2} />}
              source={trackedReposLoadSource}
              trigger={trackedReposLoadTrigger}
            >
            {githubPackagesTrackedRepos ? (
              <div style={{ marginTop: 10 }}>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10, alignItems: 'center' }}>
                  <div className="muted">
                    预览 {githubPackagesTrackedRepos.repos.length} / {githubPackagesTrackedRepos.filteredTotal}
                  </div>
                  <div style={{ display: 'flex', gap: 10, marginLeft: 'auto' }}>
                    <Button variant="ghost" disabled={busy} onClick={openGhcrRegistry}>
                      查看更多
                      {githubPackagesTrackedRepos.filteredTotal > githubPackagesTrackedRepos.repos.length
                        ? `（+${githubPackagesTrackedRepos.filteredTotal - githubPackagesTrackedRepos.repos.length}）`
                        : ''}
                    </Button>
                  </div>
                  <Button variant="ghost" onClick={() => navigate({ name: 'ghcr-webhook-inbox' })}>
                    收件箱
                  </Button>
                </div>

                {githubPackagesTrackedRepos.repos.length ? (
                  <div
                    style={{
                      marginTop: 10,
                      maxHeight: 420,
                      overflowY: 'auto',
                      paddingRight: 6,
                      overscrollBehavior: 'contain',
                      display: 'flex',
                      flexDirection: 'column',
                      gap: 10,
                    }}
                    >
                    {githubPackagesTrackedRepos.repos.map((r) => {
                      const state = normalizeWebhookState(r.webhookState)
                      const dotClass = webhookStateDotClass(state)
                      const lastSync = r.lastSyncAt ? r.lastSyncAt : '-'
                      const lastAudit = r.lastAuditAt ? r.lastAuditAt : '-'
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
                              <Icon icon={webhookStateIcon(state)} className={dotClass} aria-hidden="true" />
                              <div className="mono" style={{ overflowWrap: 'anywhere' }}>
                                {r.fullName}
                              </div>
                            </div>
                            <div className="muted" style={{ marginTop: 4, overflowWrap: 'anywhere' }}>
                              状态: {webhookStateLabel(state)} · hookId: {hookId} · lastSyncAt: {lastSync} · lastAuditAt:{' '}
                              {lastAudit}
                              {r.lastError ? ` · lastError: ${r.lastError}` : null}
                            </div>
                            {state === 'conflict' ? (
                              <div className="muted" style={{ marginTop: 4 }}>
                                检测到重复 webhook，请先到 GitHub 手工删除重复项，再到 GHCR 维护页点“重新注册”。
                              </div>
                            ) : null}
                          </div>
                        </div>
                      )
                    })}
                  </div>
                ) : (
                  <div className="muted" style={{ marginTop: 10 }}>
                    暂无已跟踪仓库
                  </div>
                )}

                {githubPackagesTrackedRepos.filteredTotal > githubPackagesTrackedRepos.repos.length ? (
                  <div className="muted" style={{ marginTop: 10 }}>
                    设置页仅展示前 {GHCR_PREVIEW_LIMIT} 条，更多仓库请点击“查看更多”进入维护页。
                  </div>
                ) : null}
              </div>
            ) : (
              <div className="muted" style={{ marginTop: 10 }}>
                暂无已跟踪仓库
              </div>
            )}
            </AsyncDataRegion>
          </div>
          </div>

          </AsyncDataRegion>
          ) : (
            <AsyncDataRegion
              className="settingsGhcrRegion"
              error={githubPackagesLoadError}
              hasData={false}
              label="正在加载 GHCR 设置"
              onRetry={() => void refresh({ source: 'memory', trigger: 'user-action', domains: ['githubPackages'] })}
              phase={githubPackagesLoadPhase}
              skeleton={<AsyncDataSkeleton className="settingsLoadingSkeleton" lines={5} />}
              source={loadSource}
              trigger={loadTrigger}
            />
          )}
        </div>
      </div>
      {showAutoSaveToast ? (
        <div className={autoSaveToastClassName} role="status" aria-live="polite">
          <div>{autoSaveStatusText}</div>
          {autoSavePhase === 'error' && autoSaveIssue ? (
            <div className="autoSaveToastDetail">{autoSaveIssue.message}</div>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}
