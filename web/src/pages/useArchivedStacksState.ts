import { useCallback, useEffect, useState } from "react";
import {
  getStack,
  listStacksArchived,
  type StackDetail,
  type StackListItem,
} from "../api";
import { useManagementEventBatch } from "../managementEvents";
import type { AsyncDataPhase } from "../asyncData";

export function useArchivedStacksState() {
  const [archivedStacks, setArchivedStacks] = useState<StackListItem[]>([]);
  const [archivedDetails, setArchivedDetails] = useState<
    Record<string, StackDetail | undefined>
  >({});
  const [phase, setPhase] = useState<AsyncDataPhase>("initial-loading");
  const [error, setError] = useState<string | null>(null);

  const requestRefresh = useCallback(async () => {
    setPhase((current) => current === "initial-loading" ? "initial-loading" : "refreshing");
    setError(null);
    try {
      const stacks = await listStacksArchived("only");
      const results = await Promise.all(
        stacks.map(async (stack) => {
          try {
            return [stack.id, await getStack(stack.id)] as const;
          } catch {
            return [stack.id, undefined] as const;
          }
        }),
      );
      setArchivedStacks(stacks);
      setArchivedDetails(Object.fromEntries(results));
      setPhase(stacks.length === 0 ? "ready-empty" : "ready-data");
    } catch (reason: unknown) {
      const message = reason instanceof Error ? reason.message : String(reason);
      setError(message);
      setPhase("error");
      throw reason;
    }
  }, []);

  useEffect(() => {
    void Promise.resolve().then(requestRefresh).catch(() => {});
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
    phase,
    requestRefresh,
  };
}
