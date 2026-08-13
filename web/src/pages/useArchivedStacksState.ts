import { useCallback, useEffect, useState } from "react";
import {
  getStack,
  listStacksArchived,
  type StackDetail,
  type StackListItem,
} from "../api";
import { useManagementEventBatch } from "../managementEvents";

export function useArchivedStacksState() {
  const [archivedStacks, setArchivedStacks] = useState<StackListItem[]>([]);
  const [archivedDetails, setArchivedDetails] = useState<
    Record<string, StackDetail | undefined>
  >({});

  const requestRefresh = useCallback(async () => {
    const stacks = await listStacksArchived("only").catch(() => []);
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
    requestRefresh,
  };
}
