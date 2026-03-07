import type { AuthRequiredDetails } from '../api'
import { navigate } from '../routes'
import { Button } from '../ui'

function reasonLabel(reason?: string): string {
  switch (reason) {
    case 'authz_config_missing':
      return 'Dockrev 尚未配置允许的用户或组。'
    case 'identity_missing':
      return '入口代理没有把 Forward Auth 身份头传给 Dockrev。'
    case 'authz_no_match':
      return '当前用户或组没有命中 Dockrev 的允许名单。'
    default:
      return '当前请求未通过 Dockrev 鉴权。'
  }
}

function joinOrDash(values?: string[] | null): string {
  if (!values || values.length === 0) return '-'
  return values.join(', ')
}

export function UnauthorizedPage(props: { authDetails?: AuthRequiredDetails | null }) {
  const auth = props.authDetails ?? null
  const canOpenDeployCheck = auth?.redirectTo === 'deploy-check'

  return (
    <div className="deployWelcomeRoot">
      <main className="deployWelcomeMain">
        <section className="deployWelcomePanel deployWelcomeSummaryPanel is-fail">
          <div className="deployWelcomeSummaryHead">
            <div>
              <p className="deployWelcomeEyebrow">Unauthorized</p>
              <h1 className="deployWelcomeTitle">当前身份未获 Dockrev 授权</h1>
              <p className="deployWelcomeSubtitle">
                认证由入口代理完成，Dockrev 只根据 Forward Auth 提供的用户/组做项目侧鉴权。
              </p>
            </div>
            <div className="deployWelcomeOverall is-fail">
              <span className="deployWelcomeOverallLabel">结论</span>
              <strong>401</strong>
              <span className="deployWelcomeOverallSummary">{reasonLabel(auth?.reason)}</span>
            </div>
          </div>

          <div className="kv" style={{ marginTop: 16 }}>
            <div className="kvRow">
              <div className="label">用户头</div>
              <div className="mono">{auth?.forwardHeaderName ?? 'X-Forwarded-User'}</div>
            </div>
            <div className="kvRow">
              <div className="label">组头</div>
              <div className="mono">{auth?.groupHeaderName ?? 'Remote-Groups'}</div>
            </div>
            <div className="kvRow">
              <div className="label">允许用户</div>
              <div className="mono">{auth?.allowedUserMasked ?? '-'}</div>
            </div>
            <div className="kvRow">
              <div className="label">允许组</div>
              <div className="mono">{auth?.allowedGroupMasked ?? '-'}</div>
            </div>
            <div className="kvRow">
              <div className="label">当前用户</div>
              <div className="mono">{auth?.currentUser ?? '-'}</div>
            </div>
            <div className="kvRow">
              <div className="label">当前组</div>
              <div className="mono">{joinOrDash(auth?.currentGroups)}</div>
            </div>
          </div>

          <div className="deployWelcomeActions" style={{ marginTop: 18 }}>
            {canOpenDeployCheck ? (
              <Button variant="primary" onClick={() => navigate({ name: 'deploy-check' })}>
                打开自检页
              </Button>
            ) : null}
            <Button variant="ghost" onClick={() => window.location.reload()}>
              重新加载
            </Button>
          </div>
        </section>
      </main>
    </div>
  )
}
