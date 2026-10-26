import { useMemo, useState, type ReactNode } from "react";
import {
  ApiError,
  archiveStack,
  deleteMissingStack,
  getStackRecordDeletionPreview,
  restoreService,
  restoreStack,
  type Service,
} from "../api";
import { ReadonlySnapshotNotice } from "../components/ReadonlySnapshotNotice";
import { navigate } from "../routes";
import { Button, Mono, Pill } from "../ui";
import {
  AsyncDataRegion,
  AsyncDataSkeleton,
} from "../components/AsyncDataRegion";
import { splitImageNameForDisplay, splitImageRef } from "../imageLinks";
import { formatCurrentTagDisplay as formatTagDisplay } from "../versionDisplay";
import { OperationsDashboardSectionView } from "./OperationsDashboardSection";
import {
  useArchivedStacksState,
  useMissingStacksState,
} from "./useArchivedStacksState";
import { useOverviewPageState } from "./useOverviewPageState";
import { useConfirm } from "../confirm";

function formatShort(ts: string) {
  const d = new Date(ts);
  if (Number.isNaN(d.valueOf())) return ts;
  return d.toLocaleString();
}

function missingStackReasonCopy(reason?: string) {
  const normalized = reason?.trim() ?? "";
  if (
    normalized === "stack_is_not_missing" ||
    normalized === "stack is no longer missing"
  ) {
    return "该 Stack 已恢复为可发现状态，请刷新后重试。";
  }
  if (
    normalized === "stack_has_pending_operation" ||
    normalized === "stack has a pending operation"
  ) {
    return "该 Stack 存在正在运行的任务，请等待任务结束后重试。";
  }
  if (normalized === "managed_override_recovery_pending") {
    return "该 Stack 仍有待恢复的临时配置，请先完成恢复。";
  }
  if (
    normalized === "compose_files_reappeared" ||
    normalized === "Compose files reappeared"
  ) {
    return "Compose 文件已重新出现，请刷新后确认。";
  }
  if (normalized.startsWith("compose_files_unreadable")) {
    return "Compose 文件状态暂时无法确认，请检查权限后重试。";
  }
  if (
    normalized.startsWith("backup_artifact_unsafe") ||
    normalized === "backup artifacts changed"
  ) {
    return "备份路径或内容已变化，记录未删除，请刷新后重试。";
  }
  if (normalized === "database deletion preconditions changed") {
    return "删除前提已变化，记录未删除，请刷新后重试。";
  }
  if (normalized === "stack record deletion already in progress") {
    return "已有删除请求正在处理，请稍后刷新。";
  }
  return "删除前提已变化，记录未删除，请刷新后重试。";
}

function missingStackConflictMessage(error: ApiError) {
  const details =
    typeof error.details === "object" && error.details !== null
      ? (error.details as { reason?: unknown })
      : null;
  const reason =
    typeof details?.reason === "string" ? details.reason : error.message;
  return `删除冲突：${missingStackReasonCopy(reason)}`;
}

