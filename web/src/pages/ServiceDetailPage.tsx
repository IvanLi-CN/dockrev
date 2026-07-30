import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Cpu, Download, HardDriveDownload, HardDriveUpload, MemoryStick, Upload } from "lucide-react";
import {
  createIgnore,
  deleteIgnore,
  getServiceResourceUsageHistory,
  inferServiceRepoLink,
  listJobs,
  listJobsPage,
  newJobsEventsSource,
  putServiceBackupTargets,
  putServiceSettings,
  type JobListItem,
  type ServiceBackupRecordItem,
  type ServiceBackupTargetsResponse,
  type ServiceResourceUsageWindow,
  type ServiceSettings,
  type StackDetail,
} from "../api";
import { computeMonitorTerminalRate, formatMonitorBytes, formatMonitorPercent, formatMonitorRate, isMonitorDisabledError } from "./serviceDetailMonitorHelpers";
import { BackupPolicySegmentedControl } from "../components/BackupPolicySegmentedControl";
import { BackupRecordList } from "../components/ServiceBackupRecords";
import { ReadonlySnapshotNotice } from "../components/ReadonlySnapshotNotice";
import { navigate } from "../routes";
import { Button, IconButton, Input, Mono, OverlayScrollArea, RefreshIcon, SelectField, Switch, Tabs, TabsList, TabsTrigger } from "../ui";
import { usePwaStatus } from "../pwaStatus";
import { buildReadonlySnapshotKey, readReadonlySnapshot, writeReadonlySnapshot } from "../readonlySnapshotCache";
import { serviceRowStatus } from "../updateStatus";
import { ServiceResourcePanel, type ServiceResourceSnapshot } from "../components/ServiceResourcePanel";
import { ServiceLogsPanel } from "../components/ServiceLogsPanel";
import { createDefaultAutoUpdatePolicy } from "../components/AutoUpdatePolicyEditor";
import { AutoUpdatePolicyDrawer } from "../components/AutoUpdatePolicyDrawer";
import { AutoUpdatePolicyResultCard } from "../components/AutoUpdatePolicyResultCard";
import { RecentUpdateRecords, ServiceOperationHistory, filterServiceOperationJobs, selectRecentServiceUpdateJobs, selectServiceOperationJobs } from "../components/RecentUpdateRecords";
import { ResponsiveSettingsDrawer } from "../components/ResponsiveSettingsDrawer";
import { ServiceVersionsSection } from "../components/ServiceVersionsSection";
import { ServiceStackDetailAction } from "../components/ServiceSplitActionButton";
import { ImageLinkIcons, RepositoryLinkIcon, splitImageNameForDisplay, splitImageRef } from "../imageLinks";
import { ServiceComposeTagField } from "./ServiceComposeTagField";
import {
  backupPolicyHint,
  backupRelationshipLabel,
  backupTargetRequestFromDraft,
  createBackupTargetsDraft,
  formatBackupRetentionSummary,
  isDockrevService,
  sanitizeReadonlyStackSnapshot,
  ServiceDetailReadonlyBlocked,
  type BackupTargetsDraft,
  type ServiceDetailSection,
} from "./serviceDetailPageHelpers";
import { useServiceDetailPageState } from "./useServiceDetailPageState";

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}

const SERVICE_DETAIL_SNAPSHOT_STALE_MS = 60_000;
const SERVICE_DETAIL_MONITORING_WINDOW: ServiceResourceUsageWindow = "1h";

type ServiceDetailSnapshotPayload = {
  stack: StackDetail;
  jobs: JobListItem[];
  historyCursor?: string | null;
  historyNextCursor?: string | null;
  historyCursorStack?: (string | null)[];
  backupTargets: ServiceBackupTargetsResponse | null;
  backupRecords: ServiceBackupRecordItem[];
  monitoring: ServiceResourceSnapshot | null;
};

