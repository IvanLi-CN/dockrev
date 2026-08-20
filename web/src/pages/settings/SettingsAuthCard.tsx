import type { SettingsResponse } from '../../api'
import type { SettingsSection } from '../../routes'

export function SettingsAuthCard(props: { auth: SettingsResponse['auth']; section?: SettingsSection }) {
  const { auth, section } = props
  return (
    <div className="card settingsSectionCard" data-settings-section="account" data-mobile-active={section === 'account' || undefined}>
      <div className="title">鉴权（Forward Auth）</div>
      <div className="muted">认证由入口代理负责；Dockrev 按用户/组执行项目侧鉴权（运行时只读）</div>
      <div className="kv">
        <div className="kvRow"><div className="label">用户头</div><div className="mono">{auth.forwardHeaderName}</div></div>
        <div className="kvRow"><div className="label">组头</div><div className="mono">{auth.groupHeaderName}</div></div>
        <div className="kvRow"><div className="label">鉴权模式</div><div className="mono">{auth.authorizationMode}</div></div>
        <div className="kvRow"><div className="label">允许用户</div><div className="mono">{auth.allowedUserMasked || '-'}</div></div>
        <div className="kvRow"><div className="label">允许组</div><div className="mono">{auth.allowedGroupMasked || '-'}</div></div>
        <div className="kvRow"><div className="label">当前用户</div><div className="mono">{auth.currentUser || '-'}</div></div>
        <div className="kvRow"><div className="label">当前组</div><div className="mono">{auth.currentGroups.length ? auth.currentGroups.join(', ') : '-'}</div></div>
        <div className="kvRow"><div className="label">命中方式</div><div className="mono">{auth.matchedBy || '-'}</div></div>
        <div className="kvRow"><div className="label">允许匿名（开发环境）</div><div className="muted">{auth.allowAnonymousInDev ? 'on' : 'off'}</div></div>
        <div className="muted" style={{ marginTop: 6 }}>
          该区域由启动配置控制：`DOCKREV_AUTH_FORWARD_HEADER_NAME` / `DOCKREV_AUTH_GROUP_HEADER_NAME` /
          `DOCKREV_AUTH_ALLOWED_USER` / `DOCKREV_AUTH_ALLOWED_GROUP` /
          `DOCKREV_AUTH_ALLOW_ANONYMOUS_IN_DEV`，修改后需重启服务生效。
        </div>
      </div>
    </div>
  )
}
