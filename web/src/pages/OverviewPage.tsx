import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import {
  Activity,
  Info,
  RefreshCw,
  ScanSearch,
} from "lucide-react";
import {
  ApiError,
  getHomepageNav,
  triggerCheck,
  triggerUpdate,
  type HomepageNavItem,
  type Service,
  type ServiceResourceOverviewItem,
  type ServiceResourceOverviewResponse,
} from "../api";
import { HomepageServiceIcon } from "../components/HomepageServiceIcon";
import { ReadonlySnapshotNotice } from "../components/ReadonlySnapshotNotice";
import { ServiceUpdateConfirmDetails } from "../components/ServiceUpdateConfirmDetails";
import { usePwaStatus } from "../pwaStatus";
import {
  buildReadonlySnapshotKey,
  readReadonlySnapshot,
  writeReadonlySnapshot,
} from "../readonlySnapshotCache";
import { navigate } from "../routes";
import { buildUpdateServiceTarget } from "../updateTargets";
import { isDockrevAppDemoRuntime } from "../demo/runtime";
import {
  canRestorePersistedHomepageSnapshot,
  homepageSnapshotFromResponse,
  markHomepageSnapshotResourceStale,
  normalizeHomepageHref,
  readHomepageSnapshot,
  writeHomepageSnapshot,
  type HomepageSnapshotCard,
} from "./homepageSnapshot";
import {
  Button,
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Mono,
} from "../ui";
import { statusLabel } from "../updateStatus";
import { useManagementEventBatch } from "../managementEvents";
import {
  CardMetric,
  HomepageClockBlock,
  HomepageHeaderContent,
  HomepageResourceMetrics,
  HomepageSearchForm,
  HomepageSidebarClock,
  HomepageTopStrip,
} from "./OverviewPageChrome";
import { HomepageFloatingToolPanel } from "./OverviewFloatingToolPanel";

const HOMEPAGE_COLUMN_BREAKPOINTS = [
  { query: "(max-width: 720px)", columns: 1 },
  { query: "(max-width: 1160px)", columns: 2 },
  { query: "(max-width: 1500px)", columns: 3 },
] as const;
const HOMEPAGE_PERSISTED_SNAPSHOT_KEY = buildReadonlySnapshotKey(
  "overview",
  "homepage-nav",
);
const HOMEPAGE_PERSISTED_SNAPSHOT_STALE_MS = 60_000;

type PersistedHomepageSnapshotPayload = {
  generatedAt: string;
  lastCheckAt: string | null;
  resourceSummary: ServiceResourceOverviewResponse;
  cards: HomepageSnapshotCard[];
};

type HomepageNavCard = HomepageSnapshotCard & {
  source: "live" | "snapshot";
};

type ServiceBadge = {
  label: string;
  tone: "running" | "healthy" | "stale" | "muted" | "updatable" | "hint" | "bad";
};

type HomepageCardGroup = {
  groupName: string;
  cards: HomepageNavCard[];
};

function currentHomepageColumnCount(): number {
  if (typeof window === "undefined") return 4;
  for (const breakpoint of HOMEPAGE_COLUMN_BREAKPOINTS) {
    if (window.matchMedia(breakpoint.query).matches) return breakpoint.columns;
  }
  return 4;
}

function useHomepageColumnCount(): number {
  const [columnCount, setColumnCount] = useState(currentHomepageColumnCount);

  useEffect(() => {
    const update = () => setColumnCount(currentHomepageColumnCount());
    const queries = HOMEPAGE_COLUMN_BREAKPOINTS.map((breakpoint) =>
      window.matchMedia(breakpoint.query),
    );

    update();
    for (const query of queries) query.addEventListener("change", update);
    return () => {
      for (const query of queries) query.removeEventListener("change", update);
    };
  }, []);

  return columnCount;
}

