import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  Gauge,
  LayoutDashboard,
  ListChecks,
  Settings,
  Trash2,
  X,
  type LucideIcon,
} from 'lucide-react'
import { getDockrevVersion } from './api'
import { AppShellStatusBanner } from './components/AppShellStatusBanner'
import { PwaUpdateBubble } from './components/PwaUpdateBubble'
import { usePwaStatus } from './pwaStatus'
import { Button, OverlayScrollArea, ToggleGroup, ToggleGroupItem } from './ui'
import { ConfirmProvider } from './ConfirmProvider'
import { BrandLogo } from './BrandLogo'
import { UpdateActionTrackerProvider } from './updateActionTracking'
import type { Route } from './routes'
import { currentHref, navigate } from './routes'
import { TopbarUserIdentity } from './components/TopbarUserIdentity'
import { ThemePreferenceControl } from './components/ThemePreferenceControl'
import { SidebarAppMeta } from './components/SidebarAppMeta'
import type { TopbarAuthIdentity } from './topbarAuthIdentity'

const MOBILE_MENU_MEDIA_QUERY = "(max-width: 960px)";
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
    dismissOfflineReady,
  } = usePwaStatus()

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
  contextNavigation?: ReactNode;
  contextNavigationTitle?: string;
  /** @deprecated use contextNavigation; retained so older page harnesses remain source-compatible. */
  sidebarNavContent?: ReactNode;
  /** @deprecated use contextNavigation; detail content is now a page context. */
  detailSidebarContent?: ReactNode;
  detailSidebarTitle?: string;
  /** @deprecated use contextNavigation; desktop and mobile now share one node. */
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
  const contextNavigation =
    props.contextNavigation ??
    props.detailSidebarContent ??
    props.sidebarNavContent ??
    props.mobileNavContent;
  const mobileMenuVisible =
    mobileDrawerOpenFor === routeHref &&
    mobileMenuMediaMatches &&
    Boolean(contextNavigation);
  const hasContextNavigation = Boolean(contextNavigation);
  const mobileDrawerTitle =
    props.mobileDrawerTitle ?? props.contextNavigationTitle ?? (hasContextNavigation ? "页面内导航" : "主导航");
  const serviceTopbarContext =
    props.route.name === "service" && (props.title || props.topbarContent) ? (
      <div className="topbarServiceContext">
        {props.title ? (
          <div className="topbarRouteTitle" data-slot="service-title">
            {props.title}
          </div>
        ) : null}
        {props.topbarContent ? (
          <div className="topbarServiceMetrics" data-slot="service-metrics">
            {props.topbarContent}
          </div>
        ) : null}
      </div>
    ) : null;
  const shellClassName = [
    "appShell",
    props.route.name === "settings" ? "appShellSettings" : null,
    props.topbarContent ? "appShellWithTopbarContent" : null,
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
            <div className="topbarMain">
              <div className="topbarLeft">
                {hasContextNavigation ? (
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
              {serviceTopbarContext}
              {props.route.name !== "service" && props.topbarContent ? (
                <div className="topbarGlobalContent">{props.topbarContent}</div>
              ) : null}
              <div className="topbarRight">
                {mobileMenuMediaMatches && active === "settings" ? (
                  <ThemePreferenceControl variant="icon" />
                ) : null}
                {props.topActions ? (
                  <div className="topActions">{props.topActions}</div>
                ) : null}
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
                {props.route.name !== "stack" && props.route.name !== "service" ? (
                  <div className="mobileMenuDrawerTitle">{mobileDrawerTitle}</div>
                ) : null}
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
            {mobileMenuVisible && contextNavigation ? (
              <div className="mobileMenuEmbeddedContent">
                {contextNavigation}
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

          <aside className="sidebar" role="complementary" aria-label="主导航侧栏">
            <div className="sidebarNavHeader">
              <span className="sidebarSectionLabel sidebarNavLabel">导航</span>
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
                    aria-label={item.label}
                    title={item.label}
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
            <div className="sidebarContextViewport">
              <OverlayScrollArea className="sidebarContextScroll" role="region" aria-label={props.contextNavigationTitle ?? "页面内导航"} viewportLabel={props.contextNavigationTitle ?? "页面内导航"}>
                {!mobileMenuMediaMatches ? contextNavigation : null}
              </OverlayScrollArea>
            </div>
            <div className="sidebarMeta">
              {!mobileMenuMediaMatches ? <TopbarUserIdentity authIdentity={props.authIdentity} placement="sidebar" /> : null}
              {!mobileMenuMediaMatches ? <div className="sidebarThemeControl"><ThemePreferenceControl variant="segmented" /></div> : null}
              <div className="sidebarMetaDivider" aria-hidden="true" />
              <SidebarAppMeta
                collapsed={false}
                versionDisplay={versionDisplay}
                versionHref={versionHref}
              />
            </div>
          </aside>

          <OverlayScrollArea
            className={
              props.route.name === "job" ? "content contentJobDetail" : "content"
            }
            role="main"
            viewportLabel="主内容"
          >
            <ShellStatusStrip />
            {(props.title || props.pageSubtitle) && props.route.name !== "service" ? (
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
          <PwaUpdateBubble />
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
