import { useMemo, useState, type ReactNode } from "react";
import { restoreService, restoreStack, type Service } from "../api";
import { ReadonlySnapshotNotice } from "../components/ReadonlySnapshotNotice";
import { navigate } from "../routes";
import { Button, Mono, Pill } from "../ui";
import { AsyncDataRegion, AsyncDataSkeleton } from "../components/AsyncDataRegion";
import { splitImageNameForDisplay, splitImageRef } from "../imageLinks";
import { formatCurrentTagDisplay as formatTagDisplay } from "../versionDisplay";
import {
  OperationsDashboardSectionView,
} from "./OperationsDashboardSection";
import { useArchivedStacksState } from "./useArchivedStacksState";
import { useOverviewPageState } from "./useOverviewPageState";

function formatShort(ts: string) {
  const d = new Date(ts);
  if (Number.isNaN(d.valueOf())) return ts;
  return d.toLocaleString();
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
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const archivedServices = useMemo(() => {
    const out: Array<{ stackId: string; stackName: string; svc: Service }> =
      [];
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
                await Promise.all([requestRefresh(), requestArchivedRefresh("user-action")]);
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
        className="card"
        error={archivedError}
        hasData={archivedLoaded}
        label="正在刷新归档对象"
        onRetry={() => void requestArchivedRefresh("user-action").catch(() => undefined)}
        phase={archivedPhase}
        skeleton={<AsyncDataSkeleton className="archivedStacksLoadingSkeleton" lines={4} />}
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
        {archivedPhase === "ready-empty" && archivedStacks.length === 0 && archivedServices.length === 0 ? (
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
                                  requestRefresh(),
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
                                requestRefresh(),
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

      {error ? <div className="error">{error}</div> : null}
      {busy ? <div className="muted">处理中…</div> : null}
    </div>
  );
}