function balanceHomepageGroups(
  groups: HomepageCardGroup[],
  columnCount: number,
): HomepageCardGroup[][] {
  const safeColumnCount = Math.max(1, Math.min(4, Math.floor(columnCount)));
  const columns = Array.from({ length: safeColumnCount }, () => ({
    groups: [] as HomepageCardGroup[],
    weight: 0,
  }));

  for (const group of groups) {
    const target = columns.reduce((best, column) =>
      column.weight < best.weight ? column : best,
    );
    target.groups.push(group);
    target.weight += 1 + group.cards.length;
  }

  const populatedColumns = columns
    .map((column) => column.groups)
    .filter((column) => column.length > 0);
  return populatedColumns.length > 0 ? populatedColumns : [[]];
}

function snapshotCardsToNavCards(cards: HomepageSnapshotCard[]): HomepageNavCard[] {
  return cards.flatMap((card) => {
    const href = normalizeHomepageHref(card.href);
    return href ? [{ ...card, source: "snapshot" as const }] : [];
  });
}

function navCardsToSnapshot(cards: HomepageNavCard[]): HomepageSnapshotCard[] {
  return cards.map((card) => {
    const { source, ...snapshotCard } = card;
    void source;
    return snapshotCard;
  });
}

function homepageItemToCard(
  item: HomepageNavItem,
  source: HomepageNavCard["source"],
): HomepageNavCard | null {
  const homepageHref = normalizeHomepageHref(item.homepage?.href);
  if (!homepageHref) return null;
  const service: Service = {
    id: item.serviceId,
    name: item.serviceName,
    image: {
      ref: item.imageRef,
      tag: item.imageTag,
      digest: item.imageDigest ?? null,
      resolvedTag: item.imageResolvedTag ?? null,
      resolvedTags: item.imageResolvedTags ?? null,
    },
    homepage: item.homepage,
    candidate: item.candidate ?? null,
    ignore: item.ignore ?? null,
    versionInference: item.versionInference ?? null,
    newVersionDiscoveryCount: item.newVersionDiscoveryCount ?? null,
    settings: item.settings,
    archived: item.archived,
  };
  return {
    id: item.serviceId,
    stackId: item.stackId,
    stackName: item.stackName,
    serviceId: item.serviceId,
    serviceName: item.serviceName,
    imageRef: item.imageRef,
    groupName: item.homepage.group?.trim() || item.stackName,
    title: item.homepage.name?.trim() || item.serviceName,
    description: item.homepage.description?.trim() || item.imageRef,
    href: homepageHref,
    icon: item.homepage.icon ?? null,
    status:
      item.ignore?.matched
        ? "blocked"
        : item.candidate?.archMatch === "mismatch"
          ? "archMismatch"
          : item.candidate?.archMatch === "unknown"
            ? "hint"
            : item.candidate
              ? "updatable"
              : "ok",
    isDockrev: item.isDockrev,
    service,
    source,
  };
}

function homepageResponseToCards(items: HomepageNavItem[]): HomepageNavCard[] {
  return items
    .map((item) => homepageItemToCard(item, "live"))
    .filter((card): card is HomepageNavCard => card !== null);
}

function servicesEqual(left: Service, right: Service): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function cardsEqual(left: HomepageNavCard, right: HomepageNavCard): boolean {
  return (
    left.id === right.id &&
    left.stackId === right.stackId &&
    left.stackName === right.stackName &&
    left.serviceId === right.serviceId &&
    left.serviceName === right.serviceName &&
    left.imageRef === right.imageRef &&
    left.groupName === right.groupName &&
    left.title === right.title &&
    left.description === right.description &&
    left.href === right.href &&
    left.icon === right.icon &&
    left.status === right.status &&
    left.isDockrev === right.isDockrev &&
    servicesEqual(left.service, right.service)
  );
}

function mergeHomepageCards(
  previous: HomepageNavCard[],
  incoming: HomepageNavCard[],
): HomepageNavCard[] {
  const previousByServiceId = new Map(
    previous.map((card) => [card.serviceId, card] as const),
  );
  return incoming.map((incomingCard) => {
    const existing = previousByServiceId.get(incomingCard.serviceId);
    if (!existing) return incomingCard;
    if (existing.source === "live" && cardsEqual(existing, incomingCard)) {
      return existing;
    }
    return {
      ...existing,
      ...incomingCard,
      source: "live",
    };
  });
}

