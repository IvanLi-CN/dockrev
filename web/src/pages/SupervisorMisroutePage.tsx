import { useEffect, useMemo, type ReactNode } from 'react'
import { Button, Mono } from '../ui'
import { href, navigate } from '../routes'
import { selfUpgradeBaseUrl } from '../runtimeConfig'
import { useSupervisorHealth } from '../useSupervisorHealth'

export function SupervisorMisroutePage(props: { basePath: string; pathname: string; onTopActions: (node: ReactNode) => void }) {
  const { basePath, onTopActions } = props
  const supervisor = useSupervisorHealth()
  const { state, check } = supervisor
  const selfUpgradeUrl = useMemo(() => selfUpgradeBaseUrl(), [])

  useEffect(() => {
    onTopActions(
      <>
        <Button
          variant="ghost"
          onClick={() => {
            navigate({ name: 'overview' })
          }}
        >
          返回 Dockrev
        </Button>
        <Button
          variant="ghost"
          disabled={state.status === 'checking'}
          onClick={() => {
            void check()
          }}
        >
          重试
        </Button>
      </>,
    )
  }, [check, onTopActions, state.status])

  const baseWithSlash = basePath.endsWith('/') ? basePath : `${basePath}/`

  return (
    <div className="page">
      <div className="card">
        <div className="title">
          部署问题：<Mono>{baseWithSlash}</Mono> 未映射到 Dockrev Supervisor
        </div>
        <div className="muted">
          你正在访问的是自我升级入口（Supervisor）。但当前响应来自 <b>Dockrev 主服务</b>，这通常意味着反向代理/路由配置漏配或误配。
        </div>

        <div className="kv" style={{ marginTop: 12 }}>
          <div className="kvRow">
            <div className="label">期望入口</div>
            <div className="muted">
              <Mono>{selfUpgradeUrl}</Mono>
            </div>
          </div>
          <div className="kvRow">
            <div className="label">Supervisor 状态</div>
            <div className="muted">
              {state.status === 'ok'
                ? `ok (${state.okAt})`
                : state.status === 'checking'
                  ? 'checking…'
                  : state.status === 'offline'
                    ? `offline (${state.errorAt})`
                    : 'unknown'}
            </div>
          </div>
          {state.status === 'offline' ? (
            <div className="kvRow">
              <div className="label">原因</div>
              <div className="muted">
                <Mono>{state.error}</Mono>
              </div>
            </div>
          ) : null}
        </div>
      </div>

      <div className="card">
        <div className="title">如何验证</div>
        <div className="muted">请在同域下验证以下接口应由 supervisor 返回（而不是 Dockrev 主服务）：</div>
        <div className="mono" style={{ marginTop: 10, whiteSpace: 'pre-wrap' }}>
          {`curl -i ${baseWithSlash}health\ncurl -i ${baseWithSlash}version\ncurl -i ${baseWithSlash}self-upgrade`}
        </div>
      </div>

      <div className="card">
        <div className="title">如何修复（思路）</div>
        <div className="muted">
          在你的反向代理中，把 <Mono>{baseWithSlash}</Mono> 路由到 supervisor 的 HTTP 地址（并保持 base path 一致）。
        </div>
        <div className="muted" style={{ marginTop: 8 }}>
          常见相关配置：<Mono>DOCKREV_SELF_UPGRADE_URL</Mono>（Dockrev 主服务/前端使用）与 <Mono>DOCKREV_SUPERVISOR_BASE_PATH</Mono>（supervisor 使用）。
        </div>
        <div className="muted" style={{ marginTop: 10 }}>
          Dockrev 主站：<a href={href({ name: 'overview' })}>/</a>
        </div>
      </div>
    </div>
  )
}