export function ServicesPage(props: {
  onLastScanHint: (lastScan?: string) => void;
  onTopActions: (node: ReactNode) => void;
}) {
  const state = useOverviewPageState(props);
  const {
    details,
    readonlyOffline,
    requestRefresh,
    snapshotActive,
    snapshotFetchedAt,
    stacks,
  } = state;
  const {
    archivedDetails,
    archivedStacks,
    error: archivedError,
    loaded: archivedLoaded,
    phase: archivedPhase,
    requestRefresh: requestArchivedRefresh,
    trigger: archivedTrigger,
  } = useArchivedStacksState();
  const {
    archivedDetails: missingDetails,
    archivedStacks: missingStacks,
    error: missingError,
    loaded: missingLoaded,
    phase: missingPhase,
    requestRefresh: requestMissingRefresh,
    trigger: missingTrigger,
  } = useMissingStacksState();
  const confirm = useConfirm();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refreshRecordViews = async () => {
    await Promise.all([
      requestRefresh({ source: "memory", trigger: "user-action" }),
      requestArchivedRefresh("user-action"),
      requestMissingRefresh("user-action"),
    ]);
  };
  const archivedServices = useMemo(() => {
    const out: Array<{ stackId: string; stackName: string; svc: Service }> = [];
    for (const stack of stacks) {
      const detail = details[stack.id];
      if (!detail) continue;
      for (const svc of detail.services) {
        if (svc.archived) {
          out.push({ stackId: stack.id, stackName: detail.name, svc });
        }
      }
    }
    return out;
  }, [details, stacks]);

  return (
    <div className="page">
      {snapshotActive ? (
        <ReadonlySnapshotNotice
          tone={readonlyOffline ? "warn" : "info"}
          title={
            readonlyOffline
              ? "当前离线，显示已缓存的运维大盘数据。"
              : "运维大盘先显示已缓存数据，后台会继续刷新。"
          }
          detail="扫描、批量更新、自升级和归档恢复都需要恢复联网后再继续。"
          fetchedAt={snapshotFetchedAt}
          actionLabel="重试刷新"
          actionDisabled={readonlyOffline || busy}
          onAction={() => {
            void (async () => {
              setBusy(true);
              setError(null);
              try {
                await refreshRecordViews();
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
      ) : readonlyOffline ? (
        <ReadonlySnapshotNotice
          tone="bad"
          title="当前没有可用的离线运维数据。"
          detail="请恢复联网后重新加载该页面。"
        />
      ) : null}
      <OperationsDashboardSectionView state={state} />

      <AsyncDataRegion
        ariaLabelledBy="missing-stacks-title"
        className="card"
        error={missingError}
        hasData={missingLoaded}
        label="正在刷新缺失 Stack"
        onRetry={() =>
          void requestMissingRefresh("user-action").catch(() => undefined)
        }
        phase={missingPhase}
        skeleton={
          <AsyncDataSkeleton
            className="archivedStacksLoadingSkeleton"
            lines={3}
          />
        }
        trigger={missingTrigger}
      >
        <div className="sectionRow">
          <div>
            <h2
              id="missing-stacks-title"
              className="title"
              style={{ margin: 0 }}
            >
              缺失 Stack
            </h2>
            <div className="muted">
              Compose 文件均已确认不存在。这里的删除只清理 Dockrev
              记录和备份文件，不修改宿主文件或 Docker 资源。
            </div>
          </div>
        </div>
        {missingPhase === "ready-empty" && missingStacks.length === 0 ? (
          <div className="muted">暂无缺失 Stack</div>
        ) : null}
        {missingStacks.length > 0 ? (
          <div className="svcGrid" style={{ marginTop: 10 }}>
            {missingStacks.map((stack) => {
              const detail = missingDetails[stack.id];
              const title = detail ? detail.name : stack.name;
              const archived = Boolean(stack.archived);
              return (
                <div
                  key={stack.id}
                  className="svcCard"
                  style={{ cursor: "default" }}
                >
                  <div className="svcCardTop">
                    <div className="svcCardName">{title}</div>
                    <div style={{ display: "flex", gap: 6 }}>
                      <Pill tone="bad">missing</Pill>
                      {archived ? <Pill tone="muted">archived</Pill> : null}
                    </div>
                  </div>
                  <div className="svcCardMeta">
                    <div className="muted">
                      id <Mono>{stack.id}</Mono>
                    </div>
                    <div className="muted">
                      services <Mono>{stack.services}</Mono> · last scan{" "}
                      <Mono>{formatShort(stack.lastCheckAt)}</Mono>
                    </div>
                    <div
                      style={{
                        display: "flex",
                        gap: 8,
                        marginTop: 8,
                        flexWrap: "wrap",
                      }}
                    >
                      <Button
                        variant="primary"
                        disabled={busy || readonlyOffline}
                        onClick={() => {
                          void (async () => {
                            setBusy(true);
                            setError(null);
                            try {
                              if (archived) {
                                await restoreStack(stack.id);
                              } else {
                                await archiveStack(stack.id);
                              }
                              await refreshRecordViews();
                            } catch (value: unknown) {
                              setError(
                                value instanceof Error
                                  ? value.message
                                  : String(value),
                              );
                            } finally {
                              setBusy(false);
                            }
                          })();
                        }}
                      >
                        {archived ? "恢复归档" : "归档 Stack"}
                      </Button>
                      <Button
                        variant="ghost"
                        disabled={busy || readonlyOffline}
                        onClick={() => {
                          void (async () => {
                            setBusy(true);
                            setError(null);
                            try {
                              const preview =
                                await getStackRecordDeletionPreview(stack.id);
                              if (!preview.canDelete) {
                                throw new Error(
                                  `当前不可删除：${missingStackReasonCopy(preview.blockingReason)}`,
                                );
                              }
                              const approved = await confirm({
                                title: `永久删除缺失记录：${title}`,
                                body: (
                                  <div>
                                    <p>
                                      将删除 Dockrev 中的
                                      Stack、服务、历史元数据和备份文件。
                                    </p>
                                    <p>
                                      不会停止、删除或修改宿主机 Compose
                                      文件、容器、镜像、卷、网络或构建缓存。
                                    </p>
                                    <p>
                                      如果仍有 Docker
                                      资源，它们会保留，但删除记录后 Dockrev
                                      不再把它们视为该 Stack
                                      的已知归属，后续可能按未归属资源规则处理。
                                    </p>
                                    <p>
                                      删除后若 Compose 文件重新出现，Discovery
                                      会创建新的 Stack 记录。
                                    </p>
                                  </div>
                                ),
                                confirmText: "永久删除记录",
                                confirmVariant: "danger",
                                badgeText: "不可撤销",
                                badgeTone: "bad",
                              });
                              if (!approved) return;
                              await deleteMissingStack(stack.id);
                              await refreshRecordViews();
                            } catch (value: unknown) {
                              const message =
                                value instanceof ApiError &&
                                value.status === 409
                                  ? missingStackConflictMessage(value)
                                  : value instanceof Error
                                    ? value.message
                                    : String(value);
                              setError(message);
                              await refreshRecordViews().catch(() => undefined);
                            } finally {
                              setBusy(false);
                            }
                          })();
                        }}
                      >
                        永久删除缺失记录
                      </Button>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        ) : null}
      </AsyncDataRegion>

      <AsyncDataRegion
        className="card"
        error={archivedError}
        hasData={archivedLoaded}
        label="正在刷新归档对象"
        onRetry={() =>
          void requestArchivedRefresh("user-action").catch(() => undefined)
        }
        phase={archivedPhase}
        skeleton={
          <AsyncDataSkeleton
            className="archivedStacksLoadingSkeleton"
            lines={4}
          />
        }
        trigger={archivedTrigger}
      >
        <div className="sectionRow">
          <div>
            <div className="title">已归档</div>
            <div className="muted">
              保留 stack / service 恢复能力，避免职责切换后丢失运维修复入口。
            </div>
          </div>
        </div>
        {archivedPhase === "ready-empty" &&
        archivedStacks.length === 0 &&
        archivedServices.length === 0 ? (
          <div className="muted">暂无归档对象</div>
        ) : null}

        {archivedStacks.length > 0 ? (
          <div style={{ marginTop: 10 }}>
            <div className="muted" style={{ marginBottom: 8 }}>
              已归档 stacks（按 stack 成组展示）
            </div>
            <div className="svcGrid">
              {archivedStacks.map((stack) => {
                const detail = archivedDetails[stack.id];
                const title = detail ? detail.name : stack.name;
                return (
                  <div
                    key={stack.id}
                    className="svcCard"
                    style={{ cursor: "default" }}
                  >
                    <div className="svcCardTop">
                      <div className="svcCardName">{title}</div>
                      <Pill tone="muted">archived</Pill>
                    </div>
                    <div className="svcCardMeta">
                      <div className="muted">
                        id <Mono>{stack.id}</Mono>
                      </div>
                      <div className="muted">
                        services <Mono>{stack.services}</Mono> · archived
                        services <Mono>{stack.archivedServices ?? 0}</Mono> ·
                        updates <Mono>{stack.updates}</Mono>
                      </div>
                      <div className="muted">
                        last scan <Mono>{formatShort(stack.lastCheckAt)}</Mono>
                      </div>
                      <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
                        <Button
                          variant="primary"
                          disabled={busy || readonlyOffline}
                          onClick={() => {
                            void (async () => {
                              setBusy(true);
                              setError(null);
                              try {
                                await restoreStack(stack.id);
                                await Promise.all([
                                  requestRefresh({
                                    source: "memory",
                                    trigger: "user-action",
                                  }),
                                  requestArchivedRefresh(),
                                ]);
                              } catch (value: unknown) {
                                setError(
                                  value instanceof Error
                                    ? value.message
                                    : String(value),
                                );
                              } finally {
                                setBusy(false);
                              }
                            })();
                          }}
                        >
                          恢复 stack
                        </Button>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        ) : null}

        {archivedServices.length > 0 ? (
          <div style={{ marginTop: 16 }}>
            <div className="muted" style={{ marginBottom: 8 }}>
              已归档 services（按所属 stack 聚合）
            </div>
            <div className="svcGrid">
              {archivedServices.map((item) => (
                <div
                  key={item.svc.id}
                  className="svcCard"
                  style={{ cursor: "default" }}
                >
                  <div className="svcCardTop">
                    <div className="svcCardName">{item.svc.name}</div>
                    <Pill tone="muted">archived</Pill>
                  </div>
                  <div className="svcCardMeta">
                    <div className="muted">
                      stack <Mono>{item.stackName}</Mono>
                    </div>
                    {(() => {
                      const image = splitImageRef(item.svc.image.ref);
                      const displayName = splitImageNameForDisplay(
                        image.name,
                        item.svc.image.tag,
                      );
                      return (
                        <div className="muted">
                          image{" "}
                          <span
                            className="mono"
                            title={
                              displayName.suffix
                                ? `${displayName.base}${displayName.suffix}`
                                : displayName.base
                            }
                          >
                            {displayName.base}
                          </span>{" "}
                          · registry <Mono>{image.registry}</Mono> · current{" "}
                          <Mono>
                            {formatTagDisplay(
                              item.svc.image.tag,
                              item.svc.image.resolvedTag,
                              item.svc.versionInference?.status,
                            )}
                          </Mono>
                        </div>
                      );
                    })()}
                    <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
                      <Button
                        variant="primary"
                        disabled={busy || readonlyOffline}
                        onClick={() => {
                          void (async () => {
                            setBusy(true);
                            setError(null);
                            try {
                              await restoreService(item.svc.id);
                              await Promise.all([
                                requestRefresh({
                                  source: "memory",
                                  trigger: "user-action",
                                }),
                                requestArchivedRefresh(),
                              ]);
                            } catch (value: unknown) {
                              setError(
                                value instanceof Error
                                  ? value.message
                                  : String(value),
                              );
                            } finally {
                              setBusy(false);
                            }
                          })();
                        }}
                      >
                        恢复 service
                      </Button>
                      <Button
                        variant="ghost"
                        disabled={busy}
                        onClick={() =>
                          navigate({
                            name: "service",
                            stackId: item.stackId,
                            serviceId: item.svc.id,
                          })
                        }
                      >
                        打开详情
                      </Button>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        ) : null}
      </AsyncDataRegion>

      {error ? (
        <div className="error" role="alert" aria-live="assertive">
          {error}
        </div>
      ) : null}
      {busy ? <div className="muted">处理中…</div> : null}
    </div>
  );
}