function mergeHomepageCardList(
  previous: HomepageNavCard[],
  incoming: HomepageNavCard[],
): HomepageNavCard[] {
  if (incoming.length === 0) return [];
  if (previous.length === 0) return incoming;
  return mergeHomepageCards(previous, incoming);
}

function matchesSearch(card: HomepageNavCard, query: string): boolean {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return true;
  const haystack = [
    card.groupName,
    card.title,
    card.description,
    card.imageRef,
    card.stackName,
    card.serviceName,
  ]
    .join(" ")
    .toLowerCase();
  return haystack.includes(normalized);
}

function formatPercent(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "-";
  if (value < 10) return `${value.toFixed(1)}%`;
  return `${value.toFixed(0)}%`;
}

function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes)) return "-";
  const units = ["B", "kB", "MB", "GB", "TB"];
  let value = bytes;
  let idx = 0;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }
  const digits = idx === 0 || value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${units[idx]}`;
}

function formatRate(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "-";
  if (value < 1) return "0 B/s";
  return `${formatBytes(value)}/s`;
}

function metricMap(
  overview: ServiceResourceOverviewResponse | null,
): Map<string, ServiceResourceOverviewItem> {
  const map = new Map<string, ServiceResourceOverviewItem>();
  for (const item of overview?.services ?? []) {
    map.set(item.serviceId, item);
  }
  return map;
}

function sumMetricValues<T>(
  items: T[],
  read: (item: T) => number | null | undefined,
): number | null {
  const values = items
    .map(read)
    .filter((value): value is number => value != null && Number.isFinite(value));
  if (values.length === 0) return null;
  return values.reduce((sum, value) => sum + value, 0);
}

function aggregateMetrics(items: ServiceResourceOverviewItem[]) {
  const active = items.filter((item) => item.sampledAt && !item.stale);
  return {
    activeCount: active.length,
    cpu: sumMetricValues(active, (item) => item.cpuPercent),
    memory: sumMetricValues(active, (item) => item.memUsedBytes),
    rx: sumMetricValues(active, (item) => item.netRxRateBps),
    tx: sumMetricValues(active, (item) => item.netTxRateBps),
  };
}

function serviceBadge(
  card: HomepageNavCard,
  metric: ServiceResourceOverviewItem | undefined,
  overview: ServiceResourceOverviewResponse | null,
  resourceUnavailable: boolean,
): ServiceBadge {
  if (card.status === "updatable")
    return { label: statusLabel(card.status), tone: "updatable" };
  if (card.status === "hint")
    return { label: statusLabel(card.status), tone: "hint" };
  if (card.status === "archMismatch" || card.status === "blocked")
    return { label: statusLabel(card.status), tone: "bad" };
  if (resourceUnavailable) return { label: "NO DATA", tone: "muted" };
  if (overview?.enabled === false) return { label: "NO DATA", tone: "muted" };
  if (metric?.sampledAt && !metric.stale)
    return { label: "RUNNING", tone: "running" };
  if (metric || overview) return { label: "NO DATA", tone: "muted" };
  return { label: "NO DATA", tone: "muted" };
}

function openHomepageHref(href: string) {
  window.open(href, "_blank", "noopener,noreferrer");
}

function eventTargetIsNestedAction(event: KeyboardEvent<HTMLElement>): boolean {
  const target = event.target;
  return (
    target instanceof HTMLElement &&
    target.closest("button, a, input, select, textarea") !== event.currentTarget
  );
}

export function OverviewPage(props: {
  onLastScanHint: (lastScan?: string) => void;
  onTopActions: (node: ReactNode) => void;
  onTopbarContent: (node: ReactNode) => void;
  onSidebarNavContent: (node: ReactNode) => void;
  onMobileNavContent: (node: ReactNode) => void;
}) {
  const {
    onLastScanHint,
    onMobileNavContent,
    onSidebarNavContent,
    onTopActions,
    onTopbarContent,
  } =
    props;
  const pageRef = useRef<HTMLDivElement | null>(null);
  const isAppDemoRuntime = isDockrevAppDemoRuntime();
  const { isOnline } = usePwaStatus();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [resourceError, setResourceError] = useState<string | null>(null);
  const [noticeCheckJobId, setNoticeCheckJobId] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(true);
  const [cachedCards, setCachedCards] = useState<HomepageNavCard[]>(() => {
    const snapshot = readHomepageSnapshot();
    if (!snapshot) return [];
    return snapshotCardsToNavCards(snapshot.cards);
  });
  const [hasCachedNavSnapshot, setHasCachedNavSnapshot] = useState(
    () => readHomepageSnapshot() !== null,
  );
  const [resourceFromCache, setResourceFromCache] = useState(() => readHomepageSnapshot() !== null);
  const [resourceOverview, setResourceOverview] =
    useState<ServiceResourceOverviewResponse | null>(() => {
      const snapshot = readHomepageSnapshot();
      if (!snapshot) return null;
      return snapshot.resourceSummary;
    });
  const [cards, setCards] = useState<HomepageNavCard[]>(() => {
    const snapshot = readHomepageSnapshot();
    if (!snapshot) return [];
    return snapshotCardsToNavCards(snapshot.cards);
  });
  const [liveLoaded, setLiveLoaded] = useState(false);
  const [, setPersistedSnapshotStatus] = useState<
    "missing" | "fresh" | "stale" | "expired" | "unsupported"
  >("missing");
  const [persistedSnapshotFetchedAt, setPersistedSnapshotFetchedAt] = useState<
    string | null
  >(null);
  const [search, setSearch] = useState("");
  const [searchDraft, setSearchDraft] = useState("");
  const [headerSearchOpen, setHeaderSearchOpen] = useState(false);
  const [updateDialogCard, setUpdateDialogCard] =
    useState<HomepageNavCard | null>(null);
  const [now] = useState(() => new Date());
  const homepageColumnCount = useHomepageColumnCount();

  const applySearch = useCallback(() => {
    setSearch(searchDraft);
  }, [searchDraft]);
  const applyHeaderSearch = useCallback(() => {
    applySearch();
    setHeaderSearchOpen(false);
  }, [applySearch]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const persisted = await readReadonlySnapshot<PersistedHomepageSnapshotPayload>(
        HOMEPAGE_PERSISTED_SNAPSHOT_KEY,
      );
      if (cancelled) return;
      setPersistedSnapshotStatus(persisted.status);
      setPersistedSnapshotFetchedAt(persisted.record?.fetchedAt ?? null);

      const legacy = readHomepageSnapshot();
      if (legacy) {
        void writeReadonlySnapshot(
          HOMEPAGE_PERSISTED_SNAPSHOT_KEY,
          {
            generatedAt: legacy.generatedAt,
            lastCheckAt: legacy.lastCheckAt,
            resourceSummary: legacy.resourceSummary,
            cards: legacy.cards,
          },
          {
            staleAfterMs: HOMEPAGE_PERSISTED_SNAPSHOT_STALE_MS,
            fetchedAt: Date.parse(legacy.generatedAt) || Date.now(),
          },
        );
        return;
      }

      if (
        !canRestorePersistedHomepageSnapshot(persisted.status) ||
        persisted.record === null
      ) {
        return;
      }
      const payload = persisted.record.payload;
      const resourceSummary =
        persisted.status === "stale"
          ? markHomepageSnapshotResourceStale({
              version: 2,
              generatedAt: payload.generatedAt,
              lastCheckAt: payload.lastCheckAt,
              resourceSummary: payload.resourceSummary,
              cards: payload.cards,
            }).resourceSummary
          : payload.resourceSummary;
      setHasCachedNavSnapshot(true);
      setCachedCards(snapshotCardsToNavCards(payload.cards));
      setCards((current) =>
        current.length > 0 ? current : snapshotCardsToNavCards(payload.cards),
      );
      setResourceOverview(resourceSummary);
      setResourceFromCache(true);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      const payload = await getHomepageNav();
      const liveCards = homepageResponseToCards(payload.items);
      onLastScanHint(payload.lastCheckAt ?? undefined);
      setResourceOverview(payload.resourceSummary);
      setResourceFromCache(false);
      setResourceError(null);
      setCards((previous) => mergeHomepageCardList(previous, liveCards));
      setLiveLoaded(true);
      const snapshotCards = navCardsToSnapshot(liveCards);
      const snapshot = homepageSnapshotFromResponse({
        generatedAt: payload.generatedAt,
        lastCheckAt: payload.lastCheckAt,
        resourceSummary: payload.resourceSummary,
        cards: snapshotCards,
      });
      writeHomepageSnapshot(snapshot);
      void writeReadonlySnapshot(
        HOMEPAGE_PERSISTED_SNAPSHOT_KEY,
        {
          generatedAt: snapshot.generatedAt,
          lastCheckAt: snapshot.lastCheckAt,
          resourceSummary: snapshot.resourceSummary,
          cards: snapshot.cards,
        },
        {
          staleAfterMs: HOMEPAGE_PERSISTED_SNAPSHOT_STALE_MS,
          fetchedAt: Date.parse(snapshot.generatedAt) || Date.now(),
        },
      );
      setCachedCards(snapshotCardsToNavCards(snapshot.cards));
      setHasCachedNavSnapshot(true);
    } catch (value: unknown) {
      setResourceError(
        value instanceof Error ? value.message : String(value),
      );
      throw value;
    } finally {
      setRefreshing(false);
    }
  }, [onLastScanHint]);

  const requestRefresh = refresh;

  useEffect(() => {
    void requestRefresh().catch((value: unknown) =>
      setError(value instanceof Error ? value.message : String(value)),
    );
  }, [requestRefresh]);

  useEffect(() => {
    onTopActions(
      <>
        <Button
          aria-label="刷新服务列表"
          variant="ghost"
          disabled={busy}
          onClick={() => {
            void (async () => {
              setBusy(true);
              setError(null);
              try {
                await requestRefresh();
              } catch (value: unknown) {
                setError(
                  value instanceof Error ? value.message : String(value),
                );
              } finally {
                setBusy(false);
              }
            })();
          }}
        >
          <RefreshCw className="homepageTopActionIcon" aria-hidden="true" />
          <span className="homepageTopActionLabel">刷新</span>
        </Button>
        <Button
          aria-label="立即扫描更新"
          variant="primary"
          disabled={busy}
          onClick={() => {
            void (async () => {
              setBusy(true);
              setError(null);
              setNoticeCheckJobId(null);
              try {
                const response = await triggerCheck("all");
                setNoticeCheckJobId(response.checkId);
                await requestRefresh();
              } catch (value: unknown) {
                if (value instanceof ApiError && value.status === 409) {
                  const details = value.details;
                  const existingJobId =
                    details &&
                    typeof details === "object" &&
                    details !== null &&
                    "existingJobId" in details &&
                    typeof (details as Record<string, unknown>)
                      .existingJobId === "string"
                      ? ((details as Record<string, unknown>)
                          .existingJobId as string)
                      : null;
                  if (existingJobId) setNoticeCheckJobId(existingJobId);
                  else setError(value.message);
                } else {
                  setError(
                    value instanceof Error ? value.message : String(value),
                  );
                }
              } finally {
                setBusy(false);
              }
            })();
          }}
        >
          <ScanSearch className="homepageTopActionIcon" aria-hidden="true" />
          <span className="homepageTopActionLabel">立即扫描</span>
        </Button>
      </>,
    );
  }, [busy, onTopActions, requestRefresh]);

  useManagementEventBatch(({ events, resyncRequired }) => {
    const refreshRequired = resyncRequired || events.some((event) =>
      ["jobs", "stacks", "services", "discovery"].includes(event.domain) ||
      event.summary.jobId === noticeCheckJobId,
    );
    if (!refreshRequired) return;
    void requestRefresh().catch((value: unknown) =>
      setError(value instanceof Error ? value.message : String(value)),
    );
  });

  const allCards = useMemo(() => {
    if (liveLoaded) return cards;
    if (cards.length > 0) return cards;
    return cachedCards;
  }, [cachedCards, cards, liveLoaded]);
  const filteredCards = useMemo(
    () => allCards.filter((card) => matchesSearch(card, search)),
    [allCards, search],
  );
  const groupedCards = useMemo(() => {
    const groups = new Map<string, HomepageNavCard[]>();
    for (const card of filteredCards) {
      const current = groups.get(card.groupName) ?? [];
      current.push(card);
      groups.set(card.groupName, current);
    }
    return Array.from(groups.entries())
      .map(([groupName, cards]) => ({
        groupName,
        cards: [...cards].sort((left, right) =>
          left.title.localeCompare(right.title),
        ),
      }))
      .sort((left, right) => left.groupName.localeCompare(right.groupName));
  }, [filteredCards]);
  const balancedCardColumns = useMemo(
    () => balanceHomepageGroups(groupedCards, homepageColumnCount),
    [groupedCards, homepageColumnCount],
  );
  const metricsByServiceId = useMemo(
    () => metricMap(resourceOverview),
    [resourceOverview],
  );
  const summary = useMemo(
    () => aggregateMetrics(resourceOverview?.services ?? []),
    [resourceOverview],
  );
  const hasCachedCardsInUse = allCards.some((card) => card.source === "snapshot");

  const topbarContent = useMemo(
    () => (
      <HomepageHeaderContent
        metricsLabel="资源摘要"
        summary={summary}
        searchDraft={searchDraft}
        searchOpen={headerSearchOpen}
        onSearchDraftChange={setSearchDraft}
        onApplySearch={applyHeaderSearch}
        onToggleSearch={() => setHeaderSearchOpen((value) => !value)}
        onCloseSearch={() => setHeaderSearchOpen(false)}
      />
    ),
    [applyHeaderSearch, headerSearchOpen, searchDraft, summary],
  );
  const sidebarNavContent = useMemo(
    () =>
      isAppDemoRuntime ? null : <HomepageSidebarClock now={now} />,
    [isAppDemoRuntime, now],
  );
  const mobileNavContent = useMemo(
    () => (
      <div className="homepageDrawerNavControls">
        <div className="homepageDrawerSearchSlot">
          <HomepageSearchForm
            searchDraft={searchDraft}
            onSearchDraftChange={setSearchDraft}
            onApplySearch={applySearch}
          />
        </div>
        <div className="homepageDrawerBottomSummary">
          <HomepageResourceMetrics metricsLabel="菜单资源摘要" summary={summary} />
          <HomepageClockBlock clockLabel="菜单当前时间" now={now} />
        </div>
      </div>
    ),
    [applySearch, now, searchDraft, summary],
  );

  useEffect(() => {
    onTopbarContent(topbarContent);
  }, [onTopbarContent, topbarContent]);

  useEffect(() => {
    onSidebarNavContent(sidebarNavContent);
  }, [onSidebarNavContent, sidebarNavContent]);

  useEffect(() => {
    onMobileNavContent(mobileNavContent);
  }, [mobileNavContent, onMobileNavContent]);

  useEffect(() => {
    return () => {
      onTopbarContent(null);
      onSidebarNavContent(null);
      onMobileNavContent(null);
    };
  }, [onMobileNavContent, onSidebarNavContent, onTopbarContent]);

  return (
    <div ref={pageRef} className="page homepageDashboardPage">
      <h1 className="srOnly">服务导航</h1>
      {isAppDemoRuntime ? (
        <HomepageFloatingToolPanel pageRef={pageRef} />
      ) : null}
      {hasCachedCardsInUse || resourceFromCache ? (
        <ReadonlySnapshotNotice
          tone={!isOnline ? "warn" : "info"}
          title={
            !isOnline
              ? "当前离线，显示已缓存的首页数据。"
              : "首页先显示已缓存数据，后台会继续刷新。"
          }
          detail="应用壳和首页只读导航已缓存；写操作与高时效数据仍以联网结果为准。"
          fetchedAt={persistedSnapshotFetchedAt}
          actionLabel="重试刷新"
          actionDisabled={!isOnline || busy}
          onAction={() => {
            void (async () => {
              setBusy(true);
              setError(null);
              try {
                await requestRefresh();
              } catch (value: unknown) {
                setError(
                  value instanceof Error ? value.message : String(value),
                );
              } finally {
                setBusy(false);
              }
            })();
          }}
        />
      ) : !hasCachedNavSnapshot && !liveLoaded && !isOnline ? (
        <ReadonlySnapshotNotice
          tone="bad"
          title="当前没有可用的离线首页数据。"
          detail="请恢复联网后重新加载应用。"
        />
      ) : null}
      <div className="homepageMobileNavModule" aria-label="导航页快捷栏">
        <HomepageTopStrip
          className="homepageTopStripMobile"
          metricsLabel="导航页资源摘要"
          clockLabel="导航页当前时间"
          summary={summary}
          now={now}
          showClock={false}
        />
      </div>

      <div className="homepageStatusLine">
        <span>
          {hasCachedCardsInUse
            ? "正在同步服务入口"
            : summary.activeCount > 0
            ? `${summary.activeCount} 个服务提供实时摘要`
            : resourceOverview?.enabled === false
              ? "资源监控已关闭"
              : resourceOverview && resourceFromCache
                ? "显示已缓存的资源样本"
                : refreshing
                  ? "正在加载资源样本"
                  : "等待资源样本"}
        </span>
        {resourceError ? (
          <span>
            {resourceOverview
              ? `首页导航刷新失败：${resourceError}`
              : `首页导航暂不可用：${resourceError}`}
          </span>
        ) : null}
      </div>

      {groupedCards.length === 0 && refreshing && !hasCachedNavSnapshot ? (
        <div className="homepageNavSkeleton" aria-label="正在加载服务入口">
          {Array.from({ length: 3 }).map((_, groupIndex) => (
            <section
              key={`homepage-skeleton-group-${groupIndex}`}
              className="homepageDashboardGroup homepageDashboardGroupSkeleton"
            >
              <div className="homepageDashboardGroupHeader">
                <span className="homepageSkeletonLine homepageSkeletonTitle" />
                <span className="homepageSkeletonPill" />
              </div>
              <div className="homepageDashboardStack">
                {Array.from({ length: groupIndex === 0 ? 2 : 1 }).map((__, cardIndex) => (
                  <div
                    key={`homepage-skeleton-card-${groupIndex}-${cardIndex}`}
                    className="homepageServiceCard homepageServiceCardSkeleton"
                  >
                    <div className="homepageServiceCardTop">
                      <span className="homepageServiceIcon homepageSkeletonBlock" />
                      <span className="homepageServiceCardIdentity">
                        <span className="homepageSkeletonLine" />
                        <span className="homepageSkeletonLine homepageSkeletonLineShort" />
                      </span>
                      <span className="homepageServiceDetailButton homepageSkeletonBlock" />
                    </div>
                    <div className="homepageServiceMetricsGrid">
                      {["CPU", "MEM", "RX", "TX"].map((label) => (
                        <CardMetric key={label} value="-" label={label} />
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>
      ) : groupedCards.length === 0 ? (
        <div className="homepageEmptyState">
          <Activity className="homepageEmptyIcon" aria-hidden="true" />
          <div>
            {refreshing
              ? "正在刷新服务入口。"
              : "当前搜索条件下没有可展示的服务入口。"}
          </div>
        </div>
      ) : (
        <div
          className="homepageDashboardGrid"
          data-column-count={balancedCardColumns.length}
        >
          {balancedCardColumns.map((column, columnIndex) => (
            <div
              key={`homepage-column-${columnIndex}`}
              className="homepageDashboardColumn"
            >
              {column.map((group) => (
                <section key={group.groupName} className="homepageDashboardGroup">
                  <div className="homepageDashboardGroupHeader">
                    <h2>{group.groupName}</h2>
                    <span>{group.cards.length}</span>
                  </div>
                  <div className="homepageDashboardStack">
                    {group.cards.map((card) => {
                      const metric = metricsByServiceId.get(card.serviceId);
                      const badge = serviceBadge(
                        card,
                        metric,
                        resourceOverview,
                        resourceError !== null || resourceOverview === null,
                      );
                      return (
                        <div
                          key={card.id}
                          className="homepageServiceCard"
                          role="link"
                          tabIndex={0}
                          onClick={() => openHomepageHref(card.href)}
                          onKeyDown={(event) => {
                            if (eventTargetIsNestedAction(event)) return;
                            if (event.key !== "Enter" && event.key !== " ") return;
                            event.preventDefault();
                            openHomepageHref(card.href);
                          }}
                        >
                          {card.status === "updatable" && !card.isDockrev && card.service ? (
                            <button
                              type="button"
                              className={`homepageServiceStateBadge homepageServiceStateBadge-${badge.tone} homepageServiceStateButton`}
                              aria-label={`更新 ${card.title}`}
                              onClick={(event) => {
                                event.stopPropagation();
                                setUpdateDialogCard(card);
                              }}
                            >
                              {badge.label}
                            </button>
                          ) : (
                            <span
                              className={`homepageServiceStateBadge homepageServiceStateBadge-${badge.tone}`}
                            >
                              {badge.label}
                            </span>
                          )}
                          <div className="homepageServiceCardTop">
                            <HomepageServiceIcon icon={card.icon} title={card.title} />
                            <div className="homepageServiceCardIdentity">
                              <div className="homepageServiceCardTitle">
                                {card.title}
                              </div>
                              <div className="muted homepageServiceCardDescription">
                                {card.description}
                              </div>
                            </div>
                            <button
                              type="button"
                              className="homepageServiceDetailButton"
                              aria-label={`查看 ${card.title} 服务详情`}
                              title="服务详情"
                              onClick={(event) => {
                                event.stopPropagation();
                                navigate({
                                  name: "service",
                                  stackId: card.stackId,
                                  serviceId: card.serviceId,
                                });
                              }}
                            >
                              <Info size={15} strokeWidth={2.2} aria-hidden="true" />
                            </button>
                          </div>

                          <div className="homepageServiceMetricsGrid">
                            <CardMetric
                              value={formatPercent(metric?.cpuPercent)}
                              label="CPU"
                            />
                            <CardMetric
                              value={formatBytes(metric?.memUsedBytes)}
                              label="MEM"
                            />
                            <CardMetric
                              value={formatRate(metric?.netRxRateBps)}
                              label="RX"
                            />
                            <CardMetric
                              value={formatRate(metric?.netTxRateBps)}
                              label="TX"
                            />
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </section>
              ))}
            </div>
          ))}
        </div>
      )}

      {error ? <div className="error">{error}</div> : null}
      {noticeCheckJobId ? (
        <div className="success">
          扫描任务 <Mono>{noticeCheckJobId}</Mono> ·{" "}
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => navigate({ name: "job", jobId: noticeCheckJobId })}
          >
            查看任务
          </Button>
        </div>
      ) : null}
      {busy ? <div className="muted">处理中…</div> : null}
      <Dialog
        open={updateDialogCard !== null}
        onOpenChange={(open) => {
          if (!open) setUpdateDialogCard(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              确认更新服务 {updateDialogCard?.title ?? ""}？
            </DialogTitle>
          </DialogHeader>
          {updateDialogCard?.service ? (
            <ServiceUpdateConfirmDetails
              service={updateDialogCard.service}
              status={updateDialogCard.status}
            />
          ) : null}
          <DialogFooter>
            <DialogClose asChild>
              <Button variant="ghost" disabled={busy}>
                取消
              </Button>
            </DialogClose>
            <Button
              variant="primary"
              disabled={busy || !updateDialogCard?.service?.candidate}
              onClick={() => {
                const service = updateDialogCard?.service;
                if (!updateDialogCard || !service?.candidate) return;
                const card = updateDialogCard;
                void (async () => {
                  setBusy(true);
                  setError(null);
                  setNoticeCheckJobId(null);
                  try {
                    const response = await triggerUpdate({
                      scope: "service",
                      stackId: card.stackId,
                      ...(await buildUpdateServiceTarget(service)),
                      mode: "apply",
                      allowArchMismatch: false,
                      backupMode: "inherit",
                    });
                    setUpdateDialogCard(null);
                    navigate({ name: "job", jobId: response.jobId });
                  } catch (value: unknown) {
                    setError(
                      value instanceof Error ? value.message : String(value),
                    );
                  } finally {
                    setBusy(false);
                  }
                })();
              }}
            >
              执行更新
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
