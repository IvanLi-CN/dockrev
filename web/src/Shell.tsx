import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  ChevronLeft,
  ChevronRight,
  Gauge,
  History,
  LayoutDashboard,
  ListChecks,
  Settings,
  Trash2,
  X,
  type LucideIcon,
} from 'lucide-react'
import { getDockrevVersion } from './api'
import { AppShellStatusBanner } from './components/AppShellStatusBanner'
import { usePwaStatus } from './pwaStatus'
import { Button, GitHubIcon, Mono, OverlayScrollArea, ToggleGroup, ToggleGroupItem } from './ui'
import { ConfirmProvider } from './ConfirmProvider'
import { BrandLogo } from './BrandLogo'
import { UpdateActionTrackerProvider } from './updateActionTracking'
import type { Route } from './routes'
import { currentHref, navigate } from './routes'
import { TopbarUserIdentity } from './components/TopbarUserIdentity'
import type { TopbarAuthIdentity } from './topbarAuthIdentity'

const MOBILE_MENU_MEDIA_QUERY = "(max-width: 960px)";
export const APP_SHELL_SIDEBAR_COLLAPSED_STORAGE_KEY =
  "dockrev:shell:sidebarCollapsed:v1";

type PrimaryNavItem = {
  key: "overview" | "queue" | "services" | "cleanup" | "settings";
  label: string;
  mobileLabel: string;
  icon: LucideIcon;
  to: Route;
};

function readMobileMenuMediaMatches(): boolean {
  return (
    typeof window !== "undefined" &&
    window.matchMedia(MOBILE_MENU_MEDIA_QUERY).matches
  );
}

function readSidebarCollapsed(): boolean {
  if (typeof window === "undefined") return false;
  try {
    return (
      window.localStorage.getItem(APP_SHELL_SIDEBAR_COLLAPSED_STORAGE_KEY) ===
      "1"
    );
  } catch {
    return false;
  }
}

function formatShort(ts: string) {
  const d = new Date(ts);
  if (Number.isNaN(d.valueOf())) return ts;
  const pad2 = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}

function formatVersionLabel(version: string | null): string {
  const v = (version ?? "").trim();
  if (!v) return "-";
  // Show server-reported version verbatim to avoid misleading operators.
  return v;
}

function formatVersionDisplay(version: string | null): string {
  const v = formatVersionLabel(version);
  if (v === "-") return v;
  // Make it obvious this is a version without altering the underlying ref used for links.
  if (/^v/i.test(v)) return v;
  if (/^\d+\.\d+\.\d+([+-].+)?$/.test(v)) return `v${v}`;
  return v;
}

function encodeGitRefForPath(ref: string): string {
  // Keep slashes so branch names like "feat/x" can still be used as a ref segment.
  return encodeURIComponent(ref).replaceAll("%2F", "/");
}

function ShellStatusStrip() {
  const {
    isOnline,
    offlineReady,
    updateAvailable,
    dismissOfflineReady,
    dismissUpdate,
    applyUpdate,
  } = usePwaStatus()

  if (updateAvailable) {
    return (
      <AppShellStatusBanner
        tone="update"
        title="发现新版本，可刷新更新。"
        detail="当前页面不会自动切换；确认后才载入新的前端资源。"
        actions={
          <>
          <Button onClick={() => void applyUpdate()} variant="primary">
            刷新更新
          </Button>
          <Button onClick={dismissUpdate} variant="ghost">
            稍后
          </Button>
          </>
        }
      />
    )
  }

  if (!isOnline) {
    return (
      <AppShellStatusBanner
        tone="offline"
        title="当前离线，优先使用本地只读快照。"
        detail="写操作、日志流和部分高时效页面需要恢复联网后才能继续。"
      />
    )
  }

  if (offlineReady) {
    return (
      <AppShellStatusBanner
        tone="ready"
        title="离线壳已就绪。"
        detail="之后断网刷新仍可先启动应用与已缓存的主要只读页。"
        actions={
          <>
          <Button onClick={dismissOfflineReady} variant="ghost">
            知道了
          </Button>
          </>
        }
      />
    )
  }

  return null
}

