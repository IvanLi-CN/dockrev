import {
  type CSSProperties,
  type ReactNode,
} from "react";
import {
  DOCKREV_AGGREGATE_GUARD_HINT,
  resolveAggregateUpdateActionState,
} from "../aggregateUpdateGuard";
import { AggregateUpdatePreviewList } from "../components/AggregateUpdatePreviewList";
import { ConfirmServiceVersionCell } from "../components/ConfirmServiceVersionCell";
import { CurrentVersionPopover } from "../components/CurrentVersionPopover";
import { UpdateCandidateFilters } from "../components/UpdateCandidateFilters";
import { VersionTagsPopover } from "../components/VersionTagsPopover";
import {
  ImageLinkIcons,
  splitImageNameForDisplay,
  splitImageRef,
} from "../imageLinks";
import { navigate } from "../routes";
import { resolveCandidateVersionState } from "../candidateVersionState";
import { ArrowRightIcon, Button, Input, Mono, Pill, StatusRemark } from "../ui";
import { resolveUpdateActionTargetKey } from "../updateActionTracking";
import {
  blockedReasonFor,
  isSemverDowngradeAnomaly,
} from "../updateStatus";
import {
  buildUpdateServiceTarget,
  buildUpdateServiceTargets,
} from "../updateTargets";
import {
  DiscoveryIssueDetailDialog,
  buildDiscoveryIssueMetaParts,
} from "./overviewHelpers";
import {
  GroupGuide,
  StackIcon,
  formatCompactDateTime,
  formatGroupSummary,
  formatShort,
  isDockrevService,
  shouldPrefetchFloatingCandidate,
  stopRowLink,
} from "./operationsDashboardShared";
import { buildStackAggregateScope } from "./aggregateUpdateScope";
import { useOverviewPageState } from "./useOverviewPageState";

export function OperationsDashboardSection(props: {
  onLastScanHint: (lastScan?: string) => void;
  onTopActions: (node: ReactNode) => void;
}) {
  const state = useOverviewPageState(props);
  return <OperationsDashboardSectionView state={state} />;
}

