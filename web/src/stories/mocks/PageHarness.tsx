import { useEffect, useState, type ReactNode } from "react";
import { writeDockrevRuntimeMode, type DockrevRuntimeMode } from "../../demo/runtime";
import {
  APP_SHELL_SIDEBAR_COLLAPSED_STORAGE_KEY,
  AppShell,
} from "../../Shell";
import { DetailRouteServiceTree } from "../../components/DetailRouteServiceTree";
import { GitHubReleaseDrawer } from "../../components/GitHubReleaseDrawer";
import {
  CLOSED_GITHUB_RELEASE_DRAWER_STATE,
  RELEASE_DRAWER_LOCATION_EVENT,
  RELEASE_DRAWER_QUERY_KEYS,
  closeGitHubReleaseDrawer,
  readGitHubReleaseDrawerState,
} from "../../releaseDrawer";
import {
  currentHref,
  currentRoutePathname,
  parseRoute,
  subscribeNavigation,
  type Route,
} from "../../routes";
import type { TopbarAuthIdentity } from "../../topbarAuthIdentity";

export function PageHarness(props: {
  route: Route;
  title?: string;
  pageSubtitle?: string;
  sidebarCollapsed?: boolean;
  authIdentity?: TopbarAuthIdentity | null;
  runtimeMode?: DockrevRuntimeMode | null;
  children: (ctx: {
    route: Route;
    onTopActions: (node: ReactNode) => void;
    onPageTitle: (title: string) => void;
    onTopbarContent: (node: ReactNode) => void;
    onSidebarNavContent: (node: ReactNode) => void;
    onMobileNavContent: (node: ReactNode) => void;
    onLastScanHint: (lastScan?: string) => void;
  }) => ReactNode;
}) {
  if (typeof window !== "undefined" && props.sidebarCollapsed !== undefined) {
    window.localStorage.setItem(
      APP_SHELL_SIDEBAR_COLLAPSED_STORAGE_KEY,
      props.sidebarCollapsed ? "1" : "0",
    );
  }
  return <PageHarnessInner key={currentHref(props.route)} {...props} />;
}

function PageHarnessInner(props: {
  route: Route;
  title?: string;
  pageSubtitle?: string;
  sidebarCollapsed?: boolean;
  authIdentity?: TopbarAuthIdentity | null;
  runtimeMode?: DockrevRuntimeMode | null;
  children: (ctx: {
    route: Route;
    onTopActions: (node: ReactNode) => void;
    onPageTitle: (title: string) => void;
    onTopbarContent: (node: ReactNode) => void;
    onSidebarNavContent: (node: ReactNode) => void;
    onMobileNavContent: (node: ReactNode) => void;
    onLastScanHint: (lastScan?: string) => void;
  }) => ReactNode;
}) {
  const [topActions, setTopActions] = useState<ReactNode>(null);
  const [pageTitle, setPageTitle] = useState(props.title ?? "");
  const [topbarContent, setTopbarContent] = useState<ReactNode>(null);
  const [sidebarNavContent, setSidebarNavContent] = useState<ReactNode>(null);
  const [mobileNavContent, setMobileNavContent] = useState<ReactNode>(null);
  const [lastScanHint, setLastScanHint] = useState<string | undefined>(
    undefined,
  );
  const [releaseDrawerState, setReleaseDrawerState] = useState(
    CLOSED_GITHUB_RELEASE_DRAWER_STATE,
  );
  const [route, setRoute] = useState<Route>(props.route);
  const detailSidebarContent =
    route.name === "stack" || route.name === "service" ? (
      <DetailRouteServiceTree route={route} variant="desktop" />
    ) : null;
  const mobileDrawerContent =
    route.name === "stack" || route.name === "service" ? (
      <DetailRouteServiceTree route={route} variant="mobile" />
    ) : (
      mobileNavContent
    );

  useEffect(() => {
    const previousMode = props.runtimeMode ?? null
    writeDockrevRuntimeMode(previousMode)
    return () => {
      writeDockrevRuntimeMode(null)
    }
  }, [props.runtimeMode])

  useEffect(() => {
    if (typeof window === "undefined") return;
    const url = new URL(window.location.href);
    let changed = false;
    for (const key of RELEASE_DRAWER_QUERY_KEYS) {
      if (!url.searchParams.has(key)) continue;
      url.searchParams.delete(key);
      changed = true;
    }
    if (!changed) return;
    window.history.replaceState(
      {},
      "",
      `${url.pathname}${url.search}${url.hash}`,
    );
  }, []);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const targetHref = currentHref(props.route);
    if (targetHref.startsWith("#")) {
      if (window.location.hash !== targetHref)
        window.location.hash = targetHref;
      return;
    }
    if (window.location.pathname !== targetHref) {
      window.history.replaceState({}, "", targetHref);
    }
  }, [props.route]);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const sync = () => setRoute(parseRoute(currentRoutePathname()));
    window.addEventListener("popstate", sync);
    window.addEventListener("hashchange", sync);
    const unsubscribe = subscribeNavigation(sync);
    return () => {
      window.removeEventListener("popstate", sync);
      window.removeEventListener("hashchange", sync);
      unsubscribe();
    };
  }, []);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const sync = () => setReleaseDrawerState(readGitHubReleaseDrawerState());
    const handleLocation = () => sync();
    sync();
    window.addEventListener("popstate", handleLocation);
    window.addEventListener("hashchange", handleLocation);
    window.addEventListener(
      RELEASE_DRAWER_LOCATION_EVENT,
      handleLocation as EventListener,
    );
    return () => {
      window.removeEventListener("popstate", handleLocation);
      window.removeEventListener("hashchange", handleLocation);
      window.removeEventListener(
        RELEASE_DRAWER_LOCATION_EVENT,
        handleLocation as EventListener,
      );
    };
  }, []);

  return (
    <>
      <AppShell
        route={route}
        title={pageTitle}
        pageSubtitle={props.pageSubtitle}
        topActions={topActions}
        topbarContent={topbarContent}
        sidebarNavContent={sidebarNavContent}
        detailSidebarContent={detailSidebarContent}
        detailSidebarTitle={undefined}
        mobileNavContent={mobileDrawerContent}
        mobileDrawerTitle={
          detailSidebarContent
            ? "服务导航"
            : mobileDrawerContent
              ? "页面工具"
              : undefined
        }
        authIdentity={props.authIdentity}
        lastScanHint={lastScanHint}
      >
        {props.children({
          route,
          onTopActions: setTopActions,
          onPageTitle: setPageTitle,
          onTopbarContent: setTopbarContent,
          onSidebarNavContent: setSidebarNavContent,
          onMobileNavContent: setMobileNavContent,
          onLastScanHint: setLastScanHint,
        })}
      </AppShell>
      <GitHubReleaseDrawer
        open={releaseDrawerState.open}
        serviceId={releaseDrawerState.serviceId}
        version={releaseDrawerState.version}
        onOpenChange={(open) => {
          if (open) return;
          closeGitHubReleaseDrawer("replace");
        }}
      />
    </>
  );
}
