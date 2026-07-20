import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  partitionAggregateUpdateServices,
  readUpdateGuardBlockedReason,
} from "../aggregateUpdateGuard";
import {
  ApiError,
  getStack,
  listStacks,
  listStacksArchived,
  triggerCheck,
  triggerUpdate,
  type Service,
  type ServiceDigestTagsScanSummary,
  type StackDetail,
  type StackListItem,
  type TriggerUpdateInput,
} from "../api";
import { normalizeDigest } from "../components/digest";
import { type UpdateCandidateFilter } from "../components/UpdateCandidateFilters";
import { useConfirm } from "../confirm";
import {
  DIGEST_SNAPSHOT_UPDATED_EVENT,
  type DigestSnapshotUpdatedDetail,
} from "../digestInferenceTracker";
import { imageRepoFromImageRef } from "../imageRepo";
import { selfUpgradeBaseUrl } from "../runtimeConfig";
import { Button, Mono } from "../ui";
import {
  resolveUpdateActionTargetKey,
  UPDATE_JOB_SETTLE_RETRY_MS,
  UPDATE_JOB_SETTLED_EVENT,
  useUpdateActionTracker,
  type UpdateJobSettledDetail,
} from "../updateActionTracking";
import { serviceRowStatus, type RowStatus } from "../updateStatus";
import { usePageResumeRefresh } from "../usePageResumeRefresh";
import { useSupervisorHealth } from "../useSupervisorHealth";
import {
  inferResolvedTagsFromSnapshot,
  isStrictSemverTag,
} from "../versionDisplay";

function scanHasFailures(
  scan: ServiceDigestTagsScanSummary | null | undefined,
): boolean {
  if (!scan) return false;
  return scan.manifestsTimeout > 0 || scan.manifestsError > 0;
}

