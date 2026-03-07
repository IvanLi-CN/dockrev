import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { getDockrevVersion } from './api'
import { GitHubIcon, Mono, ToggleGroup, ToggleGroupItem } from './ui'
import { ConfirmProvider } from './ConfirmProvider'
import { brandMarkUrl } from './publicAssetUrls'
import { UpdateActionTrackerProvider } from './updateActionTracking'
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
  lastScanHint?: string
  children: ReactNode
}) {
  const active =
    props.route.name === 'service'
      ? 'services'
      : props.route.name === 'job' ||
          props.route.name === 'version-inference' ||
          props.route.name === 'ghcr-webhooks' ||
          props.route.name === 'ghcr-webhook-inbox'
        ? 'queue'
        : props.route.name === 'ghcr-webhook-registry'
          ? 'settings'
        : props.route.name
  const [appVersion, setAppVersion] = useState<string | null>(null)

  const lastScan = props.lastScanHint

  const nav = useMemo(
    () => [
      { key: 'overview', label: '概览', to: { name: 'overview' } as const },
      { key: 'queue', label: '任务队列', to: { name: 'queue' } as const },
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
      ? `https://github.com/IvanLi-CN/dockrev/releases/tag/${encodeGitRefForPath(versionRef)}`
      : null

  return (
    <UpdateActionTrackerProvider>
      <ConfirmProvider>
        <div className="appShell">
        <header className="topbar">
          <div className="topbarLeft">
            <div className="topbarIdentity">
              <div className="brand">
                <img className="brandMark" src={brandMarkUrl} alt="" aria-hidden="true" />
                Dockrev
              </div>
              {props.topbarHint ? <div className="topbarHint">{props.topbarHint}</div> : null}
            </div>
          </div>
          <div className="topbarRight">
            {props.topActions ? <div className="topActions">{props.topActions}</div> : null}
            <div className="chipStatic chipStaticUser">鉴权：Forward Auth</div>
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
                  aria-label={`Release on GitHub: ${versionDisplay}`}
                  title={`Release: ${versionDisplay}`}
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
          <div className="mobileDockrevPanel">
            <nav className="mobileNav" aria-label="主导航">
              {nav.map((item) => (
                <a
                  key={`mobile-${item.key}`}
                  href={currentHref(item.to)}
                  className={active === item.key ? 'mobileNavItem mobileNavItemActive' : 'mobileNavItem'}
                  onClick={(e) => {
                    e.preventDefault()
                    navigate(item.to)
                  }}
                >
                  {item.label}
                </a>
              ))}
            </nav>
            <div className="mobileMeta">
              <div className="mobileMetaRow">
                <span className="sectionTitle">最近扫描</span>
                <span className="mono">{lastScan ? formatShort(lastScan) : '-'}</span>
              </div>
            </div>
          </div>
          <div className="pageHead">
            {props.title ? <div className="h1">{props.title}</div> : null}
            {props.pageSubtitle ? <div className="muted">{props.pageSubtitle}</div> : null}
          </div>
          {props.children}
        </main>
        </div>
      </ConfirmProvider>
    </UpdateActionTrackerProvider>
  )
}

export function FilterChips<T extends string>(props: {
  value: T
  onChange: (v: T) => void
  items: Array<{ key: T; label: string; count?: number; activeTone?: 'primary' | 'ghost' }>
}) {
  return (
    <ToggleGroup
      aria-label="过滤条件"
      className="chipRow"
      onValueChange={(value) => {
        if (value) props.onChange(value as T)
      }}
      type="single"
      value={props.value}
    >
      {props.items.map((it) => (
        <ToggleGroupItem
          key={it.key}
          className={props.value === it.key ? 'chip chipActive' : 'chip'}
          title={it.count != null ? `${it.label}: ${it.count}` : it.label}
          value={it.key}
          variant="outline"
        >
          <span>{it.label}</span>
          {it.count != null ? <span className="chipCount">{it.count}</span> : null}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}