export function OperationsDashboardSectionView(props: {
  state: ReturnType<typeof useOverviewPageState>;
}) {
  const {
    activeDiscoveryIssue,
    busy,
    candidateSearch,
    collapsed,
    countsAll,
    details,
    discoverySummary,
    effectiveDiscoveryScanAt,
    error,
    filter,
    getActiveJobByTarget,
    isTargetBusy,
    isTargetSubmitting,
    jobsSummary,
    noticeCheckJobId,
    noticeDiscoveryJobId,
    noticeJobId,
    onChangeFilter,
    overviewCardJobs,
    patchServiceInStackDetails,
    runDiscoveryScan,
    selfUpgradeUrl,
    setCandidateSearch,
    setActiveDiscoveryIssue,
    stacks,
    supervisor,
    toggleStackCollapsed,
    totalServicesAll,
    triggerApply,
  } = props.state;

  return (
    <>
      <div className="twoCol">
        <div className="card">
          <div className="sectionRow">
            <div>
              <div className="title">运行态与结果</div>
              <div className="muted">更新任务（队列）摘要</div>
            </div>
            <div style={{ marginLeft: "auto", display: "flex", gap: 10 }}>
              <Button
                variant="ghost"
                disabled={busy}
                onClick={() => navigate({ name: "queue" })}
              >
                查看队列
              </Button>
            </div>
          </div>
          <div className="chipRow" style={{ marginTop: 14 }}>
            <div className="chipStatic">{`运行中: ${jobsSummary.running}`}</div>
            <div className="chipStatic">{`失败: ${jobsSummary.failed}`}</div>
            <div className="chipStatic">{`回滚: ${jobsSummary.rolled}`}</div>
            <div className="chipStatic">{`成功: ${jobsSummary.success}`}</div>
            {jobsSummary.other > 0 ? (
              <div className="chipStatic">{`其他: ${jobsSummary.other}`}</div>
            ) : null}
          </div>
          <div className="overviewJobsList">
            {overviewCardJobs.length === 0 ? (
              <div className="muted">暂无任务</div>
            ) : (
              overviewCardJobs.map((job) => {
                const progressTitle =
                  job.progressMode === "determinate" &&
                  job.progressPercent !== null
                    ? ` · progress ${job.progressPercent}%`
                    : job.progressMode === "indeterminate"
                      ? " · progress running"
                      : "";
                const title = `${job.status} · ${job.primaryLabel}${job.scopeTag ? ` ${job.scopeTag}` : ""} · ${formatShort(job.createdAt)} · by ${job.createdBy} · reason ${job.reason}${progressTitle}`;
                const ariaLabel = `${job.status}，${job.primaryLabel}${job.scopeTag ? ` ${job.scopeTag}` : ""}，创建时间 ${formatShort(job.createdAt)}，创建人 ${job.createdBy}，来源 ${job.reason}${
                  job.progressMode === "determinate" &&
                  job.progressPercent !== null
                    ? `，进度 ${job.progressPercent}%`
                    : job.progressMode === "indeterminate"
                      ? "，进度运行中"
                      : ""
                }`;
                return (
                  <button
                    key={job.jobId}
                    type="button"
                    className="overviewJobListRow"
                    data-progress-mode={job.progressMode}
                    style={
                      (job.progressMode === "determinate" &&
                      job.progressPercent !== null
                        ? {
                            "--overview-row-progress": `${job.progressPercent}%`,
                          }
                        : undefined) as CSSProperties | undefined
                    }
                    onClick={() => navigate({ name: "job", jobId: job.jobId })}
                    title={title}
                    aria-label={ariaLabel}
                  >
                    {job.progressMode !== "none" ? (
                      <span
                        className="overviewJobProgressBg"
                        aria-hidden="true"
                      >
                        <span className="overviewJobProgressBgFill" />
                        <span className="overviewJobProgressBgShimmer" />
                      </span>
                    ) : null}
                    <span className="overviewJobLine">
                      <span
                        className="overviewJobStatusTag"
                        data-status={job.status}
                      >
                        {job.status}
                      </span>
                      <span
                        className={`overviewJobTitle overviewJobTitle-${job.typeTone}`}
                      >
                        {job.primaryLabel}
                        {job.scopeTag ? (
                          <span className="overviewJobScope">
                            {" "}
                            · {job.scopeTag}
                          </span>
                        ) : null}
                      </span>
                      <span className="overviewJobLineMeta">
                        <span>{formatCompactDateTime(job.createdAt)}</span>
                        <span className="overviewJobLineMetaSep">·</span>
                        <span>{job.createdBy}</span>
                        {job.reason && job.reason !== "ui" ? (
                          <>
                            <span className="overviewJobLineMetaSep">·</span>
                            <span>{job.reason}</span>
                          </>
                        ) : null}
                      </span>
                    </span>
                  </button>
                );
              })
            )}
          </div>
        </div>

        <div className="card">
          <div className="sectionRow">
            <div className="discoveryCardHeader">
              <div className="title">扫描与发现异常</div>
              <div className="muted">
                按最近发现结果聚焦 warning / missing / invalid 项目
              </div>
            </div>
            <div className="discoveryCardActions">
              <Button
                variant="ghost"
                disabled={busy}
                onClick={runDiscoveryScan}
              >
                执行发现扫描
              </Button>
            </div>
          </div>
          <div className="chipRow discoverySummaryRow">
            <div className="discoveryStatChip discoveryStatChipTotal">
              <span className="discoveryStatLabel">异常项目</span>
              <span className="discoveryStatValue">
                {discoverySummary.issueCount}
              </span>
            </div>
            <div className="discoveryStatChip discoveryStatChipWarn">
              <span className="discoveryStatLabel">告警</span>
              <span className="discoveryStatValue">
                {discoverySummary.warning.length}
              </span>
            </div>
            <div className="discoveryStatChip discoveryStatChipBad">
              <span className="discoveryStatLabel">缺失</span>
              <span className="discoveryStatValue">
                {discoverySummary.missing.length}
              </span>
            </div>
            <div className="discoveryStatChip discoveryStatChipBad">
              <span className="discoveryStatLabel">无效</span>
              <span className="discoveryStatValue">
                {discoverySummary.invalid.length}
              </span>
            </div>
            <div className="discoveryStatChip discoveryStatChipInfo">
              <span className="discoveryStatLabel">活跃</span>
              <span className="discoveryStatValue">
                {discoverySummary.active.length}
              </span>
            </div>
            {effectiveDiscoveryScanAt ? (
              <div className="discoveryStatChip discoveryStatChipScan">
                <span className="discoveryStatLabel">最近扫描</span>
                <span className="discoveryStatValue">
                  {formatCompactDateTime(effectiveDiscoveryScanAt)}
                </span>
              </div>
            ) : null}
          </div>
          <div className="muted discoverySummaryLead">
            {discoverySummary.issueCount > 0
              ? `共 ${discoverySummary.issueCount} 个异常项目，优先展示最近 ${discoverySummary.issues.length} 个需要立即处理的条目。`
              : "最近一次扫描未发现需要处理的 warning / missing / invalid 项目。"}
          </div>
          {discoverySummary.issues.length > 0 ? (
            <div className="discoveryIssueList">
              {discoverySummary.issues.map((issue) => {
                const metaParts = buildDiscoveryIssueMetaParts(issue);
                const pillTone = issue.tone === "warning" ? "warn" : "bad";

                return (
                  <div
                    key={`${issue.tone}:${issue.project}`}
                    className="discoveryIssueRow"
                  >
                    <div className="discoveryIssueHeadline">
                      <div className="discoveryIssuePrimary">
                        <Pill tone={pillTone}>{issue.label}</Pill>
                        <span
                          className="mono monoPrimary discoveryIssueProject"
                          title={issue.project}
                        >
                          {issue.project}
                        </span>
                      </div>
                      <div className="discoveryIssueSummaryWrap">
                        <span
                          className="discoveryIssueSummary"
                          title={issue.fullError ? undefined : issue.summary}
                        >
                          {issue.summary}
                        </span>
                        {issue.fullError ? (
                          <button
                            type="button"
                            className="discoveryIssueDetailsBtn"
                            aria-label={`查看 ${issue.project} 的完整异常详情`}
                            onClick={() => setActiveDiscoveryIssue(issue)}
                          >
                            详情
                          </button>
                        ) : null}
                      </div>
                    </div>
                    {metaParts.length > 0 ? (
                      <div className="discoveryIssueMeta">
                        {metaParts.map((part, index) => (
                          <span
                            key={`${issue.project}:${part}`}
                            className="discoveryIssueMetaPart"
                          >
                            {index > 0 ? (
                              <span className="discoveryIssueMetaSep">·</span>
                            ) : null}
                            <span>{part}</span>
                          </span>
                        ))}
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="discoveryIssueEmpty">
              <div className="discoveryIssueEmptyTitle">
                当前没有需要处理的发现异常
              </div>
              <div className="muted">
                需要时仍可执行发现扫描，刷新 discovery projects 与 stacks
                的最新状态。
              </div>
            </div>
          )}
        </div>
      </div>

      <DiscoveryIssueDetailDialog
        issue={activeDiscoveryIssue}
        open={Boolean(activeDiscoveryIssue)}
        onOpenChange={(open) => {
          if (!open) setActiveDiscoveryIssue(null);
        }}
      />

      <div className="overviewIndent">
        <div className="sectionRow">
          <div className="title">更新候选</div>
          <div
            style={{
              marginLeft: "auto",
              display: "flex",
              gap: 10,
              alignItems: "center",
            }}
          >
            <Input
              className="input"
              onChange={(event) => setCandidateSearch(event.target.value)}
              placeholder="搜索 stack / service / image / Homepage"
              value={candidateSearch}
            />
          </div>
        </div>

        <div style={{ marginTop: 14 }}>
          <UpdateCandidateFilters
            value={filter}
            onChange={onChangeFilter}
            total={totalServicesAll}
            counts={countsAll}
          />
        </div>

        <div className="table" style={{ marginTop: 14 }}>
          <div className="tableHeader">
            <div>Service</div>
            <div>Image</div>
            <div>Versions</div>
            <div>状态 / 备注</div>
            <div>操作</div>
          </div>

          {stacks.map((st) => {
            const d = details[st.id];
            if (!d) return null;

            const scope = buildStackAggregateScope(
              d,
              filter,
              candidateSearch,
            );
            const rows = scope.rows;

            if (rows.length === 0) return null;

            const isCollapsed = collapsed[st.id] ?? false;
            const totalServices = scope.visibleServiceCount;
            const groupSummary = formatGroupSummary(
              totalServices,
              scope.counts,
            );
            const stackApply =
              resolveAggregateUpdateActionState({
                counts: scope.counts,
                guardedDockrevPreview: scope.previewItems.filter(
                  (item) => item.guardedDockrev,
                ),
                guardedApplyBlocked: scope.guardedApplyBlocked,
              });
            const stackApplyActionKey = resolveUpdateActionTargetKey(
              "stack",
              st.id,
              null,
            );
            const stackApplyActiveJob = stackApplyActionKey
              ? getActiveJobByTarget(stackApplyActionKey)
              : null;
            const stackApplySubmitting = stackApplyActionKey
              ? isTargetSubmitting(stackApplyActionKey)
              : false;

            return (
              <div
                key={st.id}
                className={
                  isCollapsed ? "tableGroup" : "tableGroup tableGroupExpanded"
                }
              >
                {!isCollapsed ? <GroupGuide /> : null}
                <div
                  className="groupHead"
                  role="button"
                  tabIndex={0}
                  onClick={() => toggleStackCollapsed(st.id)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      toggleStackCollapsed(st.id);
                    }
                  }}
                >
                  <div className="cellService cellServiceGroup">
                    <StackIcon
                      variant={isCollapsed ? "collapsed" : "expanded"}
                    />
                    <div className="groupTitle">{d.name}</div>
                  </div>
                  <div className="groupMeta">{groupSummary}</div>
                  <div />
                  <div />
                  <div
                    className="actionCell"
                    onClick={(e) => e.stopPropagation()}
                    onKeyDown={(e) => e.stopPropagation()}
                  >
                    <div className="actionStack">
                      <Button
                        variant="ghost"
                        disabled={
                          stackApplyActiveJob
                            ? false
                            : !stackApply.enabled || busy || stackApplySubmitting
                        }
                        loading={
                          stackApplyActionKey
                            ? isTargetBusy(stackApplyActionKey)
                            : false
                        }
                        loadingClickable={Boolean(stackApplyActiveJob)}
                        title={
                          stackApplyActiveJob
                            ? "任务进行中，点击查看任务详情"
                            : (stackApply.title ?? undefined)
                        }
                        hint={
                          stackApplyActiveJob
                            ? "任务进行中，点击查看任务详情"
                            : !stackApply.enabled
                              ? (stackApply.hint ?? undefined)
                              : undefined
                        }
                        onClick={() => {
                          if (stackApplyActiveJob) {
                            navigate({
                              name: "job",
                              jobId: stackApplyActiveJob.jobId,
                            });
                            return;
                          }
                          const totalCandidates = scope.actionableCount;
                          const anomalyCount = scope.previewItems.filter(
                            (item) => isSemverDowngradeAnomaly(item.svc),
                          ).length;
                          const body = (
                            <>
                              <div className="modalKvGrid">
                                <div className="modalKvLabel">范围</div>
                                <div className="modalKvValue">
                                  <Mono>stack</Mono>
                                </div>
                                <div className="modalKvLabel">目标</div>
                                <div className="modalKvValue">
                                  <Mono>{d.name}</Mono>
                                </div>
                                <div className="modalKvLabel">候选服务</div>
                                <div className="modalKvValue">
                                  {totalCandidates} 个（可更新/需确认）
                                </div>
                                <div className="modalKvLabel">其中</div>
                                <div className="modalKvValue">
                                  可更新 {scope.counts.updatable} · 需确认{" "}
                                  {scope.counts.hint}
                                </div>
                                <div className="modalKvLabel">将跳过</div>
                                <div className="modalKvValue">
                                  架构不匹配{" "}
                                  {scope.counts.archMismatch} · 被阻止{" "}
                                  {scope.counts.blocked}
                                </div>
                              </div>
                              {anomalyCount > 0 ? (
                                <div className="muted" style={{ marginTop: 10 }}>
                                  ⚠ 检测到 {anomalyCount}{" "}
                                  个版本异常（候选低于当前）；手动确认后仍可继续更新。
                                </div>
                              ) : null}
                              <div className="modalDivider" />
                              <div className="modalLead">
                                将更新的服务（预览）
                              </div>
                              <AggregateUpdatePreviewList
                                items={scope.previewItems}
                                dockrevGuardHint={DOCKREV_AGGREGATE_GUARD_HINT}
                                onServiceResolvedTags={(update) => {
                                  const stackId =
                                    (update.stackId ?? "").trim() || st.id;
                                  patchServiceInStackDetails(
                                    stackId,
                                    update.serviceId,
                                    (prev) => ({
                                      ...prev,
                                      image: {
                                        ...prev.image,
                                        resolvedTag: update.resolvedTag,
                                        resolvedTags: update.resolvedTags,
                                      },
                                    }),
                                  );
                                }}
                                onServiceCandidateResolvedTag={(update) => {
                                  const stackId =
                                    (update.stackId ?? "").trim() || st.id;
                                  patchServiceInStackDetails(
                                    stackId,
                                    update.serviceId,
                                    (prev) => ({
                                      ...prev,
                                      candidate: prev.candidate
                                        ? {
                                            ...prev.candidate,
                                            resolvedTag: update.resolvedTag,
                                          }
                                        : prev.candidate,
                                    }),
                                  );
                                }}
                              />
                              <div className="modalDivider" />
                            </>
                          );
                          void triggerApply({
                            scope: "stack",
                            stackId: st.id,
                            targetLabel: `stack:${d.name}`,
                            buildRequest: async () => ({
                              scope: "stack",
                              stackId: st.id,
                              targets: await buildUpdateServiceTargets(
                                scope.actionableServices,
                              ),
                              mode: "apply",
                              allowArchMismatch: false,
                              backupMode: "inherit",
                            }),
                            confirmBody: body,
                            confirmTitle: `确认更新此 stack？`,
                          });
                        }}
                      >
                        {stackApplyActiveJob?.status === "queued"
                          ? "排队中…"
                          : stackApplyActiveJob
                            ? "更新中…"
                            : stackApplySubmitting
                              ? "提交中…"
                              : "更新此 stack"}
                      </Button>
                    </div>
                  </div>
                </div>

                {!isCollapsed
                  ? rows.map(({ svc, stt }) => {
                      const isDockrev = isDockrevService(svc);
                      const versionState = resolveCandidateVersionState(svc);
                      const {
                        candidateDisplayTag,
                        candidateTag,
                        currentDisplayTag,
                        inferencePending,
                        sameDisplayUpdate,
                        showCandidate,
                        showRawTag,
                      } = versionState;
                      const candidatePrefetchOnMount =
                        candidateTag && candidateDisplayTag
                          ? shouldPrefetchFloatingCandidate(
                              candidateTag,
                              svc.candidate?.resolvedTag ?? null,
                              svc.candidate?.digest ?? null,
                            )
                          : false;
                      const arrowPulse = inferencePending;
                      const svcApply =
                        stt === "updatable"
                          ? {
                              enabled: true,
                              title: null as string | null,
                              note: null as string | null,
                            }
                          : stt === "hint"
                            ? {
                                enabled: true,
                                title: "需确认候选；将由服务端计算是否实际变更",
                                note: "需确认",
                              }
                            : stt === "ok"
                              ? {
                                  enabled: false,
                                  title: "无候选版本",
                                  note: null,
                                }
                              : stt === "archMismatch"
                                ? {
                                    enabled: false,
                                    title: "架构不匹配（仅提示，不允许更新）",
                                    note: null,
                                  }
                                : {
                                    enabled: false,
                                    title: blockedReasonFor(svc) ?? "被阻止",
                                    note: null,
                                  };
                      const svcApplyActionKey = resolveUpdateActionTargetKey(
                        "service",
                        null,
                        svc.id,
                      );
                      const svcApplyActiveJob = svcApplyActionKey
                        ? getActiveJobByTarget(svcApplyActionKey)
                        : null;
                      const svcApplySubmitting = svcApplyActionKey
                        ? isTargetSubmitting(svcApplyActionKey)
                        : false;
                      return (
                        <div
                          key={svc.id}
                          className="rowLine"
                          onClick={(e) => {
                            const t = e.target as unknown;
                            const el =
                              t instanceof Element
                                ? t
                                : t &&
                                    (t as { parentElement?: unknown })
                                      .parentElement instanceof Element
                                  ? (t as { parentElement: Element })
                                      .parentElement
                                  : null;
                            if (
                              el?.closest("button, a, input, select, textarea")
                            )
                              return;
                            navigate({
                              name: "service",
                              stackId: st.id,
                              serviceId: svc.id,
                            });
                          }}
                          role="button"
                          tabIndex={0}
                          onKeyDown={(e) => {
                            const t = e.target as unknown;
                            const el =
                              t instanceof Element
                                ? t
                                : t &&
                                    (t as { parentElement?: unknown })
                                      .parentElement instanceof Element
                                  ? (t as { parentElement: Element })
                                      .parentElement
                                  : null;
                            if (
                              el?.closest("button, a, input, select, textarea")
                            )
                              return;
                            if (e.key === "Enter" || e.key === " ") {
                              e.preventDefault();
                              navigate({
                                name: "service",
                                stackId: st.id,
                                serviceId: svc.id,
                              });
                            }
                          }}
                        >
                          <div className="cellService">
                            <span className="svcBullet" aria-hidden="true" />
                            <span className="svcName">{svc.name}</span>
                          </div>
                          {(() => {
                            const img = splitImageRef(svc.image.ref);
                            const dn = splitImageNameForDisplay(
                              img.name,
                              svc.image.tag,
                            );
                            return (
                              <div className="cellTwoLine">
                                <div
                                  className="mono monoPrimary monoSplit imageLinkRow"
                                  title={
                                    dn.suffix
                                      ? `${dn.base}${dn.suffix}`
                                      : dn.base
                                  }
                                >
                                  <span className="monoSplitBase">
                                    {dn.base}
                                  </span>
                                  <ImageLinkIcons
                                    imageRef={svc.image.ref}
                                    onClick={stopRowLink}
                                    repoUrl={svc.settings.repoUrl}
                                  />
                                </div>
                                <div className="mono monoSecondary">
                                  {img.registry}
                                </div>
                              </div>
                            );
                          })()}
                          <div className="cellTwoLine">
                            <div className="versionLine">
                              <CurrentVersionPopover
                                serviceId={svc.id}
                                displayTag={currentDisplayTag}
                                imageTag={svc.image.tag}
                                imageDigest={svc.image.digest ?? null}
                                resolvedTag={svc.image.resolvedTag}
                                resolvedTags={svc.image.resolvedTags}
                                onLocalResolvedTags={(update) => {
                                  patchServiceInStackDetails(
                                    st.id,
                                    svc.id,
                                    (prev) => ({
                                      ...prev,
                                      image: {
                                        ...prev.image,
                                        resolvedTag: update.resolvedTag,
                                        resolvedTags: update.resolvedTags,
                                      },
                                    }),
                                  );
                                }}
                                inferenceLoading={inferencePending}
                              />
                              {showCandidate ? (
                                <>
                                  <span
                                    className={
                                      arrowPulse
                                        ? "inlineIconLoading"
                                        : "inlineIconMuted"
                                    }
                                  >
                                    <ArrowRightIcon className="inlineIcon" />
                                  </span>
                                  <VersionTagsPopover
                                    serviceId={svc.id}
                                    candidateTag={candidateTag}
                                    candidateDigest={
                                      svc.candidate?.digest ?? null
                                    }
                                    prefetchOnMount={candidatePrefetchOnMount}
                                    onLocalResolvedTag={(resolvedTag) => {
                                      patchServiceInStackDetails(
                                        st.id,
                                        svc.id,
                                        (prev) => ({
                                          ...prev,
                                          candidate: prev.candidate
                                            ? {
                                                ...prev.candidate,
                                                resolvedTag,
                                              }
                                            : prev.candidate,
                                        }),
                                      );
                                    }}
                                  >
                                    {candidateDisplayTag}
                                  </VersionTagsPopover>
                                  {sameDisplayUpdate ? (
                                    <span className="versionInlineHint">
                                      同标签新 digest
                                    </span>
                                  ) : null}
                                </>
                              ) : null}
                            </div>
                            {showRawTag ? (
                              <div>
                                <CurrentVersionPopover
                                  serviceId={svc.id}
                                  displayTag={svc.image.tag}
                                  imageTag={svc.image.tag}
                                  imageDigest={svc.image.digest ?? null}
                                  resolvedTag={svc.image.resolvedTag}
                                  resolvedTags={svc.image.resolvedTags}
                                  onLocalResolvedTags={(update) => {
                                    patchServiceInStackDetails(
                                      st.id,
                                      svc.id,
                                      (prev) => ({
                                        ...prev,
                                        image: {
                                          ...prev.image,
                                          resolvedTag: update.resolvedTag,
                                          resolvedTags: update.resolvedTags,
                                        },
                                      }),
                                    );
                                  }}
                                  preferSource="rawTag"
                                  triggerClassName="versionTagsTrigger mono monoSecondary"
                                >
                                  {svc.image.tag}
                                </CurrentVersionPopover>
                              </div>
                            ) : null}
                          </div>
                          <StatusRemark service={svc} status={stt} />
                          <div
                            className="actionCell"
                            onClick={(e) => e.stopPropagation()}
                            onKeyDown={(e) => e.stopPropagation()}
                          >
                            {isDockrev ? (
                              <div className="actionStack">
                                <Button
                                  variant="ghost"
                                  disabled={
                                    busy || supervisor.state.status !== "ok"
                                  }
                                  title={
                                    supervisor.state.status === "offline"
                                      ? `自我升级不可用（supervisor offline） · ${supervisor.state.errorAt} · ${supervisor.state.error}`
                                      : supervisor.state.status === "checking"
                                        ? "检查 supervisor 中…"
                                        : undefined
                                  }
                                  onClick={() => {
                                    window.location.href = selfUpgradeUrl;
                                  }}
                                >
                                  升级 Dockrev
                                </Button>
                                {supervisor.state.status !== "ok" ? (
                                  <Button
                                    variant="ghost"
                                    disabled={
                                      busy ||
                                      supervisor.state.status === "checking"
                                    }
                                    onClick={() => {
                                      void supervisor.check();
                                    }}
                                  >
                                    重试
                                  </Button>
                                ) : null}
                                {supervisor.state.status === "offline" ? (
                                  <div className="muted">
                                    supervisor offline ·{" "}
                                    {supervisor.state.errorAt} ·{" "}
                                    <Mono>{supervisor.state.error}</Mono>
                                  </div>
                                ) : null}
                              </div>
                            ) : (
                              <Button
                                variant="ghost"
                                disabled={
                                  svcApplyActiveJob
                                    ? false
                                    : !svcApply.enabled ||
                                      busy ||
                                      svcApplySubmitting
                                }
                                loading={
                                  svcApplyActionKey
                                    ? isTargetBusy(svcApplyActionKey)
                                    : false
                                }
                                loadingClickable={Boolean(svcApplyActiveJob)}
                                title={
                                  svcApplyActiveJob
                                    ? "任务进行中，点击查看任务详情"
                                    : (svcApply.title ?? undefined)
                                }
                                hint={
                                  svcApplyActiveJob
                                    ? "任务进行中，点击查看任务详情"
                                    : undefined
                                }
                                onClick={() => {
                                  if (svcApplyActiveJob) {
                                    navigate({
                                      name: "job",
                                      jobId: svcApplyActiveJob.jobId,
                                    });
                                    return;
                                  }
                                  const body = (
                                    <>
                                      <div className="modalLead">
                                        将对该服务执行更新（apply）。
                                      </div>
                                      <div className="modalKvGrid">
                                        <div className="modalKvLabel">范围</div>
                                        <div className="modalKvValue">
                                          <Mono>service</Mono>
                                        </div>
                                        <div className="modalKvLabel">目标</div>
                                        <div className="modalKvValue">
                                          <Mono>{`${d.name}/${svc.name}`}</Mono>
                                        </div>
                                        <div className="modalKvLabel">镜像</div>
                                        <div className="modalKvValue">
                                          {(() => {
                                            const img = splitImageRef(
                                              svc.image.ref,
                                            );
                                            const dn = splitImageNameForDisplay(
                                              img.name,
                                              svc.image.tag,
                                            );
                                            return (
                                              <div className="cellTwoLine">
                                                <div
                                                  className="mono monoPrimary monoSplit imageLinkRow"
                                                  title={
                                                    dn.suffix
                                                      ? `${dn.base}${dn.suffix}`
                                                      : dn.base
                                                  }
                                                >
                                                  <span className="monoSplitBase">
                                                    {dn.base}
                                                  </span>
                                                  <ImageLinkIcons
                                                    imageRef={svc.image.ref}
                                                    repoUrl={
                                                      svc.settings.repoUrl
                                                    }
                                                  />
                                                </div>
                                                <div className="mono monoSecondary">
                                                  {img.registry}
                                                </div>
                                              </div>
                                            );
                                          })()}
                                        </div>
                                        <div className="modalKvLabel">
                                          目标版本
                                        </div>
                                        <div className="modalKvValue">
                                          <ConfirmServiceVersionCell
                                            serviceId={svc.id}
                                            imageTag={svc.image.tag}
                                            imageDigest={
                                              svc.image.digest ?? null
                                            }
                                            resolvedTag={svc.image.resolvedTag}
                                            resolvedTags={
                                              svc.image.resolvedTags
                                            }
                                            inferenceStatus={
                                              svc.versionInference?.status
                                            }
                                            candidateTag={svc.candidate?.tag}
                                            candidateDigest={
                                              svc.candidate?.digest ?? null
                                            }
                                            candidateResolvedTag={
                                              svc.candidate?.resolvedTag
                                            }
                                            prefetchOnMount={
                                              candidatePrefetchOnMount
                                            }
                                            onHostResolvedTags={(update) => {
                                              patchServiceInStackDetails(
                                                st.id,
                                                svc.id,
                                                (prev) => ({
                                                  ...prev,
                                                  image: {
                                                    ...prev.image,
                                                    resolvedTag:
                                                      update.resolvedTag,
                                                    resolvedTags:
                                                      update.resolvedTags,
                                                  },
                                                }),
                                              );
                                            }}
                                            onHostCandidateResolvedTag={(
                                              resolvedTag,
                                            ) => {
                                              patchServiceInStackDetails(
                                                st.id,
                                                svc.id,
                                                (prev) => ({
                                                  ...prev,
                                                  candidate: prev.candidate
                                                    ? {
                                                        ...prev.candidate,
                                                        resolvedTag,
                                                      }
                                                    : prev.candidate,
                                                }),
                                              );
                                            }}
                                          />
                                        </div>
                                        <div className="modalKvLabel">状态</div>
                                        <div className="modalKvValue">
                                          <Mono>{stt}</Mono>
                                        </div>
                                      </div>
                                      <div className="modalDivider" />
                                    </>
                                  );
                                  void triggerApply({
                                    scope: "service",
                                    stackId: st.id,
                                    serviceId: svc.id,
                                    targetLabel: `service:${d.name}/${svc.name}`,
                                    buildRequest: async () => ({
                                      scope: "service",
                                      stackId: st.id,
                                      ...(await buildUpdateServiceTarget(svc)),
                                      mode: "apply",
                                      allowArchMismatch: false,
                                      backupMode: "inherit",
                                    }),
                                    confirmBody: body,
                                    confirmTitle: `确认更新服务 ${svc.name}？`,
                                  });
                                }}
                              >
                                {svcApplyActiveJob?.status === "queued"
                                  ? "排队中…"
                                  : svcApplyActiveJob
                                    ? "更新中…"
                                    : svcApplySubmitting
                                      ? "提交中…"
                                      : "执行更新"}
                              </Button>
                            )}
                          </div>
                        </div>
                      );
                    })
                  : null}
              </div>
            );
          })}
        </div>
      </div>

      {error ? <div className="error">{error}</div> : null}
      {noticeJobId ? (
        <div className="success">
          已创建更新任务 <Mono>{noticeJobId}</Mono> ·{" "}
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => navigate({ name: "queue" })}
          >
            查看队列
          </Button>
        </div>
      ) : null}
      {noticeDiscoveryJobId ? (
        <div className="success">
          已创建扫描任务 <Mono>{noticeDiscoveryJobId}</Mono> ·{" "}
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => navigate({ name: "queue" })}
          >
            查看队列
          </Button>
        </div>
      ) : null}
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
    </>
  );
}
