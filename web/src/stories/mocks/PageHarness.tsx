import { useEffect, useState, type ReactNode } from "react";
import { writeDockrevRuntimeMode, type DockrevRuntimeMode } from "../../demo/runtime";
import { AppShell } from "../../Shell";
import { DetailRouteServiceTree } from "../../components/DetailRouteServiceTree";
import {
  QueueContextNavigation,
  SettingsContextNavigation,
} from "../../components/PageContextNavigation";
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
    onContextNavigation: (node: ReactNode) => void;
    onLastScanHint: (lastScan?: string) => void;
  }) => ReactNode;
}) {
  void props.sidebarCollapsed;
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
    onContextNavigation: (node: ReactNode) => void;
    onLastScanHint: (lastScan?: string) => void;
  }) => ReactNode;
}) {
  const [topActions, setTopActions] = useState<ReactNode>(null);
  const [pageTitle, setPageTitle] = useState(props.title ?? "");
  const [topbarContent, setTopbarContent] = useState<ReactNode>(null);
  const [contextNavigation, setContextNavigation] = useState<ReactNode>(null);
  const [lastScanHint, setLastScanHint] = useState<string | undefined>(
    undefined,
  );
  const [releaseDrawerState, setReleaseDrawerState] = useState(
    CLOSED_GITHUB_RELEASE_DRAWER_STATE,
  );
  const [route, setRoute] = useState<Route>(props.route);
  const detailContextNavigation =
    route.name === "services" || route.name === "stack" || route.name === "service" ? (
      <DetailRouteServiceTree route={route} variant="desktop" />
    ) : null;
  const staticContextNavigation =
    route.name === "queue" ||
    route.name === "job" ||
    route.name === "version-inference" ||
    route.name === "ghcr-webhooks" ||
    route.name === "ghcr-webhook-inbox" ? (
      <QueueContextNavigation />
    ) : route.name === "settings" || route.name === "ghcr-webhook-registry" ? (
      <SettingsContextNavigation section={route.name === "settings" ? route.section : "integrations"} />
    ) : null;
  const resolvedContextNavigation =
    detailContextNavigation ?? staticContextNavigation ?? contextNavigation;

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
    if (window.location.pathname !== targetHref) {
      window.history.replaceState({}, "", targetHref);
    }
  }, [props.route]);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const sync = () => setRoute(parseRoute(currentRoutePathname()));
    window.addEventListener("popstate", sync);
    const unsubscribe = subscribeNavigation(sync);
    return () => {
      window.removeEventListener("popstate", sync);
      unsubscribe();
    };
  }, []);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const sync = () => setReleaseDrawerState(readGitHubReleaseDrawerState());
    const handleLocation = () => sync();
    sync();
    window.addEventListener("popstate", handleLocation);
    window.addEventListener(
      RELEASE_DRAWER_LOCATION_EVENT,
      handleLocation as EventListener,
    );
    return () => {
      window.removeEventListener("popstate", handleLocation);
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
        contextNavigation={resolvedContextNavigation}
        contextNavigationTitle={detailContextNavigation ? "服务导航" : "页面内导航"}
        authIdentity={props.authIdentity}
        lastScanHint={lastScanHint}
      >
        {props.children({
          route,
          onTopActions: setTopActions,
          onPageTitle: setPageTitle,
          onTopbarContent: setTopbarContent,
          onSidebarNavContent: () => undefined,
          onMobileNavContent: () => undefined,
          onContextNavigation: setContextNavigation,
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
