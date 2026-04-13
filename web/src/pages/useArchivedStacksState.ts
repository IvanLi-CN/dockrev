import { useCallback, useEffect, useState } from "react";
import {
  getStack,
  listStacksArchived,
  type StackDetail,
  type StackListItem,
} from "../api";

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
    const timer = window.setTimeout(() => {
      void requestRefresh().catch(() => {});
    }, 0);
    return () => {
      window.clearTimeout(timer);
    };
  }, [requestRefresh]);

  return {
    archivedDetails,
    archivedStacks,
    requestRefresh,
  };
}
