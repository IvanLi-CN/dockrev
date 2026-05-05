import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import {
  Activity,
  Clock3,
  Cpu,
  Download,
  Info,
  MemoryStick,
  RefreshCw,
  ScanSearch,
  Search,
  Upload,
} from "lucide-react";
import {
  ApiError,
  getJob,
  getServiceResourceUsageOverview,
  getStack,
  listStacks,
  triggerCheck,
  triggerUpdate,
  type Service,
  type ServiceResourceOverviewItem,
  type ServiceResourceOverviewResponse,
  type StackDetail,
  type StackListItem,
} from "../api";
import { HomepageServiceIcon } from "../components/HomepageServiceIcon";
import { ServiceUpdateConfirmDetails } from "../components/ServiceUpdateConfirmDetails";
import { navigate } from "../routes";
import { isDockrevImageRef } from "../runtimeConfig";
import { buildUpdateServiceTarget } from "../updateTargets";
import {
  Button,
  Dialog,
  DialogClose,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Mono,
} from "../ui";
import { serviceRowStatus, statusLabel, type RowStatus } from "../updateStatus";
import { usePageResumeRefresh } from "../usePageResumeRefresh";

const HOMEPAGE_COLUMN_BREAKPOINTS = [
  { query: "(max-width: 720px)", columns: 1 },
  { query: "(max-width: 1160px)", columns: 2 },
  { query: "(max-width: 1500px)", columns: 3 },
] as const;

type HomepageNavCard = {
  id: string;
  stackId: string;
  stackName: string;
  serviceId: string;
  serviceName: string;
  imageRef: string;
  groupName: string;
  title: string;
  description: string;
  href: string;
  icon: string | null;
  status: RowStatus;
  isDockrev: boolean;
  service: Service;
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

  return columns.map((column) => column.groups).filter((column) => column.length > 0);
}

function normalizeHomepageHref(
  value: string | null | undefined,
): string | null {
  const trimmed = (value ?? "").trim();
  if (!trimmed) return null;
  if (trimmed.startsWith("/") && !trimmed.startsWith("//")) return trimmed;
  try {
    const url = new URL(trimmed);
    if (url.protocol === "http:" || url.protocol === "https:")
      return url.toString();
  } catch {
    return null;
  }
  return null;
}

