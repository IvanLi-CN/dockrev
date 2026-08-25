import "./App.css";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { AppShell } from "./Shell";
import { PwaUpdateBubble } from "./components/PwaUpdateBubble";
import { DetailRouteServiceTree } from "./components/DetailRouteServiceTree";
import type { Route } from "./routes";
import { currentRoutePathname, navigate } from "./routes";
import { OverviewPage } from "./pages/OverviewPage";
import { QueuePage } from "./pages/QueuePage";
import { JobDetailPage } from "./pages/JobDetailPage";
import { ServicesPage } from "./pages/ServicesPage";
import { CleanupPage } from "./pages/CleanupPage";
import { ServiceDetailPage } from "./pages/ServiceDetailPage";
import { StackDetailPage } from "./pages/StackDetailPage";
import { SettingsPage } from "./pages/SettingsPage";
import { VersionInferencePage } from "./pages/VersionInferencePage";
import { GhcrWebhookQueuePage } from "./pages/GhcrWebhookQueuePage";
import { GhcrWebhookInboxPage } from "./pages/GhcrWebhookInboxPage";
import { GhcrWebhookRegistryPage } from "./pages/GhcrWebhookRegistryPage";
import { SupervisorMisroutePage } from "./pages/SupervisorMisroutePage";
import { BrandLogo } from "./BrandLogo";
import { DeployWelcomePage } from "./pages/DeployWelcomePage";
import { UnauthorizedPage } from "./pages/UnauthorizedPage";
import { useRoute } from "./useRoute";
import { usePwaStatus } from "./pwaStatus";
import { useManagementEventBatch, useManagementEvents } from "./managementEvents";
import { AppShellStatusBanner } from "./components/AppShellStatusBanner";
import { IconButton } from "./ui";
import { RefreshCw } from "lucide-react";
import { shouldApplyUpdateOnPathnameNavigation } from "./pwaUpdateLifecycle";
import {
  AUTH_RECOVERED_EVENT,
  AUTH_REQUIRED_EVENT,
  getDeployWelcome,
  getSettings,
  refreshDeployCheckReport,
  type AuthRequiredDetails,
} from "./api";
import {
  hasBlockingDeployCheckFailure,
  loadDeployCheckReport,
} from "./deployCheck";
import { TopbarUserIdentity } from "./components/TopbarUserIdentity";
import { GitHubReleaseDrawer } from "./components/GitHubReleaseDrawer";
import {
  CLOSED_GITHUB_RELEASE_DRAWER_STATE,
  RELEASE_DRAWER_LOCATION_EVENT,
  closeGitHubReleaseDrawer,
  readGitHubReleaseDrawerState,
  shouldResetReleaseDrawerOnRouteChange,
} from "./releaseDrawer";
import {
  buildFallbackTopbarAuthIdentity,
  buildTopbarAuthIdentityFromAuthRequired,
  buildTopbarAuthIdentityFromSettings,
  type TopbarAuthIdentity,
} from "./topbarAuthIdentity";

function pageTitle(route: Route): { title: string; pageSubtitle?: string } {
  switch (route.name) {
    case "overview":
      return {
        title: "",
      };
    case "queue":
      return { title: "任务队列" };
    case "job":
      return { title: "任务详情" };
    case "services":
      return {
        title: "服务",
        pageSubtitle: "运行态与结果、发现异常、更新候选与归档恢复",
      };
    case "cleanup":
      return {
        title: "清理",
        pageSubtitle:
          "按规则预览 docker prune 候选，支持全局 / Stack / 服务三级清理",
      };
    case "version-inference":
      return {
        title: "版本推测",
        pageSubtitle: "镜像版本推测任务与缓存状态总览",
      };
    case "ghcr-webhooks":
      return {
        title: "GHCR Webhook",
        pageSubtitle: "Webhook 注册/反注册任务与巡检状态",
      };
    case "ghcr-webhook-inbox":
      return {
        title: "Webhook 收件箱",
        pageSubtitle: "按 delivery 展示事件、处理结果与响应状态",
      };
    case "ghcr-webhook-registry":
      return {
        title: "GHCR Webhook 维护",
        pageSubtitle: "集中维护仓库 webhook 注册状态、重试注册与删除反注册任务",
      };
    case "deploy-check":
      return {
        title: "部署检查",
        pageSubtitle: "面向运维：按功能能力判定配置完整性（PASS/FAIL）",
      };
    case "settings":
      return {
        title: "系统设置",
        pageSubtitle: "Forward Auth · 用户/组鉴权 · 通知配置 · 备份默认策略",
      };
    case "service":
      return {
        title: "",
      };
    case "stack":
      return { title: "" };
    case "supervisor-misroute":
      return { title: "部署问题" };
  }
}

