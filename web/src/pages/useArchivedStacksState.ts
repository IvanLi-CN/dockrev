import { useCallback, useEffect, useRef, useState } from "react";
import {
  getStack,
  listStacksArchived,
  type StackDetail,
  type StackListItem,
} from "../api";
import { useManagementEventBatch } from "../managementEvents";
import type { AsyncDataPhase, AsyncDataTrigger } from "../asyncData";

export function useArchivedStacksState() {
  const [archivedStacks, setArchivedStacks] = useState<StackListItem[]>([]);
  const [archivedDetails, setArchivedDetails] = useState<
    Record<string, StackDetail | undefined>
  >({});
  const [phase, setPhase] = useState<AsyncDataPhase>("initial-loading");
  const [loaded, setLoaded] = useState(false);
  const [trigger, setTrigger] = useState<AsyncDataTrigger>("background");
  const [error, setError] = useState<string | null>(null);
  const requestIdRef = useRef(0);
  const hasCommittedDataRef = useRef(false);

  const requestRefresh = useCallback(async (nextTrigger: AsyncDataTrigger = "background") => {
    const requestId = ++requestIdRef.current;
    setTrigger(nextTrigger);
    setPhase(hasCommittedDataRef.current ? "refreshing" : "initial-loading");
    setError(null);
    try {
      const stacks = await listStacksArchived("only");
      if (requestId !== requestIdRef.current) return;
      setArchivedStacks(stacks);
      setLoaded(true);
      hasCommittedDataRef.current = true;
      const results = await Promise.allSettled(stacks.map(async (stack) => [stack.id, await getStack(stack.id)] as const));
      if (requestId !== requestIdRef.current) return;
      const details = Object.fromEntries(
        results.flatMap((result) => result.status === "fulfilled" ? [result.value] : []),
      );
      setArchivedDetails(details);
      const rejected = results.find((result): result is PromiseRejectedResult => result.status === "rejected");
      if (rejected) {
        setError(rejected.reason instanceof Error ? rejected.reason.message : String(rejected.reason));
        setPhase("error");
        return;
      }
      setPhase(stacks.length === 0 ? "ready-empty" : "ready-data");
    } catch (reason: unknown) {
      if (requestId !== requestIdRef.current) return;
      const message = reason instanceof Error ? reason.message : String(reason);
      setError(message);
      setPhase("error");
      throw reason;
    }
  }, []);

  useEffect(() => {
    void Promise.resolve().then(() => requestRefresh()).catch(() => {});
  }, [requestRefresh]);

  useManagementEventBatch(({ events, resyncRequired }) => {
    const archiveStateChanged = events.some((event) => {
      if (event.domain !== "stacks" && event.domain !== "services") return false;
      const operation = event.summary.operation;
      return operation === "archived" || operation === "restored";
    });
    if (resyncRequired || archiveStateChanged) {
      void requestRefresh().catch(() => {});
    }
  });

  return {
    archivedDetails,
    archivedStacks,
    error,
    loaded,
    phase,
    requestRefresh,
    trigger,
  };
}
