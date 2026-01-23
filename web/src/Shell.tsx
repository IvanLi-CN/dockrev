import { useMemo, type ReactNode } from 'react'
import { Chip, Mono } from './ui'
import type { Route } from './routes'
import { currentHref, navigate } from './routes'

function formatShort(ts: string) {
  const d = new Date(ts)
  if (Number.isNaN(d.valueOf())) return ts
  const pad2 = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(d.getHours())}:${pad2(d.getMinutes())}`
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
  const active = props.route.name === 'service' ? 'services' : props.route.name

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

  return (
    <div className="appShell">
      <header className="topbar">
        <div className="topbarLeft">
          <div className="brand">Dockrev</div>
          <div className="topbarHint">{props.topbarHint ?? 'Compose 镜像更新 / 版本提示'}</div>
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
      </aside>

      <main className="content">
        <div className="pageHead">
          {props.title ? <div className="h1">{props.title}</div> : null}
          {props.pageSubtitle ? <div className="muted">{props.pageSubtitle}</div> : null}
        </div>
        {props.children}
      </main>
    </div>
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
          {it.label}
        </Chip>
      ))}
    </div>
  )
}