export function AppShell(props: {
  route: Route;
  title?: string;
  pageSubtitle?: string;
  topActions?: ReactNode;
  topbarContent?: ReactNode;
  sidebarNavContent?: ReactNode;
  detailSidebarContent?: ReactNode;
  detailSidebarTitle?: string;
  mobileNavContent?: ReactNode;
  mobileDrawerTitle?: string;
  authIdentity?: TopbarAuthIdentity | null;
  lastScanHint?: string;
  children: ReactNode;
}) {
  const active =
    props.route.name === "service" || props.route.name === "stack"
      ? "services"
      : props.route.name === "job" ||
          props.route.name === "version-inference" ||
          props.route.name === "ghcr-webhooks" ||
          props.route.name === "ghcr-webhook-inbox"
        ? "queue"
        : props.route.name === "ghcr-webhook-registry"
          ? "settings"
          : props.route.name;
  const [appVersion, setAppVersion] = useState<string | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] =
    useState(readSidebarCollapsed);
  const [mobileDrawerOpenFor, setMobileDrawerOpenFor] = useState<string | null>(
    null,
  );
  const [mobileMenuMediaMatches, setMobileMenuMediaMatches] = useState(
    readMobileMenuMediaMatches,
  );

  const lastScan = props.lastScanHint;

  const nav = useMemo(
    (): PrimaryNavItem[] => [
      {
        key: "overview",
        label: "导航概览",
        mobileLabel: "概览",
        icon: LayoutDashboard,
        to: { name: "overview" },
      },
      {
        key: "queue",
        label: "任务队列",
        mobileLabel: "队列",
        icon: ListChecks,
        to: { name: "queue" },
      },
      {
        key: "services",
        label: "运维大盘",
        mobileLabel: "服务",
        icon: Gauge,
        to: { name: "services" },
      },
      {
        key: "cleanup",
        label: "清理",
        mobileLabel: "清理",
        icon: Trash2,
        to: { name: "cleanup" },
      },
      {
        key: "settings",
        label: "系统设置",
        mobileLabel: "设置",
        icon: Settings,
        to: { name: "settings" },
      },
    ],
    [],
  );

  useEffect(() => {
    if (typeof window === "undefined") return;
    try {
      window.localStorage.setItem(
        APP_SHELL_SIDEBAR_COLLAPSED_STORAGE_KEY,
        sidebarCollapsed ? "1" : "0",
      );
    } catch {
      // Sidebar width is a local preference; failure to persist should not block navigation.
    }
  }, [sidebarCollapsed]);

  useEffect(() => {
    let cancelled = false;
    void getDockrevVersion()
      .then((v) => {
        if (cancelled) return;
        setAppVersion(v);
      })
      .catch(() => {
        if (cancelled) return;
        setAppVersion(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const versionLabel = formatVersionLabel(appVersion);
  const versionRef = (appVersion ?? "").trim();
  const versionDisplay = formatVersionDisplay(appVersion);
  const routeHref = currentHref(props.route);
  const mobileMenuVisible =
    mobileDrawerOpenFor === routeHref &&
    mobileMenuMediaMatches &&
    Boolean(props.mobileNavContent);
  const hasDetailSidebar = Boolean(props.detailSidebarContent);
  const hasMobileDrawerContent = Boolean(props.mobileNavContent);
  const mobileDrawerTitle =
    props.mobileDrawerTitle ??
    (hasDetailSidebar
      ? "服务导航"
      : hasMobileDrawerContent
        ? "页面工具"
        : "主导航");
  const shellClassName = [
    "appShell",
    props.topbarContent ? "appShellWithTopbarContent" : null,
    hasDetailSidebar ? "appShellWithDetailSidebar" : null,
    sidebarCollapsed ? "appShellSidebarCollapsed" : null,
  ]
    .filter(Boolean)
    .join(" ");
  const versionHref =
    versionLabel !== "-" && versionRef
      ? `https://github.com/IvanLi-CN/dockrev/releases/tag/${encodeGitRefForPath(versionRef)}`
      : null;

  useEffect(() => {
    if (typeof window === "undefined") return;
    const query = window.matchMedia(MOBILE_MENU_MEDIA_QUERY);
    const sync = () => {
      setMobileMenuMediaMatches(query.matches);
      if (!query.matches) setMobileDrawerOpenFor(null);
    };
    sync();
    query.addEventListener("change", sync);
    return () => query.removeEventListener("change", sync);
  }, []);

  useEffect(() => {
    if (!mobileMenuVisible) return;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMobileDrawerOpenFor(null);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [mobileMenuVisible]);

  return (
    <UpdateActionTrackerProvider>
      <ConfirmProvider>
        <div className={shellClassName}>
          <header className="topbar">
            <div className="topbarDesktopBrand" aria-hidden="true">
              <div className="brand">
                <BrandLogo />
              </div>
            </div>
            <div className="topbarMain">
              <div className="topbarLeft">
                {hasMobileDrawerContent ? (
                  <div
                    className={
                      mobileMenuVisible
                        ? "mobileDockrevPanel mobileDockrevPanelOpen"
                        : "mobileDockrevPanel"
                    }
                  >
                    <button
                      type="button"
                      className="mobileMenuButton"
                      aria-label={
                        mobileMenuVisible
                          ? `关闭${mobileDrawerTitle}`
                          : `打开${mobileDrawerTitle}`
                      }
                      aria-controls="mobileDockrevMenu"
                      aria-expanded={mobileMenuVisible}
                      onClick={() =>
                        setMobileDrawerOpenFor((value) =>
                          value === routeHref ? null : routeHref,
                        )
                      }
                    >
                      <span className="mobileMenuIcon" aria-hidden="true">
                        <span />
                        <span />
                        <span />
                      </span>
                    </button>
                  </div>
                ) : null}
                <div className="topbarIdentity">
                  <div className="brand">
                    <BrandLogo />
                  </div>
                </div>
              </div>
              {props.topbarContent ? (
                <div className="topbarGlobalContent">{props.topbarContent}</div>
              ) : null}
              <div className="topbarRight">
                {props.topActions ? (
                  <div className="topActions">{props.topActions}</div>
                ) : null}
                <TopbarUserIdentity authIdentity={props.authIdentity} />
              </div>
            </div>
          </header>

          {mobileMenuVisible ? (
            <button
              type="button"
              className="mobileMenuBackdrop"
              aria-label={`关闭${mobileDrawerTitle}`}
              onClick={() => setMobileDrawerOpenFor(null)}
            />
          ) : null}
          <div
            id="mobileDockrevMenu"
            className="mobileMenuDrawer"
            role="dialog"
            aria-modal="true"
            aria-label={mobileDrawerTitle}
            hidden={!mobileMenuVisible}
          >
            <div className="mobileMenuDrawerHead">
              <div className="mobileMenuDrawerBrand">
                <BrandLogo />
                <div className="mobileMenuDrawerTitle">{mobileDrawerTitle}</div>
              </div>
              <button
                type="button"
                className="mobileMenuDrawerClose"
                aria-label={`关闭${mobileDrawerTitle}`}
                onClick={() => setMobileDrawerOpenFor(null)}
              >
                <X size={18} strokeWidth={2.2} />
              </button>
            </div>
            {mobileMenuVisible && props.mobileNavContent ? (
              <div className="mobileMenuEmbeddedContent">
                {props.mobileNavContent}
              </div>
            ) : null}
            <div className="mobileMeta">
              <div className="mobileMetaRow">
                <span className="sectionTitle">最近扫描</span>
                <span className="mono">
                  {lastScan ? formatShort(lastScan) : "-"}
                </span>
              </div>
            </div>
          </div>

          <OverlayScrollArea
            className="sidebar"
            role="complementary"
            aria-label="主导航侧栏"
            viewportLabel="主导航侧栏"
          >
            <div className="sidebarNavHeader">
              <span className="sidebarSectionLabel sidebarNavLabel">导航</span>
              <button
                type="button"
                className="sidebarCollapseButton"
                aria-label={sidebarCollapsed ? "展开左侧导航" : "折叠左侧导航"}
                aria-controls="appShellPrimaryNav"
                aria-expanded={!sidebarCollapsed}
                title={sidebarCollapsed ? "展开左侧导航" : "折叠左侧导航"}
                onClick={() => setSidebarCollapsed((value) => !value)}
              >
                {sidebarCollapsed ? (
                  <ChevronRight
                    size={17}
                    strokeWidth={2.2}
                    aria-hidden="true"
                  />
                ) : (
                  <ChevronLeft size={17} strokeWidth={2.2} aria-hidden="true" />
                )}
              </button>
            </div>
            <nav id="appShellPrimaryNav" className="nav" aria-label="主导航">
              {nav.map((item) => {
                const NavIcon = item.icon;
                return (
                  <a
                    key={item.key}
                    href={currentHref(item.to)}
                    className={
                      active === item.key ? "navItem navItemActive" : "navItem"
                    }
                    aria-label={sidebarCollapsed ? item.label : undefined}
                    title={sidebarCollapsed ? item.label : undefined}
                    onClick={(e) => {
                      e.preventDefault();
                      navigate(item.to);
                    }}
                  >
                    <NavIcon
                      className="navItemIcon"
                      aria-hidden="true"
                      strokeWidth={2.1}
                    />
                    <span className="navItemLabel">{item.label}</span>
                  </a>
                );
              })}
            </nav>
            {!sidebarCollapsed && props.sidebarNavContent ? (
              <div className="sidebarEmbeddedContent">
                {props.sidebarNavContent}
              </div>
            ) : null}

            {!sidebarCollapsed ? (
              <div className="sidebarScanBlock">
                <div className="sidebarSectionLabel" style={{ marginTop: 24 }}>
                  最近一次扫描
                </div>
                {lastScan ? (
                  <div className="sidebarMono sidebarInfoLine">
                    <History className="sidebarInfoIcon" aria-hidden="true" />
                    <Mono>{formatShort(lastScan)}</Mono>
                  </div>
                ) : (
                  <div className="sidebarMuted sidebarInfoLine">
                    <History className="sidebarInfoIcon" aria-hidden="true" />
                    <span>-</span>
                  </div>
                )}
              </div>
            ) : null}

            <div className="sidebarMeta">
              <TopbarUserIdentity
                authIdentity={props.authIdentity}
                placement="sidebar"
              />
              <div className="sidebarMetaDivider" aria-hidden="true" />
              <div className="sidebarMetaTop">
                {!sidebarCollapsed ? (
                  versionHref ? (
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
                  )
                ) : null}
                {sidebarCollapsed && versionHref ? (
                  <a
                    className="sidebarMetaIcon"
                    href={versionHref}
                    target="_blank"
                    rel="noopener noreferrer"
                    aria-label={`Release on GitHub: ${versionDisplay}`}
                    title={`Release: ${versionDisplay}`}
                  >
                    <span className="mono" aria-hidden="true">
                      v
                    </span>
                  </a>
                ) : null}
                {sidebarCollapsed && !versionHref ? (
                  <span
                    className="sidebarMetaIcon sidebarMetaIconDisabled"
                    aria-hidden="true"
                  >
                    <Mono>v</Mono>
                  </span>
                ) : null}
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
              {!sidebarCollapsed ? (
                <a
                  className="sidebarMetaPowered"
                  href="https://github.com/IvanLi-CN"
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  Powered by <span className="mono">Ivan Li</span>
                </a>
              ) : null}
            </div>
          </OverlayScrollArea>

          {props.detailSidebarContent ? (
            <OverlayScrollArea
              className="detailSidebar"
              role="complementary"
              aria-label={props.detailSidebarTitle ?? "服务导航"}
              viewportLabel={props.detailSidebarTitle ?? "服务导航"}
            >
              {props.detailSidebarTitle ? (
                <div className="detailSidebarHeader">
                  <span className="sidebarSectionLabel detailSidebarLabel">
                    {props.detailSidebarTitle}
                  </span>
                </div>
              ) : null}
              <div className="detailSidebarBody">
                {props.detailSidebarContent}
              </div>
            </OverlayScrollArea>
          ) : null}

          <OverlayScrollArea
            className={
              props.route.name === "job"
                ? props.detailSidebarContent
                  ? "content contentWithDetailSidebar contentJobDetail"
                  : "content contentJobDetail"
                : props.detailSidebarContent
                  ? "content contentWithDetailSidebar"
                  : "content"
            }
            role="main"
            viewportLabel="主内容"
          >
            <ShellStatusStrip />
            {props.title || props.pageSubtitle ? (
              <div className="pageHead">
                {props.title ? <div className="h1">{props.title}</div> : null}
                {props.pageSubtitle ? (
                  <div className="muted">{props.pageSubtitle}</div>
                ) : null}
              </div>
            ) : null}
            {props.children}
          </OverlayScrollArea>

          <nav className="mobileBottomNav" aria-label="底部主导航">
            {nav.map((item) => {
              const NavIcon = item.icon;
              return (
                <a
                  key={`mobile-bottom-${item.key}`}
                  href={currentHref(item.to)}
                  className={
                    active === item.key
                      ? "mobileBottomNavItem mobileBottomNavItemActive"
                      : "mobileBottomNavItem"
                  }
                  aria-current={active === item.key ? "page" : undefined}
                  onClick={(event) => {
                    event.preventDefault();
                    navigate(item.to);
                  }}
                >
                  <NavIcon
                    className="mobileBottomNavIcon"
                    aria-hidden="true"
                    strokeWidth={2.1}
                  />
                  <span className="mobileBottomNavLabel">
                    {item.mobileLabel}
                  </span>
                </a>
              );
            })}
          </nav>
        </div>
      </ConfirmProvider>
    </UpdateActionTrackerProvider>
  );
}

export function FilterChips<T extends string>(props: {
  value: T;
  onChange: (v: T) => void;
  items: Array<{
    key: T;
    label: string;
    count?: number;
    activeTone?: "primary" | "ghost";
  }>;
}) {
  return (
    <ToggleGroup
      aria-label="过滤条件"
      className="chipRow"
      onValueChange={(value) => {
        if (value) props.onChange(value as T);
      }}
      type="single"
      value={props.value}
    >
      {props.items.map((it) => (
        <ToggleGroupItem
          key={it.key}
          className={props.value === it.key ? "chip chipActive" : "chip"}
          title={it.count != null ? `${it.label}: ${it.count}` : it.label}
          value={it.key}
          variant="outline"
        >
          <span>{it.label}</span>
          {it.count != null ? (
            <span className="chipCount">{it.count}</span>
          ) : null}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  );
}