function ManagementEventsStatusBanner() {
  const managementEvents = useManagementEvents()
  if (managementEvents.connection !== "reconnecting") return null

  const errorLabel = managementEvents.lastError === "eventsource_error"
    ? "连接错误"
    : managementEvents.lastError === "open_timeout"
      ? "连接打开超时"
      : managementEvents.lastError === "heartbeat_timeout"
        ? "心跳超时"
        : managementEvents.lastError === "protocol_invalid"
          ? "协议数据异常"
          : "等待连接"
  const attemptLabel = managementEvents.reconnectAttempt > 0
    ? `第 ${managementEvents.reconnectAttempt} 次重试`
    : "首次连接"
  const lastActivityLabel = managementEvents.lastActivityAt
    ? `最近活动：${new Date(managementEvents.lastActivityAt).toLocaleTimeString()}。`
    : "尚未收到管理事件。"

  return (
    <AppShellStatusBanner
      tone="warning"
      title="管理事件流重连中"
      detail={`${errorLabel}，${attemptLabel}。${lastActivityLabel}`}
      actions={
        <IconButton title="立即重试管理事件流" onClick={managementEvents.retryNow}>
          <RefreshCw size={16} aria-hidden="true" />
        </IconButton>
      }
    />
  )
}

export default function App() {
  const route = useRoute();
  const { applyUpdateOnNavigation } = usePwaStatus();
  const [releaseDrawerState, setReleaseDrawerState] = useState(() =>
    readGitHubReleaseDrawerState(),
  );
  const [pageActions, setPageActions] = useState<ReactNode>(null);
  const [servicePageTitle, setServicePageTitle] = useState<string>("");
  const [topbarContent, setTopbarContent] = useState<ReactNode>(null);
  const [sidebarNavContent, setSidebarNavContent] = useState<ReactNode>(null);
  const [mobileNavContent, setMobileNavContent] = useState<ReactNode>(null);
  const [lastScanHint, setLastScanHint] = useState<string | undefined>(
    undefined,
  );
  const [deployWelcomeState, setDeployWelcomeState] = useState<{
    loaded: boolean;
    neverAutoOpen: boolean;
  }>({
    loaded: false,
    neverAutoOpen: true,
  });
  const [deployCheckGate, setDeployCheckGate] = useState<
    "loading" | "pass" | "fail"
  >("loading");
  const [authFailure, setAuthFailure] = useState<AuthRequiredDetails | null>(
    null,
  );
  const [authIdentity, setAuthIdentity] = useState<TopbarAuthIdentity>(() =>
    buildFallbackTopbarAuthIdentity(),
  );
  const authFailureActiveRef = useRef(false);
  const authFailureVersionRef = useRef(0);
  const authIdentityRefreshInFlightRef = useRef(false);
  const suppressNextAuthRecoveredRef = useRef(false);
  const deployCheckBackgroundRefreshInFlightRef = useRef(false);
  const deployCheckGateRequestIdRef = useRef(0);
  const previousRoutePathRef = useRef<string | null>(null);
  const previousUpdateNavigationPathRef = useRef<string | null>(null);

  const head = useMemo(() => pageTitle(route), [route]);
  const resolvedHead = useMemo(
    () =>
      route.name === "service" && servicePageTitle
        ? { ...head, title: servicePageTitle }
        : head,
    [head, route.name, servicePageTitle],
  );
  const detailSidebarContent = useMemo(() => {
    if (route.name !== "stack" && route.name !== "service") return null;
    return <DetailRouteServiceTree route={route} variant="desktop" />;
  }, [route]);
  const mobileDrawerContent = useMemo(() => {
    if (route.name === "stack" || route.name === "service") {
      return <DetailRouteServiceTree route={route} variant="mobile" />;
    }
    return mobileNavContent;
  }, [mobileNavContent, route]);
  const topActions = useMemo(() => {
    return <>{pageActions}</>;
  }, [pageActions]);

  useEffect(() => {
    if (route.name !== "overview" && route.name !== "service") {
      setTopbarContent(null);
      setSidebarNavContent(null);
      setMobileNavContent(null);
    }
  }, [route.name]);

  const serviceRouteKey =
    route.name === "service" ? `${route.stackId}:${route.serviceId}` : null;

  useEffect(() => {
    setServicePageTitle("");
    if (serviceRouteKey !== null) setTopbarContent(null);
  }, [serviceRouteKey]);

  const refreshAuthIdentity = useCallback(async () => {
    if (authIdentityRefreshInFlightRef.current) return null;
    authIdentityRefreshInFlightRef.current = true;
    suppressNextAuthRecoveredRef.current = true;
    const authFailureVersionAtStart = authFailureVersionRef.current;
    try {
      const settings = await getSettings();
      if (
        authFailureActiveRef.current ||
        authFailureVersionAtStart !== authFailureVersionRef.current
      ) {
        return null;
      }
      return buildTopbarAuthIdentityFromSettings(settings.auth);
    } finally {
      authIdentityRefreshInFlightRef.current = false;
      suppressNextAuthRecoveredRef.current = false;
    }
  }, []);
  const refreshDeployCheckGate = useCallback(async (requestBackgroundRefresh = false) => {
    const requestId = ++deployCheckGateRequestIdRef.current;
    const envelope = await loadDeployCheckReport();
    if (requestId !== deployCheckGateRequestIdRef.current) return;
    const report = envelope.report;
    if (!report) return;
    const blocked = hasBlockingDeployCheckFailure(report);
    setDeployCheckGate(blocked ? "fail" : "pass");
    if (
      requestBackgroundRefresh &&
      !blocked &&
      !envelope.refreshing &&
      !deployCheckBackgroundRefreshInFlightRef.current
    ) {
      deployCheckBackgroundRefreshInFlightRef.current = true;
      void refreshDeployCheckReport()
        .catch(() => {})
        .finally(() => {
          deployCheckBackgroundRefreshInFlightRef.current = false;
        });
    }
  }, []);

  useEffect(() => {
    const requestId = deployCheckGateRequestIdRef.current + 1;
    void refreshDeployCheckGate(true).catch(() => {
      if (requestId === deployCheckGateRequestIdRef.current) {
        setDeployCheckGate("loading");
      }
    });
  }, [refreshDeployCheckGate]);

  useManagementEventBatch(({ events, resyncRequired }) => {
    if (
      resyncRequired ||
      events.some((event) => event.domain === "deploy_check")
    ) {
      void refreshDeployCheckGate().catch(() => {});
    }
  });

  useManagementEventBatch(({ events, resyncRequired }) => {
    const deployWelcomeChanged = events.some(
      (event) =>
        event.domain === "settings" &&
        event.summary.operation === "deploy_welcome_updated",
    );
    if (!resyncRequired && !deployWelcomeChanged) return;
    void getDeployWelcome()
      .then((settings) => {
        setDeployWelcomeState({
          loaded: true,
          neverAutoOpen: settings.neverAutoOpen,
        });
      })
      .catch(() => {});
  });

  useEffect(() => {
    let cancelled = false;
    void getDeployWelcome()
      .then((settings) => {
        if (cancelled) return;
        setDeployWelcomeState({
          loaded: true,
          neverAutoOpen: settings.neverAutoOpen,
        });
      })
      .catch(() => {
        if (cancelled) return;
        // Fail open to dashboard when the preference endpoint is unavailable.
        setDeployWelcomeState({ loaded: true, neverAutoOpen: true });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (deployCheckGate === "fail" && route.name !== "deploy-check") {
      navigate({ name: "deploy-check" });
      return;
    }
    if (deployCheckGate !== "pass") return;
    if (!deployWelcomeState.loaded || deployWelcomeState.neverAutoOpen) return;
    if (route.name !== "overview") return;
    if (typeof window === "undefined") return;
    const key = "dockrev:deployWelcome:redirected";
    if (window.sessionStorage.getItem(key) === "1") return;
    window.sessionStorage.setItem(key, "1");
    navigate({ name: "deploy-check" });
  }, [deployCheckGate, deployWelcomeState, route.name]);

  useEffect(() => {
    if (typeof window === "undefined") return;

    const onAuthRequired = (event: Event) => {
      const detail = (
        event as CustomEvent<{ details?: AuthRequiredDetails | null }>
      ).detail;
      const nextAuthFailure = detail?.details ?? null;
      authFailureActiveRef.current = true;
      authFailureVersionRef.current += 1;
      setAuthFailure(nextAuthFailure);
      if (nextAuthFailure) {
        setAuthIdentity(
          buildTopbarAuthIdentityFromAuthRequired(nextAuthFailure),
        );
      } else {
        setAuthIdentity(buildFallbackTopbarAuthIdentity());
      }
    };
    const onAuthRecovered = () => {
      const hadAuthFailure = authFailureActiveRef.current;
      authFailureActiveRef.current = false;
      setAuthFailure(null);
      if (suppressNextAuthRecoveredRef.current) {
        suppressNextAuthRecoveredRef.current = false;
        return;
      }
      if (!hadAuthFailure) return;
      void refreshAuthIdentity()
        .then((next) => {
          if (next) setAuthIdentity(next);
        })
        .catch(() => {});
    };

    window.addEventListener(AUTH_REQUIRED_EVENT, onAuthRequired);
    window.addEventListener(AUTH_RECOVERED_EVENT, onAuthRecovered);
    return () => {
      window.removeEventListener(AUTH_REQUIRED_EVENT, onAuthRequired);
      window.removeEventListener(AUTH_RECOVERED_EVENT, onAuthRecovered);
    };
  }, [refreshAuthIdentity]);

  useEffect(() => {
    let cancelled = false;
    void refreshAuthIdentity()
      .then((nextAuthIdentity) => {
        if (cancelled || !nextAuthIdentity) return;
        setAuthIdentity(nextAuthIdentity);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [refreshAuthIdentity]);

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

  useEffect(() => {
    const nextPathname = currentRoutePathname();
    const previousPathname = previousRoutePathRef.current;
    previousRoutePathRef.current = nextPathname;

    const hashRouting =
      typeof window !== "undefined" &&
      (window.location.hash.startsWith("#/") ||
        window.location.pathname.endsWith("/iframe.html"));

    if (
      shouldResetReleaseDrawerOnRouteChange({
        drawerOpen: releaseDrawerState.open,
        hashRouting,
        previousPathname,
        nextPathname,
      })
    ) {
      closeGitHubReleaseDrawer("replace");
      setReleaseDrawerState(CLOSED_GITHUB_RELEASE_DRAWER_STATE);
      return;
    }

    setReleaseDrawerState(readGitHubReleaseDrawerState());
  }, [releaseDrawerState.open, route]);

  useEffect(() => {
    const nextPathname = currentRoutePathname();
    const previousPathname = previousUpdateNavigationPathRef.current;
    previousUpdateNavigationPathRef.current = nextPathname;

    if (shouldApplyUpdateOnPathnameNavigation(previousPathname, nextPathname)) {
      // The URL has already changed, so a controllerchange reload returns to this target route.
      void applyUpdateOnNavigation();
    }
  }, [applyUpdateOnNavigation, route]);

  if (route.name === "supervisor-misroute") {
    return (
      <div className="standaloneShell">
        <div className="standaloneContent">
          <div className="standaloneHead">
            <div className="standaloneHeadLeft">
              <div className="brand">
                <BrandLogo />
              </div>
            </div>
            <div className="standaloneHeadRight">
              <TopbarUserIdentity authIdentity={authIdentity} />
            </div>
          </div>
          <SupervisorMisroutePage
            basePath={route.basePath}
            pathname={route.pathname}
          />
        </div>
      </div>
    );
  }

  if (authFailure) {
    return (
      <AppShell
        route={route}
        title={resolvedHead.title}
        pageSubtitle={resolvedHead.pageSubtitle}
        topActions={null}
        authIdentity={authIdentity}
        lastScanHint={lastScanHint}
      >
        <ManagementEventsStatusBanner />
        <UnauthorizedPage authDetails={authFailure} />
      </AppShell>
    );
  }

  if (route.name === "deploy-check") {
    return (
      <>
        <DeployWelcomePage />
        <PwaUpdateBubble />
      </>
    );
  }

  if (deployCheckGate !== "pass") {
    return (
      <div className="deployGateLoading" role="status" aria-live="polite">
        {deployCheckGate === "loading"
          ? "正在验证部署检查…"
          : "部署检查未通过，正在打开故障门禁…"}
      </div>
    );
  }

  return (
    <>
      <AppShell
        route={route}
        title={resolvedHead.title}
        pageSubtitle={resolvedHead.pageSubtitle}
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
        authIdentity={authIdentity}
        lastScanHint={lastScanHint}
      >
        <ManagementEventsStatusBanner />
        {route.name === "overview" ? (
          <OverviewPage
            onLastScanHint={setLastScanHint}
            onTopActions={setPageActions}
            onTopbarContent={setTopbarContent}
            onSidebarNavContent={setSidebarNavContent}
            onMobileNavContent={setMobileNavContent}
          />
        ) : null}
        {route.name === "queue" ? (
          <QueuePage onTopActions={setPageActions} />
        ) : null}
        {route.name === "job" ? (
          <JobDetailPage jobId={route.jobId} onTopActions={setPageActions} />
        ) : null}
        {route.name === "services" ? (
          <ServicesPage
            onLastScanHint={setLastScanHint}
            onTopActions={setPageActions}
          />
        ) : null}
        {route.name === "cleanup" ? (
          <CleanupPage
            onLastScanHint={setLastScanHint}
            onTopActions={setPageActions}
          />
        ) : null}
        {route.name === "version-inference" ? (
          <VersionInferencePage
            onLastScanHint={setLastScanHint}
            onTopActions={setPageActions}
          />
        ) : null}
        {route.name === "ghcr-webhooks" ? (
          <GhcrWebhookQueuePage onTopActions={setPageActions} />
        ) : null}
        {route.name === "ghcr-webhook-inbox" ? (
          <GhcrWebhookInboxPage onTopActions={setPageActions} />
        ) : null}
        {route.name === "ghcr-webhook-registry" ? (
          <GhcrWebhookRegistryPage onTopActions={setPageActions} />
        ) : null}
        {route.name === "settings" ? (
          <SettingsPage section={route.section} onTopActions={setPageActions} />
        ) : null}
        {route.name === "stack" ? (
          <StackDetailPage
            key={route.stackId}
            stackId={route.stackId}
            onLastScanHint={setLastScanHint}
            onTopActions={setPageActions}
          />
        ) : null}
        {route.name === "service" ? (
          <ServiceDetailPage
            key={`${route.stackId}:${route.serviceId}`}
            stackId={route.stackId}
            serviceId={route.serviceId}
            section={route.section}
            onLastScanHint={setLastScanHint}
            onTopActions={setPageActions}
            onPageTitle={setServicePageTitle}
            onTopbarContent={setTopbarContent}
          />
        ) : null}
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
