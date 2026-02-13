import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { getDockrevVersion } from './api'
import { Chip, GitHubIcon, Mono } from './ui'
import { ConfirmProvider } from './ConfirmProvider'
import type { Route } from './routes'
import { currentHref, navigate } from './routes'

function formatShort(ts: string) {
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  const pad2 = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(d.getHours())}:${pad2(d.getMinutes())}`
}

function formatVersionLabel(version: string | null): string {
  const v = (version ?? '').trim()
  if (!v) return '-'
  // Show server-reported version verbatim to avoid misleading operators.
  return v
}

function formatVersionDisplay(version: string | null): string {
  const v = formatVersionLabel(version)
  if (v === '-') return v
  // Make it obvious this is a version without altering the underlying ref used for links.
  if (/^v/i.test(v)) return v
  if (/^\d+\.\d+\.\d+([+-].+)?$/.test(v)) return `v${v}`
  return v
}

function encodeGitRefForPath(ref: string): string {
  // Keep slashes so branch names like "feat/x" can still be used as a ref segment.
  return encodeURIComponent(ref).replaceAll('%2F', '/')
}

export function AppShell(props: {
  route: Route
  title?: string
  pageSubtitle?: string
  topbarHint?: string
  topActions?: ReactNode
  composeHint?: { path?: string; profile?: string; lastScan?: string }
  children: ReactNode
}) {
  const active = props.route.name === 'service' ? 'services' : props.route.name === 'job' ? 'queue' : props.route.name
  const [appVersion, setAppVersion] = useState<string | null>(null)

  const composePath = props.composeHint?.path
  const profile = props.composeHint?.profile
  const lastScan = props.composeHint?.lastScan

  const nav = useMemo(
    () => [
      { key: 'overview', label: '概览', to: { name: 'overview' } as const },
      { key: 'queue', label: '更新队列', to: { name: 'queue' } as const },
      { key: 'services', label: '服务', to: { name: 'services' } as const },
      { key: 'settings', label: '系统设置', to: { name: 'settings' } as const },
    ],
    [],
  )

  useEffect(() => {
    let cancelled = false
    void getDockrevVersion()
      .then((v) => {
        if (cancelled) return
        setAppVersion(v)
      })
      .catch(() => {
        if (cancelled) return
        setAppVersion(null)
      })
    return () => {
      cancelled = true
    }
  }, [])

  const versionLabel = formatVersionLabel(appVersion)
  const versionRef = (appVersion ?? '').trim()
  const versionDisplay = formatVersionDisplay(appVersion)
  const versionHref =
    versionLabel !== '-' && versionRef
      ? `https://github.com/IvanLi-CN/dockrev/tree/${encodeGitRefForPath(versionRef)}`
      : null

  return (
    <ConfirmProvider>
      <div className="appShell">
        <header className="topbar">
          <div className="topbarLeft">
            <div className="brand">Dockrev</div>
          </div>
          <div className="topbarRight">
            {props.topActions ? <div className="topActions">{props.topActions}</div> : null}
            <div className="chipStatic chipStaticUser">用户：ivan（FH）</div>
          </div>
        </header>

        <aside className="sidebar">
          <div className="sidebarSectionLabel">导航</div>
          <nav className="nav">
            {nav.map((item) => (
              <a
                key={item.key}
                href={currentHref(item.to)}
                className={active === item.key ? 'navItem navItemActive' : 'navItem'}
                onClick={(e) => {
                  e.preventDefault()
                  navigate(item.to)
                }}
              >
                {item.label}
              </a>
            ))}
          </nav>

          <div className="sidebarSectionLabel" style={{ marginTop: 24 }}>
            Compose
          </div>
          {composePath ? (
            <div className="sidebarMono">
              <Mono>{composePath}</Mono>
            </div>
          ) : (
            <div className="sidebarMuted">尚未选择 stack</div>
          )}
          {profile ? (
            <div className="chipStatic chipStaticSidebar" style={{ marginTop: 8 }}>{`profile: ${profile}`}</div>
          ) : null}

          <div className="sidebarSectionLabel" style={{ marginTop: 20 }}>
            最近一次扫描
          </div>
          {lastScan ? (
            <div className="sidebarMono">
              <Mono>{formatShort(lastScan)}</Mono>
            </div>
          ) : (
            <div className="sidebarMuted">-</div>
          )}

          <div className="sidebarMeta">
            <div className="sidebarMetaDivider" aria-hidden="true" />
            <div className="sidebarMetaTop">
              {versionHref ? (
                <a
                  className="sidebarMetaVersion"
                  href={versionHref}
                  target="_blank"
                  rel="noopener noreferrer"
                  aria-label={`Version on GitHub: ${versionDisplay}`}
                  title={`Version: ${versionDisplay}`}
                >
                  <Mono>{versionDisplay}</Mono>
                </a>
              ) : (
                <Mono>{versionDisplay}</Mono>
              )}
              <a
                className="sidebarMetaIcon"
                href="https://github.com/IvanLi-CN/dockrev"
                target="_blank"
                rel="noopener noreferrer"
                aria-label="GitHub repository"
                title="GitHub: IvanLi-CN/dockrev"
              >
                <GitHubIcon className="sidebarMetaGitHub" />
              </a>
            </div>
            <a
              className="sidebarMetaPowered"
              href="https://github.com/IvanLi-CN"
              target="_blank"
              rel="noopener noreferrer"
            >
              Powered by <span className="mono">Ivan Li</span>
            </a>
          </div>
        </aside>

        <main className="content">
          <div className="pageHead">
            {props.title ? <div className="h1">{props.title}</div> : null}
            {props.pageSubtitle ? <div className="muted">{props.pageSubtitle}</div> : null}
          </div>
          {props.children}
        </main>
      </div>
    </ConfirmProvider>
  )
}

export function FilterChips<T extends string>(props: {
  value: T
  onChange: (v: T) => void
  items: Array<{ key: T; label: string; count?: number; activeTone?: 'primary' | 'ghost' }>
}) {
  return (
    <div className="chipRow">
      {props.items.map((it) => (
        <Chip
          key={it.key}
          active={props.value === it.key}
          onClick={() => props.onChange(it.key)}
          title={it.count != null ? `${it.label}: ${it.count}` : it.label}
        >
          <span>{it.label}</span>
          {it.count != null ? <span className="chipCount">{it.count}</span> : null}
        </Chip>
      ))}
    </div>
  )
}
