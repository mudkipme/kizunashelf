import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";

import { queryKeys } from "@/api/queries";

/**
 * Returns a stable callback that refetches the list views after a list changes:
 * always the index (`lists`), plus one list's detail when `listId` is given
 * (omit it when only membership/counts on the index changed). Shared by every
 * list mutation flow — the lists page, a list's detail page, and "manage lists"
 * on an entity — so they invalidate consistently, mirroring
 * {@link useInvalidateEntityData}.
 */
export function useInvalidateLists() {
  const queryClient = useQueryClient();
  return useCallback(
    (listId?: string) =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.lists }),
        ...(listId ? [queryClient.invalidateQueries({ queryKey: queryKeys.list(listId) })] : []),
      ]),
    [queryClient],
  );
}