function scanIsComplete(
  scan: ServiceDigestTagsScanSummary | null | undefined,
): boolean {
  if (!scan) return false;
  return scan.repoTagsConsidered >= scan.repoTagsTotal;
}
export function useServicesPageState(props: {
  onLastScanHint: (lastScan?: string) => void;
  onTopActions: (node: ReactNode) => void;
  manageTopActions?: boolean;
}) {
  const {
    onLastScanHint,
    onTopActions,
    manageTopActions = true,
  } = props;
  const confirm = useConfirm();
  const [stacks, setStacks] = useState<StackListItem[]>([]);
  const [details, setDetails] = useState<
    Record<string, StackDetail | undefined>
  >({});
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<UpdateCandidateFilter>("all");
  const [error, setError] = useState<string | null>(null);
  const [noticeJobId, setNoticeJobId] = useState<string | null>(null);
  const [noticeCheckJobId, setNoticeCheckJobId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const refreshRequestIdRef = useRef(0);
  const latestAppliedStackListRequestIdRef = useRef(0);
  const {
    beginSubmitting,
    endSubmitting,
    trackJob,
    isTargetBusy,
    getActiveJobByTarget,
    isTargetSubmitting,
  } = useUpdateActionTracker();
  const supervisor = useSupervisorHealth();
  const selfUpgradeUrl = useMemo(() => selfUpgradeBaseUrl(), []);

  const [archivedStacks, setArchivedStacks] = useState<StackListItem[]>([]);
  const [archivedDetails, setArchivedDetails] = useState<
    Record<string, StackDetail | undefined>
  >({});

  const refresh = useCallback(async () => {
    const requestId = ++refreshRequestIdRef.current;
    setError(null);
    try {
      const s = await listStacks();
      const maxLastScan = s
        .map((x) => x.lastCheckAt)
        .sort()
        .at(-1);

      if (requestId < latestAppliedStackListRequestIdRef.current) return;
      latestAppliedStackListRequestIdRef.current = requestId;
      setStacks(s);
      onLastScanHint(maxLastScan);
      setCollapsed((prev) => {
        const next = { ...prev };
        for (const st of s) {
          if (next[st.id] == null) next[st.id] = false;
        }
        return next;
      });

      const ids = s.map((x) => x.id);
      const results = await Promise.all(
        ids.map(async (id) => {
          try {
            return [id, await getStack(id)] as const;
          } catch {
            return [id, undefined] as const;
          }
        }),
      );
      if (requestId < latestAppliedStackListRequestIdRef.current) return;
      setDetails(Object.fromEntries(results));

      const a = await listStacksArchived("only").catch(() => []);
      const aIds = a.map((x) => x.id);
      const aResults = await Promise.all(
        aIds.map(async (id) => {
          try {
            return [id, await getStack(id)] as const;
          } catch {
            return [id, undefined] as const;
          }
        }),
      );

      if (requestId < latestAppliedStackListRequestIdRef.current) return;
      setArchivedStacks(a);
      setArchivedDetails(Object.fromEntries(aResults));
    } catch (error: unknown) {
      if (requestId < latestAppliedStackListRequestIdRef.current) return;
      throw error;
    }
  }, [onLastScanHint]);

  const patchStackDetails = useCallback(async (stackIds: string[]) => {
    const ids = [...new Set(stackIds.map((id) => id.trim()).filter(Boolean))];
    if (ids.length === 0) return;

    const results = await Promise.all(
      ids.map(async (id) => {
        try {
          return [id, await getStack(id)] as const;
        } catch {
          return [id, undefined] as const;
        }
      }),
    );

    const patch = Object.fromEntries(results);
    setDetails((prev) => ({ ...prev, ...patch }));
    setArchivedDetails((prev) => ({ ...prev, ...patch }));
  }, []);

  const patchStackLists = useCallback(
    async (stackIds: string[]) => {
      const ids = new Set(stackIds.map((id) => id.trim()).filter(Boolean));
      if (ids.size === 0) return;

      const [nextStacks, nextArchived] = await Promise.all([
        listStacks(),
        listStacksArchived("only").catch(() => []),
      ]);
      const nextById = new Map(
        nextStacks.map((item) => [item.id, item] as const),
      );
      const archivedById = new Map(
        nextArchived.map((item) => [item.id, item] as const),
      );
      const maxLastScan = nextStacks
        .map((item) => item.lastCheckAt)
        .sort()
        .at(-1);

      setStacks((prev) => prev.map((item) => nextById.get(item.id) ?? item));
      setArchivedStacks((prev) =>
        prev.map((item) => archivedById.get(item.id) ?? item),
      );
      onLastScanHint(maxLastScan);
      setCollapsed((prev) => {
        const merged = { ...prev };
        for (const item of nextStacks) {
          if (merged[item.id] == null) merged[item.id] = false;
        }
        return merged;
      });
    },
    [onLastScanHint],
  );

  const patchServiceInStackDetails = useCallback(
    (stackId: string, serviceId: string, patch: (svc: Service) => Service) => {
      const patchStack = (
        stack: StackDetail | undefined,
      ): StackDetail | undefined => {
        if (!stack) return stack;
        let changed = false;
        const nextServices = stack.services.map((svc) => {
          if (svc.id !== serviceId) return svc;
          changed = true;
          return patch(svc);
        });
        if (!changed) return stack;
        return { ...stack, services: nextServices };
      };

      setDetails((prev) => {
        const nextStack = patchStack(prev[stackId]);
        if (nextStack === prev[stackId]) return prev;
        return { ...prev, [stackId]: nextStack };
      });
      setArchivedDetails((prev) => {
        const nextStack = patchStack(prev[stackId]);
        if (nextStack === prev[stackId]) return prev;
        return { ...prev, [stackId]: nextStack };
      });
    },
    [],
  );

  const resolveSettledStackIds = useCallback(
    (detail: UpdateJobSettledDetail): string[] => {
      const explicitStackId = (detail.stackId ?? "").trim();
      if (explicitStackId) return [explicitStackId];

      const explicitServiceId = (detail.serviceId ?? "").trim();
      if (explicitServiceId) {
        const matched = [
          ...Object.entries(details),
          ...Object.entries(archivedDetails),
        ]
          .filter(([, stack]) =>
            stack?.services.some((svc) => svc.id === explicitServiceId),
          )
          .map(([stackId]) => stackId);
        if (matched.length > 0) return [...new Set(matched)];
      }

      if (detail.target.startsWith("stack:"))
        return [detail.target.slice("stack:".length)];
      if (detail.target.startsWith("service:")) {
        const serviceId = detail.target.slice("service:".length);
        return [...Object.entries(details), ...Object.entries(archivedDetails)]
          .filter(([, stack]) =>
            stack?.services.some((svc) => svc.id === serviceId),
          )
          .map(([stackId]) => stackId);
      }

      if (detail.scope === "all" || detail.target === "all")
        return stacks.map((stack) => stack.id);
      return [];
    },
    [archivedDetails, details, stacks],
  );

  const requestRefresh = usePageResumeRefresh(refresh, {
    onError: (e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)),
  });

  useEffect(() => {
    void requestRefresh().catch((e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)),
    );
  }, [requestRefresh]);

  useEffect(() => {
    let closed = false;
    const timers = new Set<number>();

    const handleRefreshError = (error: unknown) => {
      if (closed) return;
      setError(error instanceof Error ? error.message : String(error));
    };

    const schedule = (task: () => Promise<void>) => {
      const timer = window.setTimeout(() => {
        timers.delete(timer);
        void task().catch(handleRefreshError);
      }, UPDATE_JOB_SETTLE_RETRY_MS);
      timers.add(timer);
    };

    const onUpdateJobSettled = (evt: Event) => {
      const detail =
        evt instanceof CustomEvent
          ? (evt.detail as UpdateJobSettledDetail | null)
          : null;
      if (!detail) return;

      const isAll = detail.scope === "all" || detail.target === "all";
      const stackIds = resolveSettledStackIds(detail);
      if (isAll || stackIds.length === 0) {
        void requestRefresh().catch(handleRefreshError);
        schedule(async () => {
          await requestRefresh();
        });
        return;
      }

      void patchStackDetails(stackIds).catch(handleRefreshError);
      schedule(async () => {
        await patchStackDetails(stackIds);
        await patchStackLists(stackIds);
      });
    };

    window.addEventListener(UPDATE_JOB_SETTLED_EVENT, onUpdateJobSettled);
    return () => {
      closed = true;
      for (const timer of timers) window.clearTimeout(timer);
      window.removeEventListener(UPDATE_JOB_SETTLED_EVENT, onUpdateJobSettled);
    };
  }, [
    patchStackDetails,
    patchStackLists,
    requestRefresh,
    resolveSettledStackIds,
  ]);

  const applyDigestSnapshotUpdate = useCallback(
    (detail: DigestSnapshotUpdatedDetail) => {
      // Popover-triggered refresh stays local to the clicked service, but when that service's
      // current/candidate happen to share one digest both sides should consume the new snapshot.
      const imageRepo = (detail.imageRepo ?? "").trim().toLowerCase();
      const digestNorm = normalizeDigest(detail.digest)?.toLowerCase() ?? null;
      const triggerServiceId = (detail.triggerServiceId ?? "").trim();
      if (!imageRepo || !triggerServiceId || !digestNorm) return;

      const failures = scanHasFailures(detail.scan);
      const complete = scanIsComplete(detail.scan);

      const patchService = (svc: Service): Service => {
        if (svc.id !== triggerServiceId) return svc;
        const svcRepo = imageRepoFromImageRef(svc.image.ref);
        if (!svcRepo || svcRepo !== imageRepo) return svc;

        let changed = false;
        let next: Service = svc;

        const currentDigest =
          normalizeDigest(svc.image.digest)?.toLowerCase() ?? null;
        if (
          currentDigest &&
          currentDigest === digestNorm &&
          !isStrictSemverTag(svc.image.tag)
        ) {
          const inferred = inferResolvedTagsFromSnapshot(
            detail.tags,
            svc.image.tag,
          );
          const inferredFirst = inferred[0] ?? null;
          if (inferredFirst || (!failures && complete)) {
            changed = true;
            next = {
              ...next,
              image: {
                ...next.image,
                resolvedTag: inferredFirst,
                resolvedTags: inferred.length > 1 ? inferred : null,
              },
            };
          }
        }

        const candidate = next.candidate;
        const candidateDigest = candidate
          ? (normalizeDigest(candidate.digest)?.toLowerCase() ?? null)
          : null;
        if (
          candidate &&
          candidateDigest &&
          candidateDigest === digestNorm &&
          !isStrictSemverTag(candidate.tag)
        ) {
          const inferred = inferResolvedTagsFromSnapshot(
            detail.tags,
            candidate.tag,
          );
          const inferredFirst = inferred[0] ?? null;
          if (inferredFirst || (!failures && complete)) {
            changed = true;
            next = {
              ...next,
              candidate: {
                ...candidate,
                resolvedTag: inferredFirst,
              },
            };
          }
        }

        return changed ? next : svc;
      };

      const patchStacks = (
        prev: Record<string, StackDetail | undefined>,
      ): Record<string, StackDetail | undefined> => {
        let changed = false;
        const next: Record<string, StackDetail | undefined> = { ...prev };

        for (const [stackId, stack] of Object.entries(prev)) {
          if (!stack) continue;
          let stackChanged = false;
          const nextServices = stack.services.map((svc) => {
            const patched = patchService(svc);
            if (patched !== svc) stackChanged = true;
            return patched;
          });
          if (!stackChanged) continue;
          changed = true;
          next[stackId] = { ...stack, services: nextServices };
        }

        return changed ? next : prev;
      };

      setDetails(patchStacks);
      setArchivedDetails(patchStacks);
    },
    [],
  );

  useEffect(() => {
    if (typeof window === "undefined") return;
    const onDigestSnapshotUpdated = (evt: Event) => {
      const detail =
        evt instanceof CustomEvent
          ? (evt.detail as DigestSnapshotUpdatedDetail | null)
          : null;
      if (!detail) return;
      applyDigestSnapshotUpdate(detail);
    };
    window.addEventListener(
      DIGEST_SNAPSHOT_UPDATED_EVENT,
      onDigestSnapshotUpdated,
    );
    return () => {
      window.removeEventListener(
        DIGEST_SNAPSHOT_UPDATED_EVENT,
        onDigestSnapshotUpdated,
      );
    };
  }, [applyDigestSnapshotUpdate]);

  const pendingInferenceStackIds = useMemo(() => {
    const ids: string[] = [];
    for (const [stackId, detail] of Object.entries(details)) {
      if (!detail) continue;
      const hasPending = detail.services.some(
        (svc) => !svc.archived && svc.versionInference?.status === "pending",
      );
      if (hasPending) ids.push(stackId);
    }
    return ids;
  }, [details]);

  useEffect(() => {
    if (pendingInferenceStackIds.length === 0) return;
    let closed = false;
    let timer: number | null = null;

    const poll = async () => {
      const ids = [...pendingInferenceStackIds];
      const results = await Promise.all(
        ids.map(async (id) => {
          try {
            return [id, await getStack(id)] as const;
          } catch {
            return [id, undefined] as const;
          }
        }),
      );
      if (closed) return;
      const patch = Object.fromEntries(results);
      setDetails((prev) => ({ ...prev, ...patch }));
      timer = window.setTimeout(() => {
        void poll();
      }, 1200);
    };

    timer = window.setTimeout(() => {
      void poll();
    }, 1200);

    return () => {
      closed = true;
      if (timer != null) window.clearTimeout(timer);
    };
  }, [pendingInferenceStackIds]);

  useEffect(() => {
    if (!manageTopActions) return;
    onTopActions(
      <>
        <Button
          variant="ghost"
          disabled={busy}
          onClick={() => {
            void (async () => {
              setBusy(true);
              try {
                await requestRefresh();
              } catch (e: unknown) {
                setError(e instanceof Error ? e.message : String(e));
              } finally {
                setBusy(false);
              }
            })();
          }}
        >
          刷新
        </Button>
        <Button
          variant="ghost"
          disabled={busy}
          onClick={() => {
            void (async () => {
              setBusy(true);
              setError(null);
              setNoticeCheckJobId(null);
              try {
                const resp = await triggerCheck("all");
                setNoticeCheckJobId(resp.checkId);
                await requestRefresh();
              } catch (e: unknown) {
                if (e instanceof ApiError) {
                  if (e.status === 401)
                    setError("需要登录/鉴权（Forward Auth）");
                  else if (e.status === 409) {
                    const d = e.details;
                    const existingJobId =
                      d &&
                      typeof d === "object" &&
                      d !== null &&
                      "existingJobId" in d &&
                      typeof (d as Record<string, unknown>).existingJobId ===
                        "string"
                        ? ((d as Record<string, unknown>)
                            .existingJobId as string)
                        : null;
                    if (existingJobId) setNoticeCheckJobId(existingJobId);
                    else setError(e.message);
                  } else setError(e.message);
                } else {
                  setError(e instanceof Error ? e.message : String(e));
                }
              } finally {
                setBusy(false);
              }
            })();
          }}
        >
          立即扫描更新
        </Button>
      </>,
    );
  }, [busy, manageTopActions, onTopActions, requestRefresh]);

  const triggerApply = useCallback(
    async (input: {
      scope: "stack" | "service";
      stackId: string;
      serviceId?: string;
      targetLabel: string;
      buildRequest: () => Promise<TriggerUpdateInput>;
      confirmBody?: ReactNode;
      confirmTitle?: string;
    }) => {
      const scopeLabel = input.scope === "stack" ? "stack" : "service";
      const confirmVariant = input.scope === "service" ? "primary" : "danger";
      const ok = await confirm({
        title: input.confirmTitle ?? "确认执行更新？",
        body: input.confirmBody ?? (
          <>
            <div className="modalKvGrid">
              <div className="modalKvLabel">模式</div>
              <div className="modalKvValue">
                <Mono>apply</Mono>
              </div>
              <div className="modalKvLabel">范围</div>
              <div className="modalKvValue">
                <Mono>{scopeLabel}</Mono>
              </div>
              <div className="modalKvLabel">目标</div>
              <div className="modalKvValue">
                <Mono>{input.targetLabel}</Mono>
              </div>
              <div className="modalKvLabel">备份</div>
              <div className="modalKvValue">
                <Mono>inherit</Mono>
              </div>
              <div className="modalKvLabel">架构不匹配</div>
              <div className="modalKvValue">
                <Mono>disallow</Mono>
              </div>
            </div>
          </>
        ),
        confirmText: "执行更新",
        cancelText: "取消",
        confirmVariant,
        // Hide the pill badge; it doesn't add value for operators (scope/kv already shows intent).
        badgeText: null,
      });
      if (!ok) return;

      const targetKey = resolveUpdateActionTargetKey(
        input.scope,
        input.stackId,
        input.serviceId,
      );

      setError(null);
      setNoticeJobId(null);
      if (targetKey) beginSubmitting(targetKey);
      try {
        const resp = await triggerUpdate(await input.buildRequest());
        setNoticeJobId(resp.jobId);
        if (targetKey) trackJob(targetKey, resp.jobId, "queued");
      } catch (e: unknown) {
        if (e instanceof ApiError) {
          if (e.status === 401) setError("需要登录/鉴权（Forward Auth）");
          else if (e.status === 409) {
            const guardReason = readUpdateGuardBlockedReason(e);
            if (guardReason) setError(guardReason);
            else {
              setError("扫描结果已变化，请刷新并重新扫描后再更新");
              await requestRefresh();
            }
          } else setError(e.message);
        } else {
          setError(e instanceof Error ? e.message : String(e));
        }
      } finally {
        if (targetKey) endSubmitting(targetKey);
      }
    },
    [beginSubmitting, confirm, endSubmitting, requestRefresh, trackJob],
  );

  const groupsAll = useMemo(() => {
    const q = search.trim().toLowerCase();
    const out: Array<{
      stackId: string;
      stackName: string;
      lastCheckAt: string;
      servicesAll: Array<{ svc: Service; status: RowStatus }>;
      servicesSearch: Array<{ svc: Service; status: RowStatus }>;
      countsAll: Record<Exclude<RowStatus, "ok">, number>;
      countsSearch: Record<Exclude<RowStatus, "ok">, number>;
      totalServices: number;
      aggregatePartition: ReturnType<typeof partitionAggregateUpdateServices>;
    }> = [];

    for (const st of stacks) {
      const d = details[st.id];
      if (!d) continue;

      const servicesAll = d.services
        .filter((svc) => !svc.archived)
        .map((svc) => ({ svc, status: serviceRowStatus(svc) }));

      const servicesSearch = q
        ? servicesAll.filter((x) => {
            const hay =
              `${d.name} ${x.svc.name} ${x.svc.image.ref} ${x.svc.image.tag}`.toLowerCase();
            return hay.includes(q);
          })
        : servicesAll;

      if (q && servicesSearch.length === 0) continue;

      const countsAll: Record<Exclude<RowStatus, "ok">, number> = {
        updatable: 0,
        hint: 0,
        archMismatch: 0,
        blocked: 0,
      };
      for (const x of servicesAll) {
        if (x.status === "ok") continue;
        countsAll[x.status] += 1;
      }

      const countsSearch: Record<Exclude<RowStatus, "ok">, number> = {
        updatable: 0,
        hint: 0,
        archMismatch: 0,
        blocked: 0,
      };
      for (const x of servicesSearch) {
        if (x.status === "ok") continue;
        countsSearch[x.status] += 1;
      }

      const totalServices = servicesAll.length;
      out.push({
        stackId: st.id,
        stackName: d.name,
        lastCheckAt: st.lastCheckAt,
        servicesAll,
        servicesSearch,
        countsAll,
        countsSearch,
        totalServices,
        aggregatePartition: partitionAggregateUpdateServices(d.services),
      });
    }

    return out;
  }, [details, search, stacks]);

  const filterSummary = useMemo(() => {
    let total = 0;
    const counts: Record<Exclude<RowStatus, "ok">, number> = {
      updatable: 0,
      hint: 0,
      archMismatch: 0,
      blocked: 0,
    };
    for (const g of groupsAll) {
      total += g.servicesSearch.length;
      for (const k of Object.keys(counts) as Array<Exclude<RowStatus, "ok">>) {
        counts[k] += g.countsSearch[k];
      }
    }
    return { total, counts };
  }, [groupsAll]);

  const groups = useMemo(() => {
    const out: Array<{
      stackId: string;
      stackName: string;
      lastCheckAt: string;
      services: Array<{ svc: Service; status: RowStatus }>;
      countsAll: Record<Exclude<RowStatus, "ok">, number>;
      totalServices: number;
      aggregatePartition: ReturnType<typeof partitionAggregateUpdateServices>;
    }> = [];

    for (const g of groupsAll) {
      const services =
        filter === "all"
          ? g.servicesSearch
          : g.servicesSearch.filter((x) => x.status === filter);
      if (filter !== "all" && services.length === 0) continue;
      out.push({
        stackId: g.stackId,
        stackName: g.stackName,
        lastCheckAt: g.lastCheckAt,
        services,
        countsAll: g.countsAll,
        totalServices: g.totalServices,
        aggregatePartition: g.aggregatePartition,
      });
    }
    return out;
  }, [filter, groupsAll]);

  const totals = useMemo(() => {
    let total = 0;
    for (const st of stacks) {
      const d = details[st.id];
      if (!d) continue;
      total += d.services.filter((svc) => !svc.archived).length;
    }
    const filtered = groups.reduce((acc, g) => acc + g.services.length, 0);
    return { total, filtered };
  }, [details, groups, stacks]);

  const archivedServices = useMemo(() => {
    const out: Array<{ stackId: string; stackName: string; svc: Service }> = [];
    for (const st of stacks) {
      const d = details[st.id];
      if (!d) continue;
      for (const svc of d.services) {
        if (svc.archived) out.push({ stackId: st.id, stackName: d.name, svc });
      }
    }
    return out;
  }, [details, stacks]);
  return {
    archivedDetails,
    archivedServices,
    archivedStacks,
    busy,
    collapsed,
    error,
    filter,
    filterSummary,
    getActiveJobByTarget,
    groups,
    isTargetBusy,
    isTargetSubmitting,
    noticeCheckJobId,
    noticeJobId,
    patchServiceInStackDetails,
    requestRefresh,
    search,
    selfUpgradeUrl,
    setBusy,
    setCollapsed,
    setError,
    setFilter,
    setSearch,
    supervisor,
    totals,
    triggerApply,
  };
}