function toNavCards(
  stacks: StackListItem[],
  details: Record<string, StackDetail | undefined>,
): HomepageNavCard[] {
  const cards: HomepageNavCard[] = [];
  for (const stack of stacks) {
    const detail = details[stack.id];
    if (!detail) continue;
    for (const service of detail.services) {
      if (service.archived) continue;
      const homepageHref = normalizeHomepageHref(service.homepage?.href);
      if (!homepageHref) continue;
      const groupName = service.homepage?.group?.trim() || detail.name;
      const title = service.homepage?.name?.trim() || service.name;
      const description =
        service.homepage?.description?.trim() || service.image.ref;
      const status = serviceRowStatus(service);
      cards.push({
        id: service.id,
        stackId: stack.id,
        stackName: detail.name,
        serviceId: service.id,
        serviceName: service.name,
        imageRef: service.image.ref,
        groupName,
        title,
        description,
        href: homepageHref,
        icon: service.homepage?.icon ?? null,
        status,
        isDockrev: isDockrevImageRef(service.image.ref),
        service,
      });
    }
  }
  return cards;
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

function formatClock(date: Date): string {
  const pad2 = (value: number) => String(value).padStart(2, "0");
  return `${pad2(date.getHours())}:${pad2(date.getMinutes())}:${pad2(date.getSeconds())}`;
}

function formatGmtOffset(date: Date): string {
  const offsetMinutes = -date.getTimezoneOffset();
  const sign = offsetMinutes >= 0 ? "+" : "-";
  const abs = Math.abs(offsetMinutes);
  const hours = Math.trunc(abs / 60);
  const minutes = abs % 60;
  return minutes === 0
    ? `GMT${sign}${hours}`
    : `GMT${sign}${hours}:${String(minutes).padStart(2, "0")}`;
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
  if (metric?.sampledAt && metric.stale) return { label: "STALE", tone: "stale" };
  if (metric || overview) return { label: "NO DATA", tone: "muted" };
  return { label: "NO DATA", tone: "muted" };
}

function DashboardMetric(props: {
  icon: ReactNode;
  value: string;
  label: string;
}) {
  return (
    <div className="homepageTopMetric">
      <span className="homepageTopMetricIcon" aria-hidden="true">
        {props.icon}
      </span>
      <span className="homepageTopMetricValue">{props.value}</span>
      <span className="homepageTopMetricLabel">{props.label}</span>
    </div>
  );
}

function HomepageResourceMetrics(props: {
  className?: string;
  metricsLabel: string;
  summary: ReturnType<typeof aggregateMetrics>;
}) {
  return (
    <div
      className={props.className ? `homepageSystemMetrics ${props.className}` : "homepageSystemMetrics"}
      aria-label={props.metricsLabel}
    >
      <DashboardMetric
        icon={<Cpu />}
        value={formatPercent(props.summary.cpu)}
        label="CPU"
      />
      <DashboardMetric
        icon={<MemoryStick />}
        value={formatBytes(props.summary.memory)}
        label="MEM"
      />
      <DashboardMetric
        icon={<Download />}
        value={formatRate(props.summary.rx)}
        label="RX"
      />
      <DashboardMetric
        icon={<Upload />}
        value={formatRate(props.summary.tx)}
        label="TX"
      />
    </div>
  );
}

function HomepageSearchForm(props: {
  searchDraft: string;
  autoFocus?: boolean;
  onSearchDraftChange: (value: string) => void;
  onApplySearch: () => void;
  onEscape?: () => void;
}) {
  return (
    <form
      className="homepageOverviewSearchForm"
      onSubmit={(event) => {
        event.preventDefault();
        props.onApplySearch();
      }}
    >
      <div className="homepageOverviewSearchShell">
        <Input
          aria-label="搜索服务入口"
          autoFocus={props.autoFocus}
          className="input homepageOverviewSearchInput"
          name="overview-search"
          onKeyDown={(event) => {
            if (event.key === "Escape") props.onEscape?.();
          }}
          onChange={(event) => props.onSearchDraftChange(event.target.value)}
          placeholder="搜索服务入口..."
          type="search"
          value={props.searchDraft}
        />
      </div>
    </form>
  );
}

function HomepageClockBlock(props: {
  className?: string;
  clockLabel: string;
  now: Date;
}) {
  return (
    <div
      className={props.className ? `homepageClock ${props.className}` : "homepageClock"}
      aria-label={props.clockLabel}
    >
      <Clock3 className="homepageClockIcon" aria-hidden="true" />
      <span>{formatClock(props.now)}</span>
      <span className="homepageClockZone">{formatGmtOffset(props.now)}</span>
    </div>
  );
}

function HomepageTopStrip(props: {
  className?: string;
  metricsLabel: string;
  clockLabel: string;
  summary: ReturnType<typeof aggregateMetrics>;
  now: Date;
  showClock?: boolean;
}) {
  const className = props.className
    ? `homepageTopStrip ${props.className}`
    : "homepageTopStrip";

  return (
    <div className={className}>
      <HomepageResourceMetrics metricsLabel={props.metricsLabel} summary={props.summary} />
      {props.showClock === false ? null : (
        <HomepageClockBlock clockLabel={props.clockLabel} now={props.now} />
      )}
    </div>
  );
}

function HomepageHeaderContent(props: {
  metricsLabel: string;
  summary: ReturnType<typeof aggregateMetrics>;
  searchDraft: string;
  searchOpen: boolean;
  onSearchDraftChange: (value: string) => void;
  onApplySearch: () => void;
  onToggleSearch: () => void;
  onCloseSearch: () => void;
}) {
  return (
    <div className="homepageHeaderContent">
      <HomepageResourceMetrics
        className="homepageHeaderMetrics"
        metricsLabel={props.metricsLabel}
        summary={props.summary}
      />
      <div className="homepageHeaderSearch">
        <div className="homepageHeaderSearchDesktop">
          <HomepageSearchForm
            searchDraft={props.searchDraft}
            onSearchDraftChange={props.onSearchDraftChange}
            onApplySearch={props.onApplySearch}
          />
        </div>
        <button
          type="button"
          className="homepageHeaderSearchToggle"
          aria-label={props.searchOpen ? "关闭搜索" : "打开搜索"}
          aria-expanded={props.searchOpen}
          onClick={props.onToggleSearch}
        >
          <Search size={19} strokeWidth={2.3} aria-hidden="true" />
        </button>
        {props.searchOpen ? (
          <div className="homepageHeaderSearchPopover">
            <HomepageSearchForm
              autoFocus
              searchDraft={props.searchDraft}
              onSearchDraftChange={props.onSearchDraftChange}
              onApplySearch={props.onApplySearch}
              onEscape={props.onCloseSearch}
            />
          </div>
        ) : null}
      </div>
    </div>
  );
}

function HomepageSidebarClock(props: { now: Date }) {
  return (
    <div className="homepageSidebarClockPanel">
      <div className="homepageSidebarClockLabel">当前时间</div>
      <HomepageClockBlock
        className="homepageSidebarClock"
        clockLabel="侧边栏当前时间"
        now={props.now}
      />
    </div>
  );
}

function CardMetric(props: { value: string; label: string }) {
  return (
    <span className="homepageServiceMetric">
      <span className="homepageServiceMetricValue">{props.value}</span>
      <span className="homepageServiceMetricLabel">{props.label}</span>
    </span>
  );
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
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [resourceError, setResourceError] = useState<string | null>(null);
  const [noticeCheckJobId, setNoticeCheckJobId] = useState<string | null>(null);
  const [stacks, setStacks] = useState<StackListItem[]>([]);
  const [details, setDetails] = useState<
    Record<string, StackDetail | undefined>
  >({});
  const [resourceOverview, setResourceOverview] =
    useState<ServiceResourceOverviewResponse | null>(null);
  const [search, setSearch] = useState("");
  const [searchDraft, setSearchDraft] = useState("");
  const [headerSearchOpen, setHeaderSearchOpen] = useState(false);
  const [updateDialogCard, setUpdateDialogCard] =
    useState<HomepageNavCard | null>(null);
  const [now, setNow] = useState(() => new Date());
  const homepageColumnCount = useHomepageColumnCount();

  const applySearch = useCallback(() => {
    setSearch(searchDraft);
  }, [searchDraft]);
  const applyHeaderSearch = useCallback(() => {
    applySearch();
    setHeaderSearchOpen(false);
  }, [applySearch]);

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const refresh = useCallback(async () => {
    const [nextStacks, metricsResult] = await Promise.all([
      listStacks(),
      getServiceResourceUsageOverview("1h").then(
        (value) => ({ ok: true, value }) as const,
        (error: unknown) => ({ ok: false, error }) as const,
      ),
    ]);
    const maxLastScan = nextStacks
      .map((item) => item.lastCheckAt)
      .sort()
      .at(-1);
    onLastScanHint(maxLastScan);
    setStacks(nextStacks);

    if (metricsResult.ok) {
      setResourceOverview(metricsResult.value);
      setResourceError(null);
    } else {
      setResourceOverview(null);
      setResourceError(
        metricsResult.error instanceof Error
          ? metricsResult.error.message
          : String(metricsResult.error),
      );
    }

    const nextDetails = await Promise.all(
      nextStacks.map(async (stack) => {
        try {
          return [stack.id, await getStack(stack.id)] as const;
        } catch {
          return [stack.id, undefined] as const;
        }
      }),
    );
    setDetails(Object.fromEntries(nextDetails));
  }, [onLastScanHint]);

  const requestRefresh = usePageResumeRefresh(refresh, {
    onError: (value: unknown) =>
      setError(value instanceof Error ? value.message : String(value)),
  });

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

  useEffect(() => {
    if (!noticeCheckJobId) return;
    let closed = false;
    let timer: number | null = null;

    const poll = async () => {
      try {
        const job = await getJob(noticeCheckJobId);
        if (closed) return;
        if (job.status === "queued" || job.status === "running") {
          timer = window.setTimeout(() => {
            void poll();
          }, 1200);
          return;
        }
      } catch {
        if (closed) return;
      }

      try {
        await requestRefresh();
      } catch (value: unknown) {
        if (!closed)
          setError(value instanceof Error ? value.message : String(value));
      }
    };

    timer = window.setTimeout(() => {
      void poll();
    }, 1200);

    return () => {
      closed = true;
      if (timer != null) window.clearTimeout(timer);
    };
  }, [noticeCheckJobId, requestRefresh]);

  const allCards = useMemo(
    () => toNavCards(stacks, details),
    [details, stacks],
  );
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
    () => <HomepageSidebarClock now={now} />,
    [now],
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
    <div className="page homepageDashboardPage">
      <h1 className="srOnly">服务导航</h1>
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
          {summary.activeCount > 0
            ? `${summary.activeCount} 个服务提供实时摘要`
            : resourceOverview?.enabled === false
              ? "资源监控已关闭"
              : "等待资源样本"}
        </span>
        {resourceError ? <span>资源指标暂不可用：{resourceError}</span> : null}
      </div>

      {groupedCards.length === 0 ? (
        <div className="homepageEmptyState">
          <Activity className="homepageEmptyIcon" aria-hidden="true" />
          <div>当前搜索条件下没有可展示的服务入口。</div>
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
                          {card.status === "updatable" && !card.isDockrev ? (
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
          {updateDialogCard ? (
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
              disabled={busy || !updateDialogCard?.service.candidate}
              onClick={() => {
                if (!updateDialogCard?.service.candidate) return;
                const card = updateDialogCard;
                void (async () => {
                  setBusy(true);
                  setError(null);
                  setNoticeCheckJobId(null);
                  try {
                    const response = await triggerUpdate({
                      scope: "service",
                      stackId: card.stackId,
                      ...(await buildUpdateServiceTarget(card.service)),
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