export function ServiceDetailPage(props: {
  stackId: string;
  serviceId: string;
  section?: "overview" | "versions" | "history" | "monitoring" | "backup" | "logs" | "settings";
  onLastScanHint: (lastScan?: string) => void;
  onTopActions: (node: ReactNode) => void;
  onPageTitle?: (title: string) => void;
  onTopbarContent?: (node: ReactNode) => void;
}) {
  const { onPageTitle, onTopActions, onTopbarContent } = props;
  const section = props.section ?? "overview";
  const { isOnline } = usePwaStatus();
  const snapshotKey = buildReadonlySnapshotKey("service-detail", `${props.stackId}:${props.serviceId}`);
  const {
    anomalyCandidateTag,
    anomalyCurrentTag,
    bannerClass,
    bannerDetail,
    bannerTitle,
    backupRecords,
    busy,
    composeEnvFile,
    composeFiles,
    composeType,
    dockrevSelfUpgradeAction,
    dotClass,
    draftRepoUrl,
    error,
    lastSuccessfulRefreshAt,
    newRuleKind,
    newRuleNote,
    newRuleValue,
    notice,
    backupTargets,
    applyActiveJob,
    applySubmitting,
    repoInferBusy,
    requestRefresh,
    requestApplyUpdate,
    requestRollback,
    rollbackTarget,
    rollbackActiveJobId,
    rollbackActiveJobStatus,
    rollbackTargetRefreshing,
    rules,
    semverDowngradeAnomaly,
    service,
    serviceId,
    setBusy,
    setError,
    setNewRuleKind,
    setNewRuleNote,
    setNewRuleValue,
    setRepoInferBusy,
    settings,
    settingsBusy,
    stack,
    stackSettings,
    topActions,
    supervisorErrorAt,
    supervisorState,
    dangerousActions,
  } = useServiceDetailPageState(props);
  const [jobs, setJobs] = useState<JobListItem[]>([]);
  const [versionJobs, setVersionJobs] = useState<JobListItem[]>([]);
  const [versionJobsLoaded, setVersionJobsLoaded] = useState(false);
  const [historyCursor, setHistoryCursor] = useState<string | null>(null);
  const [historyNextCursor, setHistoryNextCursor] = useState<string | null>(null);
  const [historyCursorStack, setHistoryCursorStack] = useState<(string | null)[]>([]);
  const [historyPaginationBusy, setHistoryPaginationBusy] = useState(false);
  const historyCursorRef = useRef<string | null>(null);
  const currentServiceIdRef = useRef(props.serviceId);
  const historyRequestIdRef = useRef(0);
  const versionJobsRequestIdRef = useRef(0);
  currentServiceIdRef.current = props.serviceId;
  const [monitoringSnapshot, setMonitoringSnapshot] = useState<ServiceResourceSnapshot | null>(null);
  const [snapshotPayload, setSnapshotPayload] = useState<ServiceDetailSnapshotPayload | null>(null);
  const [, setSnapshotStatus] = useState<"missing" | "fresh" | "stale" | "expired" | "unsupported">("missing");
  const [snapshotFetchedAt, setSnapshotFetchedAt] = useState<string | null>(null);
  const [snapshotAnchorFetchedAt, setSnapshotAnchorFetchedAt] = useState<string | null>(null);
  const [snapshotActive, setSnapshotActive] = useState(false);
  const [settingsDrawerOpen, setSettingsDrawerOpen] = useState(false);
  const [tagDrawerOpen, setTagDrawerOpen] = useState(false);
  const [serviceSettingsDrawerOpen, setServiceSettingsDrawerOpen] = useState(false);
  const [backupSettingsDrawerOpen, setBackupSettingsDrawerOpen] = useState(false);
  const [autoPolicyDraft, setAutoPolicyDraft] = useState(() => createDefaultAutoUpdatePolicy("inherit"));
  const [serviceSettingsDraft, setServiceSettingsDraft] = useState<ServiceSettings | null>(null);
  const [serviceBackupTargetsDraft, setServiceBackupTargetsDraft] = useState<BackupTargetsDraft>(() => createBackupTargetsDraft(null));

  const refreshRecentJobs = useCallback(async (activateLive = false, cursor: string | null = historyCursorRef.current, nextCursorStack?: (string | null)[]) => {
    const requestedServiceId = props.serviceId;
    const requestId = ++historyRequestIdRef.current;
    const isPagination = nextCursorStack != null;
    if (isPagination) setHistoryPaginationBusy(true);
    const page = await listJobsPage({ serviceId: requestedServiceId, type: ["update", "rollback", "service_lifecycle"], limit: 20, cursor }).finally(() => {
      if (isPagination) setHistoryPaginationBusy(false);
    });
    if (requestId !== historyRequestIdRef.current || currentServiceIdRef.current !== requestedServiceId) return;
    setJobs(page.jobs);
    historyCursorRef.current = cursor;
    setHistoryCursor(cursor);
    setHistoryNextCursor(page.nextCursor ?? null);
    if (nextCursorStack != null) setHistoryCursorStack(nextCursorStack);
    if (!activateLive) return;
    setSnapshotActive(false);
    setSnapshotAnchorFetchedAt(null);
  }, [props.serviceId]);
  const refreshVersionJobs = useCallback(async () => {
    const requestedServiceId = props.serviceId;
    const requestId = ++versionJobsRequestIdRef.current;
    const versionHistory = await listJobs({ serviceId: requestedServiceId, type: ["update", "rollback", "service_lifecycle"] });
    if (requestId !== versionJobsRequestIdRef.current || currentServiceIdRef.current !== requestedServiceId) return;
    setVersionJobs(versionHistory);
    setVersionJobsLoaded(true);
  }, [props.serviceId]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const snapshot = await readReadonlySnapshot<ServiceDetailSnapshotPayload>(snapshotKey);
      if (cancelled) return;
      setSnapshotStatus(snapshot.status);
      setSnapshotFetchedAt(snapshot.record?.fetchedAt ?? null);
      setSnapshotAnchorFetchedAt(snapshot.record?.fetchedAt ?? null);
      if (snapshot.status !== "fresh") return;
      setSnapshotPayload(snapshot.record.payload);
      const snapshotHistoryCursor = snapshot.record.payload.historyCursor ?? null;
      historyCursorRef.current = snapshotHistoryCursor;
      setHistoryCursor(snapshotHistoryCursor);
      setHistoryNextCursor(snapshot.record.payload.historyNextCursor ?? null);
      setHistoryCursorStack(snapshot.record.payload.historyCursorStack ?? []);
      setMonitoringSnapshot(snapshot.record.payload.monitoring ?? null);
      setSnapshotActive(true);
    })();
    return () => {
      cancelled = true;
    };
  }, [snapshotKey]);

  useEffect(() => {
    historyRequestIdRef.current += 1;
    versionJobsRequestIdRef.current += 1;
    historyCursorRef.current = null;
    setJobs([]);
    setVersionJobs([]);
    setVersionJobsLoaded(false);
    setHistoryCursor(null);
    setHistoryNextCursor(null);
    setHistoryCursorStack([]);
    void refreshRecentJobs(false, null).catch(() => undefined);
  }, [props.serviceId, refreshRecentJobs]);

  useEffect(() => {
    void refreshVersionJobs().catch(() => undefined);
  }, [refreshVersionJobs]);

  useEffect(() => {
    if (!notice?.jobId) return;
    void refreshRecentJobs().catch(() => undefined);
    void refreshVersionJobs().catch(() => undefined);
  }, [notice?.jobId, refreshRecentJobs, refreshVersionJobs]);

  useEffect(() => {
    if (section !== "history" || !isOnline) return undefined;
    let closed = false;
    let source: EventSource | null = null;
    let errorStreak = 0;
    let lastEventId = 0;
    let refreshTimer: number | null = null;
    let pollTimer: number | null = null;
    let reconnectTimer: number | null = null;

    const clearRefreshTimer = () => {
      if (refreshTimer != null) window.clearTimeout(refreshTimer);
      refreshTimer = null;
    };
    const stopPolling = () => {
      if (pollTimer != null) window.clearInterval(pollTimer);
      pollTimer = null;
    };
    const scheduleRefresh = (delayMs: number) => {
      if (refreshTimer != null) return;
      refreshTimer = window.setTimeout(() => {
        refreshTimer = null;
        void refreshRecentJobs(true).catch(() => undefined);
      }, delayMs);
    };
    const startPolling = () => {
      if (pollTimer != null) return;
      pollTimer = window.setInterval(() => void refreshRecentJobs(true).catch(() => undefined), 10_000);
    };
    const connect = () => {
      if (closed) return;
      source = newJobsEventsSource(lastEventId > 0 ? { afterId: lastEventId } : undefined);
      source.addEventListener("open", () => {
        errorStreak = 0;
        stopPolling();
        scheduleRefresh(0);
      });
      source.addEventListener("job_event", (event: Event) => {
        const rawId = (event as MessageEvent).lastEventId;
        const parsedId = typeof rawId === "string" ? Number.parseInt(rawId, 10) : 0;
        if (Number.isFinite(parsedId) && parsedId > 0) lastEventId = parsedId;
        scheduleRefresh(250);
      });
      source.addEventListener("job_events_error", () => scheduleRefresh(0));
      source.onerror = () => {
        errorStreak += 1;
        scheduleRefresh(0);
        if (errorStreak < 3) return;
        source?.close();
        source = null;
        startPolling();
        if (reconnectTimer != null) return;
        reconnectTimer = window.setTimeout(() => {
          reconnectTimer = null;
          connect();
        }, 3_000);
      };
    };

    connect();
    return () => {
      closed = true;
      clearRefreshTimer();
      stopPolling();
      if (reconnectTimer != null) window.clearTimeout(reconnectTimer);
      source?.close();
    };
  }, [isOnline, refreshRecentJobs, section]);

  useEffect(() => {
    let cancelled = false;
    if (!isOnline) return undefined;
    void (async () => {
      try {
        const response = await getServiceResourceUsageHistory(props.serviceId, SERVICE_DETAIL_MONITORING_WINDOW);
        if (cancelled) return;
        setMonitoringSnapshot({
          fetchedAt: response.samples.length > 0 ? (response.samples[response.samples.length - 1]?.sampledAt ?? new Date().toISOString()) : new Date().toISOString(),
          windowKey: SERVICE_DETAIL_MONITORING_WINDOW,
          samples: response.samples,
          monitorDisabled: false,
        });
      } catch (error: unknown) {
        if (cancelled) return;
        if (isMonitorDisabledError(error)) {
          setMonitoringSnapshot({
            fetchedAt: new Date().toISOString(),
            windowKey: SERVICE_DETAIL_MONITORING_WINDOW,
            samples: [],
            monitorDisabled: true,
          });
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isOnline, props.serviceId]);

  useEffect(() => {
    if (!lastSuccessfulRefreshAt) return;
    setSnapshotActive(false);
    setSnapshotAnchorFetchedAt(null);
  }, [lastSuccessfulRefreshAt]);

  useEffect(() => {
    if (!stack || !service) return;
    void writeReadonlySnapshot(
      snapshotKey,
      {
        stack: sanitizeReadonlyStackSnapshot(stack),
        jobs,
        historyCursor,
        historyNextCursor,
        historyCursorStack,
        backupTargets,
        backupRecords,
        monitoring: monitoringSnapshot,
      },
      {
        staleAfterMs: SERVICE_DETAIL_SNAPSHOT_STALE_MS,
        fetchedAt: snapshotAnchorFetchedAt ? Date.parse(snapshotAnchorFetchedAt) || undefined : undefined,
      },
    );
  }, [backupRecords, backupTargets, historyCursor, historyCursorStack, historyNextCursor, jobs, monitoringSnapshot, service, snapshotAnchorFetchedAt, snapshotKey, stack]);

  const snapshotService = useMemo(() => snapshotPayload?.stack.services.find((item) => item.id === props.serviceId) ?? null, [props.serviceId, snapshotPayload]);
  const effectiveStack = stack ?? snapshotPayload?.stack ?? null;
  const effectiveService = service ?? snapshotService;
  const effectiveJobs = snapshotActive ? (snapshotPayload?.jobs ?? jobs) : jobs;
  const effectiveBackupTargets = snapshotActive ? (snapshotPayload?.backupTargets ?? backupTargets) : backupTargets;
  const effectiveBackupRecords = snapshotActive ? (snapshotPayload?.backupRecords ?? backupRecords) : backupRecords;
  const effectiveMonitoringSnapshot = snapshotActive ? (snapshotPayload?.monitoring ?? monitoringSnapshot) : monitoringSnapshot;
  const readonlyUi = !isOnline || snapshotActive;
  const latestMonitorSample =
    effectiveMonitoringSnapshot != null && effectiveMonitoringSnapshot.samples.length > 0
      ? effectiveMonitoringSnapshot.samples[effectiveMonitoringSnapshot.samples.length - 1] ?? null
      : null;
  const previousMonitorSample =
    effectiveMonitoringSnapshot != null && effectiveMonitoringSnapshot.samples.length > 1
      ? effectiveMonitoringSnapshot.samples[effectiveMonitoringSnapshot.samples.length - 2] ?? null
      : null;
  const latestMonitorDiskReadRate = computeMonitorTerminalRate(previousMonitorSample, latestMonitorSample, (sample) => sample.blockReadBytes);
  const latestMonitorDiskWriteRate = computeMonitorTerminalRate(previousMonitorSample, latestMonitorSample, (sample) => sample.blockWriteBytes);
  const latestMonitorRxRate = computeMonitorTerminalRate(previousMonitorSample, latestMonitorSample, (sample) => sample.netRxBytes);
  const latestMonitorTxRate = computeMonitorTerminalRate(previousMonitorSample, latestMonitorSample, (sample) => sample.netTxBytes);
  const monitorMetricGroups = useMemo(
    () => [
      {
        key: "compute",
        metrics: [
          { label: "CPU", value: formatMonitorPercent(latestMonitorSample?.cpuPercent ?? null), icon: <Cpu className="svcDetailMonitorGlyph" aria-hidden="true" /> },
          { label: "内存", value: formatMonitorBytes(latestMonitorSample?.memUsedBytes ?? null), icon: <MemoryStick className="svcDetailMonitorGlyph" aria-hidden="true" /> },
        ],
      },
      {
        key: "disk",
        metrics: [
          { label: "磁盘读", value: formatMonitorRate(latestMonitorDiskReadRate), icon: <HardDriveDownload className="svcDetailMonitorGlyph" aria-hidden="true" /> },
          { label: "磁盘写", value: formatMonitorRate(latestMonitorDiskWriteRate), icon: <HardDriveUpload className="svcDetailMonitorGlyph" aria-hidden="true" /> },
        ],
      },
      {
        key: "network",
        metrics: [
          { label: "下载", value: formatMonitorRate(latestMonitorRxRate), icon: <Download className="svcDetailMonitorGlyph" aria-hidden="true" /> },
          { label: "上传", value: formatMonitorRate(latestMonitorTxRate), icon: <Upload className="svcDetailMonitorGlyph" aria-hidden="true" /> },
        ],
      },
    ],
    [
      latestMonitorDiskReadRate,
      latestMonitorDiskWriteRate,
      latestMonitorRxRate,
      latestMonitorSample?.cpuPercent,
      latestMonitorSample?.memUsedBytes,
      latestMonitorTxRate,
    ],
  );
  const topbarMonitorSummary = useMemo(() => {
    if (!effectiveStack || !effectiveService) return null;

    return (
      <div className="topbarServiceMonitorSummary" data-service-detail-context="monitor-summary" aria-label="服务监控指标">
        {monitorMetricGroups.map((group) => (
          <div key={group.key} className="topbarServiceMonitorGroup" data-monitor-group={group.key}>
            {group.metrics.map((metric) => (
              <div
                key={metric.label}
                className="topbarServiceMonitorMetric"
                data-monitor-metric={metric.label}
                aria-label={`${metric.label} ${metric.value}`}
                title={`${metric.label} ${metric.value}`}
              >
                <span className="topbarServiceMonitorMetricIcon" aria-hidden="true">
                  {metric.icon}
                </span>
                <span className="topbarServiceMonitorMetricValue">{metric.value}</span>
              </div>
            ))}
          </div>
        ))}
      </div>
    );
  }, [effectiveService, effectiveStack, monitorMetricGroups]);

  useEffect(() => {
    onPageTitle?.(effectiveService?.name ?? "");
    return () => onPageTitle?.("");
  }, [effectiveService?.name, onPageTitle]);

  useEffect(() => {
    onTopbarContent?.(topbarMonitorSummary);
    return () => onTopbarContent?.(null);
  }, [onTopbarContent, topbarMonitorSummary]);

  useEffect(() => {
    if (readonlyUi) {
      onTopActions(
        <>
          <ServiceStackDetailAction disabled={busy} onClick={() => navigate({ name: "stack", stackId: props.stackId })} />
          <Button disabled={busy || !isOnline} onClick={() => void requestRefresh()}>
            刷新
          </Button>
        </>,
      );
      return () => onTopActions(null);
    }
    onTopActions(topActions);
    return () => onTopActions(null);
  }, [busy, isOnline, onTopActions, props.stackId, readonlyUi, requestRefresh, topActions]);

  if (!effectiveStack || !effectiveService) {
    if (!isOnline) {
      return (
        <div className="page">
          <ReadonlySnapshotNotice tone="bad" title="当前没有可用的离线服务详情数据。" detail="请恢复联网后重新加载该页面。" />
        </div>
      );
    }
    return <div className="muted">加载中…</div>;
  }

  const policy = settings?.autoUpdatePolicy ?? stackSettings?.autoUpdatePolicy ?? createDefaultAutoUpdatePolicy("inherit");
  const serviceProtectionDraft = serviceSettingsDraft ?? settings ?? effectiveService.settings;
  const visibleRepoUrl = serviceSettingsDrawerOpen ? serviceProtectionDraft.repoUrl : draftRepoUrl;
  const recentUpdateJobs = selectRecentServiceUpdateJobs(snapshotActive || !versionJobsLoaded ? effectiveJobs : versionJobs, effectiveService.id);
  const serviceOperationJobs = filterServiceOperationJobs(effectiveJobs, effectiveService.id, effectiveStack.id);
  const versionOperationJobs = selectServiceOperationJobs(versionJobs, effectiveService.id, effectiveStack.id);
  const sectionValue = section;
  const effectiveBannerTitle =
    service != null
      ? bannerTitle
      : serviceRowStatus(effectiveService) === "blocked"
        ? "已阻止（忽略规则命中）"
        : serviceRowStatus(effectiveService) === "archMismatch"
          ? "架构不匹配（仅提示，不允许更新）"
          : serviceRowStatus(effectiveService) === "hint"
            ? "需确认（arch 未知）"
            : serviceRowStatus(effectiveService) === "updatable"
              ? "可更新"
              : "暂无候选版本";
  const effectiveBannerClass = service != null ? bannerClass : "svcBanner svcBannerMuted";
  const effectiveDotClass = service != null ? dotClass : "svcBannerDot";
  const effectiveBannerDetail = service != null ? bannerDetail : "当前展示本地快照；恢复联网后刷新可获取最新候选与实时状态。";
  const imageNameDisplay = splitImageNameForDisplay(splitImageRef(effectiveService.image.ref).name, effectiveService.image.tag);
  const renderOverviewSection = () => (
    <div className="svcDetailSectionStack">
      <RecentUpdateRecords jobs={recentUpdateJobs} />
      <div className="card serviceDetailIdentifiersCard" data-service-detail-section-card="service-identifiers">
        <div className="serviceDetailIdentifiersHead">
          <div>
            <div className="title">服务标识</div>
            <div className="muted">镜像引用与内部标识只在概览保留，避免其余子页重复占用页头空间。</div>
          </div>
        </div>
        <div className="serviceDetailIdentifiersGrid">
          <div className="serviceDetailIdentifierItem">
            <div className="serviceDetailIdentifierLabel">Image Ref</div>
            <div className="serviceDetailIdentifierValue">
              <Mono>{effectiveService.image.ref}</Mono>
            </div>
          </div>
          <div className="serviceDetailIdentifierItem">
            <div className="serviceDetailIdentifierLabel">Service ID</div>
            <div className="serviceDetailIdentifierValue">
              <Mono>{effectiveService.id}</Mono>
            </div>
          </div>
          <div className="serviceDetailIdentifierItem">
            <div className="serviceDetailIdentifierLabel">Stack ID</div>
            <div className="serviceDetailIdentifierValue">
              <Mono>{effectiveStack.id}</Mono>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
  const renderHistorySection = () => (
    <div className="svcDetailSectionStack">
      <ServiceOperationHistory
        backupRecords={effectiveBackupRecords}
        key={serviceId}
        jobs={serviceOperationJobs}
        serviceId={serviceId}
        onRollback={readonlyUi ? undefined : requestRollback}
        rollbackBusy={busy || rollbackTargetRefreshing}
        rollbackSourceJobId={readonlyUi || !rollbackTarget?.available ? null : rollbackTarget.sourceUpdateJobId}
        page={historyCursorStack.length + 1}
        hasPrevious={historyCursorStack.length > 0}
        hasNext={Boolean(historyNextCursor)}
        paginationDisabled={readonlyUi || historyPaginationBusy}
        onPrevious={() => {
          const previous = historyCursorStack[historyCursorStack.length - 1] ?? null;
          void refreshRecentJobs(false, previous, historyCursorStack.slice(0, -1));
        }}
        onNext={() => {
          if (!historyNextCursor) return;
          void refreshRecentJobs(false, historyNextCursor, [...historyCursorStack, historyCursor]);
        }}
      />
    </div>
  );
  const renderVersionsSection = () => (
    <div className="svcDetailSectionStack">
      {readonlyUi ? (
        <ServiceDetailReadonlyBlocked
          detail="版本页需要联网拉取统一 release notes 数据；恢复联网后才能定位当前部署版本并读取完整正文。"
          title="当前离线，版本页需要联网。"
        />
      ) : (
        <ServiceVersionsSection
          backupRecords={effectiveBackupRecords}
          busy={busy}
          dockrevSelfUpgradeAction={dockrevSelfUpgradeAction}
          jobs={versionOperationJobs}
          onApplyUpdate={requestApplyUpdate}
          onRollback={requestRollback}
          rollbackActiveJobId={rollbackActiveJobId}
          rollbackActiveJobStatus={rollbackActiveJobStatus}
          rollbackTarget={rollbackTarget}
          rollbackTargetRefreshing={rollbackTargetRefreshing}
          service={effectiveService}
          updateActiveJob={applyActiveJob}
          updateSubmitting={applySubmitting}
        />
      )}
    </div>
  );

  const renderMonitoringSection = () => (
    <div className="svcDetailSectionStack">
      <ServiceResourcePanel initialSnapshot={readonlyUi ? (monitoringSnapshot ?? snapshotPayload?.monitoring ?? null) : undefined} readonly={readonlyUi} serviceId={effectiveService.id} />
    </div>
  );

  const renderBackupSection = () => (
    <div className="svcDetailSectionStack">
      <div className="card serviceBackupSummaryCard" data-service-detail-section-card="backup-summary">
        <div className="serviceBackupSummaryHead">
          <div>
            <div className="title">备份设置</div>
            <div className="muted">当前服务的备份策略、存储位置与默认保留摘要。</div>
          </div>
          <div data-service-detail-action="open-backup-settings">
            <Button
              disabled={settingsBusy || readonlyUi}
              onClick={() => {
                setServiceBackupTargetsDraft(createBackupTargetsDraft(effectiveBackupTargets));
                setBackupSettingsDrawerOpen(true);
              }}
            >
              编辑备份设置
            </Button>
          </div>
        </div>
        <div className="serviceBackupMetaCard">
          {effectiveBackupTargets?.storage ? (
            <>
              <div className="serviceBackupMetaSummary">{formatBackupRetentionSummary(effectiveBackupTargets.storage)}</div>
              <div className="serviceBackupMetaGrid">
                <div>
                  <div className="muted">目录</div>
                  <div className="mono">{effectiveBackupTargets.storage.baseDir}</div>
                </div>
                <div>
                  <div className="muted">产物</div>
                  <div className="mono">{effectiveBackupTargets.storage.artifactPattern}</div>
                </div>
                <div>
                  <div className="muted">压缩</div>
                  <div className="mono">{effectiveBackupTargets.storage.compression}</div>
                </div>
              </div>
            </>
          ) : (
            <div className="muted">加载备份说明中…</div>
          )}
        </div>
      </div>

      <div className="card" data-service-detail-section-card="backup-records">
        <div className="serviceBackupSummaryHead">
          <div>
            <div className="title">实际备份记录</div>
            <div className="muted">这里只显示当前服务实际产生过备份产物的记录。</div>
          </div>
        </div>
        <BackupRecordList records={effectiveBackupRecords} />
      </div>
    </div>
  );

  const renderLogsSection = () => (
    <div className="svcDetailSectionStack">
      {readonlyUi ? <ServiceDetailReadonlyBlocked detail="日志流不做持久化。恢复联网后才能重新建立实时日志连接。" title="当前离线，日志页需要联网。" /> : <ServiceLogsPanel serviceId={effectiveService.id} />}
    </div>
  );

  const renderSettingsSection = () => (
    <div className="svcDetailSectionStack">
      {readonlyUi || !settings ? <ServiceDetailReadonlyBlocked detail="设置页包含敏感配置与写操作，不会持久化到本地；恢复联网后才可编辑。" title="当前离线，设置页需要联网。" /> : null}
      {!readonlyUi && settings ? (
        <>
          <div data-service-detail-section-card="auto-policy">
            <div data-service-detail-action="open-auto-policy">
              <AutoUpdatePolicyResultCard
                busy={settingsBusy}
                onOpenSettings={() => {
                  setAutoPolicyDraft(policy);
                  setSettingsDrawerOpen(true);
                }}
                policy={policy}
                scope="service"
                stackPolicy={stackSettings?.autoUpdatePolicy ?? null}
              />
            </div>
          </div>
          <div className="card svcComposeCard">
            <div className="title">Compose 信息</div>
            <div className="kv">
              <div className="kvRow">
                <div className="muted">type</div>
                <div className="mono">{composeType}</div>
              </div>
              <div className="kvRow">
                <div className="muted">compose files</div>
                {composeFiles.length > 0 ? (
                  <div>
                    {composeFiles.map((item, index) => (
                      <div key={`${item}-${index}`} className="mono">
                        {item}
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="mono">-</div>
                )}
              </div>
              <div className="kvRow">
                <div className="muted">env file</div>
                <div className="mono">{composeEnvFile}</div>
              </div>
            </div>
          </div>

          <div className="card serviceSafeguardCard">
            <div>
              <div className="title">部署 tag</div>
              <div className="muted">直接写回原始 Compose 文件里的镜像 tag，不自动执行 compose up。</div>
            </div>
            <div className="serviceTagCardActions">
              <div className="chipStatic">
                当前 <Mono>{effectiveService.image.tag || "-"}</Mono>
              </div>
              <Button disabled={settingsBusy} onClick={() => setTagDrawerOpen(true)}>
                编辑 tag
              </Button>
            </div>
          </div>

          <div className="card serviceSafeguardCard">
            <div>
              <div className="title">服务保护设置</div>
              <div className="muted">失败回滚与代码仓库单独配置；备份目标已经迁到独立的备份分区。</div>
            </div>
            <div data-service-detail-action="open-service-settings">
              <Button
                disabled={settingsBusy}
                onClick={() => {
                  setServiceSettingsDraft(settings);
                  setServiceSettingsDrawerOpen(true);
                }}
              >
                打开
              </Button>
            </div>
          </div>

          <div className="card" data-service-detail-section-card="ignore-rules">
            <div className="title">忽略规则</div>

            <div className="ruleList">
              {rules.map((r) => (
                <div key={r.id} className="ruleRow" style={{ display: "flex", gap: 12, alignItems: "flex-start" }}>
                  <div style={{ flex: 1 }}>
                    <div className="mono">
                      {r.match.kind}={r.match.value}
                    </div>
                    <div className="muted">
                      id <Mono>{r.id}</Mono> · enabled <Mono>{String(r.enabled)}</Mono>
                      {r.note ? (
                        <>
                          {" "}
                          · note <Mono>{r.note}</Mono>
                        </>
                      ) : null}
                    </div>
                  </div>
                  <Button
                    variant="ghost"
                    disabled={busy}
                    onClick={() => {
                      void (async () => {
                        setBusy(true);
                        setError(null);
                        try {
                          await deleteIgnore(r.id);
                          await requestRefresh();
                        } catch (e: unknown) {
                          setError(errorMessage(e));
                        } finally {
                          setBusy(false);
                        }
                      })();
                    }}
                  >
                    删除
                  </Button>
                </div>
              ))}
              {rules.length === 0 ? <div className="muted">暂无规则</div> : null}
            </div>
            <div className="sectionTitle" style={{ marginTop: 14 }}>
              添加规则
            </div>
            <div className="formGrid">
              <label className="formField">
                <span className="label">Kind</span>
                <SelectField
                  className="input"
                  onChange={(value) => setNewRuleKind(value as "exact" | "prefix" | "regex" | "semver")}
                  options={[
                    { value: "exact", label: "exact" },
                    { value: "prefix", label: "prefix" },
                    { value: "regex", label: "regex" },
                    { value: "semver", label: "semver" },
                  ]}
                  value={newRuleKind}
                />
              </label>
              <label className="formField formSpan2">
                <span className="label">Value</span>
                <Input className="input" onChange={(e) => setNewRuleValue(e.target.value)} value={newRuleValue} />
              </label>
              <label className="formField formSpan2">
                <span className="label">Note</span>
                <Input className="input" onChange={(e) => setNewRuleNote(e.target.value)} value={newRuleNote} />
              </label>
              <div className="formActions formSpan2">
                <Button
                  variant="primary"
                  disabled={busy}
                  onClick={() => {
                    void (async () => {
                      setBusy(true);
                      setError(null);
                      try {
                        await createIgnore({
                          enabled: true,
                          serviceId,
                          kind: newRuleKind,
                          value: newRuleValue,
                          note: newRuleNote,
                        });
                        await requestRefresh();
                      } catch (e: unknown) {
                        setError(errorMessage(e));
                      } finally {
                        setBusy(false);
                      }
                    })();
                  }}
                >
                  添加
                </Button>
              </div>
            </div>
          </div>

          <div className="card" data-service-detail-section-card="webhook">
            <div className="title">Webhook 触发（服务级）</div>
            <div className="muted">用于外部系统触发：更新此服务 / 更新 compose / 更新全部</div>
            <div className="webhookRow">
              <div className="label">POST</div>
              <div className="mono">/api/v1/update/service/{effectiveService.name}</div>
              <div style={{ marginLeft: "auto" }} className="chipStatic">
                需要鉴权
              </div>
            </div>
            <div className="webhookBody">
              <div className="label">Body（可选）</div>
              <div className="mono">{`{ "dryRun": true, "backup": "inherit" }`}</div>
              <div className="muted">dryRun=仅预览；backup=inherit/on/off；rollback=inherit/on/off</div>
            </div>
          </div>

          <div className="card svcDangerZoneCard" data-service-detail-section-card="danger-zone">
            <div className="svcDangerZoneHead">
              <div>
                <div className="title">维护动作</div>
                <div className="muted">低频或高影响动作下沉到设置页，避免服务详情首屏过于拥挤。</div>
              </div>
            </div>
            <div className="svcDangerZoneActions">{dangerousActions}</div>
          </div>
        </>
      ) : null}
    </div>
  );

  const renderSection = () => {
    if (sectionValue === "versions") return renderVersionsSection();
    if (sectionValue === "history") return renderHistorySection();
    if (sectionValue === "monitoring") return renderMonitoringSection();
    if (sectionValue === "backup") return renderBackupSection();
    if (sectionValue === "logs") return renderLogsSection();
    if (sectionValue === "settings") return renderSettingsSection();
    return renderOverviewSection();
  };

  return (
    <div className="page">
      {snapshotActive ? (
        <ReadonlySnapshotNotice
          tone={!isOnline ? "warn" : "info"}
          title={!isOnline ? "当前离线，显示已缓存的服务详情数据。" : "先显示已缓存的服务详情数据，后台会继续刷新。"}
          detail="仅保留概览、更新记录、监控摘要与备份摘要；版本、日志和设置会继续要求联网。"
          fetchedAt={snapshotFetchedAt}
          actionLabel="重试刷新"
          actionDisabled={!isOnline || busy}
          onAction={() => void requestRefresh()}
        />
      ) : !isOnline ? (
        <ReadonlySnapshotNotice tone="warn" title="当前离线。" detail="仅在存在可用缓存时显示只读内容；日志与设置需要联网。" />
      ) : null}
      <section className="detailHeroShell">
        <div className={`${effectiveBannerClass} svcDetailSummaryRail`} data-service-detail-context="status-summary">
          <div className="svcDetailSummaryLead">
            <div
              className="mono monoPrimary monoSplit imageLinkRow svcDetailSummaryImage"
              title={imageNameDisplay.suffix ? `${imageNameDisplay.base}${imageNameDisplay.suffix}` : imageNameDisplay.base}
            >
              <span className="monoSplitBase">{imageNameDisplay.base}</span>
              <ImageLinkIcons imageRef={effectiveService.image.ref} repoUrl={visibleRepoUrl} />
            </div>
          </div>
          <div className="svcDetailSummaryStatus">
            <span className={effectiveDotClass} />
            <div className="svcBannerTitle">{effectiveBannerTitle}</div>
            <div className="svcBannerDetail svcDetailStatusSummary">{effectiveBannerDetail}</div>
          </div>
        </div>

        <OverlayScrollArea className="svcDetailTabsShell" data-service-detail-tabs-shell="true" options={{ overflow: { x: "scroll", y: "hidden" } }}>
          <Tabs
            onValueChange={(value) => {
              const nextSection = value as ServiceDetailSection;
              navigate({
                name: "service",
                stackId: props.stackId,
                serviceId: props.serviceId,
                section: nextSection,
              });
            }}
            value={sectionValue}
          >
            <TabsList className="svcDetailTabsList" aria-label="服务详情分区">
              <TabsTrigger className={sectionValue === "overview" ? "svcDetailTab active" : "svcDetailTab"} data-service-detail-tab="overview" value="overview">
                概览
              </TabsTrigger>
              <TabsTrigger className={sectionValue === "versions" ? "svcDetailTab active" : "svcDetailTab"} data-service-detail-tab="versions" value="versions">
                版本
              </TabsTrigger>
              <TabsTrigger className={sectionValue === "history" ? "svcDetailTab active" : "svcDetailTab"} data-service-detail-tab="history" value="history">
                更新记录
              </TabsTrigger>
              <TabsTrigger className={sectionValue === "monitoring" ? "svcDetailTab active" : "svcDetailTab"} data-service-detail-tab="monitoring" value="monitoring">
                监控
              </TabsTrigger>
              <TabsTrigger className={sectionValue === "logs" ? "svcDetailTab active" : "svcDetailTab"} data-service-detail-tab="logs" value="logs">
                日志
              </TabsTrigger>
              <TabsTrigger className={sectionValue === "backup" ? "svcDetailTab active" : "svcDetailTab"} data-service-detail-tab="backup" value="backup">
                备份
              </TabsTrigger>
              <TabsTrigger className={sectionValue === "settings" ? "svcDetailTab active" : "svcDetailTab"} data-service-detail-tab="settings" value="settings">
                设置
              </TabsTrigger>
            </TabsList>
          </Tabs>
        </OverlayScrollArea>
      </section>

      {semverDowngradeAnomaly ? (
        <div className="svcAnomalyAlert" role="alert">
          <div className="svcAnomalyAlertTitle">
            <span className="svcAnomalyAlertIcon" aria-hidden="true">
              ⚠
            </span>
            <span>版本异常：候选版本低于当前版本</span>
          </div>
          <div className="svcAnomalyAlertText">
            当前 <Mono>{anomalyCurrentTag}</Mono> → 候选 <Mono>{anomalyCandidateTag}</Mono>。手动更新仍可继续，请确认这是预期降级。
          </div>
        </div>
      ) : null}

      {isDockrevService(effectiveService) && supervisorState.status === "offline" ? (
        <div className="muted" style={{ marginTop: 10 }}>
          supervisor offline · {supervisorErrorAt ?? "-"}
        </div>
      ) : null}

      {renderSection()}

      {settings ? (
        <AutoUpdatePolicyDrawer
          busy={settingsBusy}
          onChange={setAutoPolicyDraft}
          onOpenChange={setSettingsDrawerOpen}
          onSave={() => {
            void (async () => {
              setBusy(true);
              setError(null);
              try {
                await putServiceSettings(props.serviceId, {
                  ...settings,
                  autoUpdatePolicy: autoPolicyDraft,
                  repoUrl: undefined,
                });
                await requestRefresh();
              } catch (e: unknown) {
                setError(errorMessage(e));
              } finally {
                setBusy(false);
              }
            })();
          }}
          open={settingsDrawerOpen}
          policy={autoPolicyDraft}
          previewServiceId={effectiveService.id}
          scope="service"
          stackPolicy={stackSettings?.autoUpdatePolicy ?? null}
        />
      ) : null}

      <ResponsiveSettingsDrawer description="写回原始 Compose 文件里的镜像 tag；保存后不会自动执行 compose up。" onOpenChange={setTagDrawerOpen} open={tagDrawerOpen} title="部署 tag">
        <div className="settingsDrawerSection">
          <ServiceComposeTagField busy={settingsBusy} currentTag={effectiveService.image.tag} onError={setError} onSaved={requestRefresh} serviceId={props.serviceId} />
        </div>
      </ResponsiveSettingsDrawer>

      <ResponsiveSettingsDrawer
        description="配置失败回滚与代码仓库。"
        onOpenChange={(open) => {
          setServiceSettingsDrawerOpen(open);
          if (!open) {
            setServiceSettingsDraft(null);
          }
        }}
        open={serviceSettingsDrawerOpen}
        title="服务保护设置"
      >
        <div className="settingsDrawerSection">
          <div className="title">更新前备份 / 回滚</div>
          <div className="muted">服务级策略（失败回滚 + 备份 targets 三态选择）</div>

          <div className="kv">
            <div className="kvRow">
              <div className="label">失败回滚（autoRollback）</div>
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <Switch checked={serviceProtectionDraft.autoRollback} disabled={settingsBusy} onChange={(autoRollback) => setServiceSettingsDraft({ ...serviceProtectionDraft, autoRollback })} />
                <div className="muted">{serviceProtectionDraft.autoRollback ? "on" : "off"}</div>
              </div>
            </div>
            <div className="kvRow">
              <div className="label">代码仓库</div>
              <div>
                <div className="serviceRepoField">
                  <Input
                    className="input"
                    disabled={settingsBusy}
                    onChange={(e) => setServiceSettingsDraft({ ...serviceProtectionDraft, repoUrl: e.target.value })}
                    placeholder="https://github.com/owner/repo"
                    value={serviceProtectionDraft.repoUrl ?? ""}
                  />
                  <RepositoryLinkIcon repoUrl={serviceProtectionDraft.repoUrl ?? draftRepoUrl} />
                  <IconButton
                    disabled={settingsBusy}
                    hint={repoInferBusy ? "正在重新推断代码仓库…" : "根据镜像 OCI source / GHCR 重新推断"}
                    onClick={() => {
                      void (async () => {
                        setRepoInferBusy(true);
                        setError(null);
                        try {
                          const result = await inferServiceRepoLink(props.serviceId);
                          if (result.repoUrl) {
                            setServiceSettingsDraft({ ...serviceProtectionDraft, repoUrl: result.repoUrl });
                          } else {
                            setError(result.reason?.trim() || "未识别到代码仓库入口");
                          }
                        } catch (e: unknown) {
                          setError(errorMessage(e));
                        } finally {
                          setRepoInferBusy(false);
                        }
                      })();
                    }}
                    title="重新推断代码仓库"
                  >
                    <RefreshIcon className={repoInferBusy ? "inlineIcon inlineIconLoading" : "inlineIcon"} />
                  </IconButton>
                </div>
                <div className="muted">清空并保存会禁用后续自动补齐；再次手动推断并保存可恢复。</div>
              </div>
            </div>
          </div>

          <div className="formActions">
            <Button
              variant="primary"
              disabled={settingsBusy}
              onClick={() => {
                void (async () => {
                  setBusy(true);
                  setError(null);
                  try {
                    await putServiceSettings(props.serviceId, {
                      ...serviceProtectionDraft,
                      autoUpdatePolicy: settings?.autoUpdatePolicy,
                      repoUrl: (serviceProtectionDraft.repoUrl ?? "").trim() || null,
                    });
                    await requestRefresh();
                    setServiceSettingsDrawerOpen(false);
                    setServiceSettingsDraft(null);
                  } catch (e: unknown) {
                    setError(errorMessage(e));
                  } finally {
                    setBusy(false);
                  }
                })();
              }}
            >
              保存服务保护设置
            </Button>
          </div>
        </div>
      </ResponsiveSettingsDrawer>

      <ResponsiveSettingsDrawer description="配置当前服务升级前的备份 targets 与默认存储说明。" onOpenChange={setBackupSettingsDrawerOpen} open={backupSettingsDrawerOpen} title="备份设置">
        <div className="settingsDrawerSection">
          <div className="sectionTitle">备份项（服务级）</div>
          <div className="muted">每个 target 单独选择一个策略；数字表示关联服务数，停机备份会一起协调这些服务。</div>

          <div className="serviceBackupPicker">
            <div className="serviceBackupMetaCard">
              <div className="label">备份说明</div>
              {effectiveBackupTargets?.storage ? (
                <>
                  <div className="serviceBackupMetaSummary">{formatBackupRetentionSummary(effectiveBackupTargets.storage)}</div>
                  <div className="serviceBackupMetaGrid">
                    <div>
                      <div className="muted">目录</div>
                      <div className="mono">{effectiveBackupTargets.storage.baseDir}</div>
                    </div>
                    <div>
                      <div className="muted">产物</div>
                      <div className="mono">{effectiveBackupTargets.storage.artifactPattern}</div>
                    </div>
                    <div>
                      <div className="muted">压缩</div>
                      <div className="mono">{effectiveBackupTargets.storage.compression}</div>
                    </div>
                  </div>
                </>
              ) : (
                <div className="muted">加载备份说明中…</div>
              )}
            </div>

            {serviceBackupTargetsDraft.volumeNames.length === 0 && serviceBackupTargetsDraft.bindPaths.length === 0 ? (
              <div className="serviceBackupEmptyState">当前服务在 Compose 中未发现可备份 volume 或 bind path。</div>
            ) : (
              <>
                <div className="serviceBackupGroup">
                  <div className="label">Volumes</div>
                  {serviceBackupTargetsDraft.volumeNames.length === 0 ? <div className="muted">当前服务未声明可备份 volume。</div> : null}
                  {serviceBackupTargetsDraft.volumeNames.map((item) => (
                    <div key={item.key} className="serviceBackupRow">
                      <div className="serviceBackupRowHead">
                        <div>
                          <div className="mono">{item.key}</div>
                          <div className="muted">{backupRelationshipLabel(item)}</div>
                        </div>
                        <div className="serviceBackupCountBadge">{item.relatedServiceCount}</div>
                      </div>
                      <div className="serviceBackupRowControls">
                        <div className="muted">{backupPolicyHint(item)}</div>
                        <BackupPolicySegmentedControl
                          disabled={settingsBusy}
                          itemLabel={item.key}
                          onChange={(value) => {
                            setServiceBackupTargetsDraft((prev) => ({
                              ...prev,
                              volumeNames: prev.volumeNames.map((entry) => (entry.key === item.key ? { ...entry, policy: value } : entry)),
                            }));
                          }}
                          value={item.policy}
                        />
                      </div>
                    </div>
                  ))}
                </div>

                <div className="serviceBackupGroup">
                  <div className="label">Bind paths</div>
                  {serviceBackupTargetsDraft.bindPaths.length === 0 ? <div className="muted">当前服务未声明可备份 bind path。</div> : null}
                  {serviceBackupTargetsDraft.bindPaths.map((item) => (
                    <div key={item.key} className="serviceBackupRow">
                      <div className="serviceBackupRowHead">
                        <div>
                          <div className="mono">{item.key}</div>
                          <div className="muted">{backupRelationshipLabel(item)}</div>
                        </div>
                        <div className="serviceBackupCountBadge">{item.relatedServiceCount}</div>
                      </div>
                      <div className="serviceBackupRowControls">
                        <div className="muted">{backupPolicyHint(item)}</div>
                        <BackupPolicySegmentedControl
                          disabled={settingsBusy}
                          itemLabel={item.key}
                          onChange={(value) => {
                            setServiceBackupTargetsDraft((prev) => ({
                              ...prev,
                              bindPaths: prev.bindPaths.map((entry) => (entry.key === item.key ? { ...entry, policy: value } : entry)),
                            }));
                          }}
                          value={item.policy}
                        />
                      </div>
                    </div>
                  ))}
                </div>
              </>
            )}

            <div className="formActions">
              <Button
                variant="primary"
                disabled={settingsBusy}
                onClick={() => {
                  void (async () => {
                    setBusy(true);
                    setError(null);
                    try {
                      await putServiceBackupTargets(props.serviceId, backupTargetRequestFromDraft(serviceBackupTargetsDraft));
                      await requestRefresh();
                    } catch (e: unknown) {
                      setError(errorMessage(e));
                    } finally {
                      setBusy(false);
                    }
                  })();
                }}
              >
                保存备份设置
              </Button>
            </div>
          </div>
        </div>
      </ResponsiveSettingsDrawer>
      {error ? <div className="error">{error}</div> : null}
      {notice ? (
        <div className="success">
          已创建{notice.kind === "rollback" ? "回滚" : notice.kind === "lifecycle" ? "生命周期" : "更新"}任务 <Mono>{notice.jobId}</Mono> ·{" "}
          <Button variant="ghost" disabled={busy} onClick={() => navigate({ name: "queue" })}>
            查看队列
          </Button>
        </div>
      ) : null}
    </div>
  );
}
