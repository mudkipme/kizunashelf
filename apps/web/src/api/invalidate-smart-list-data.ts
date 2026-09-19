import type { QueryClient } from "@tanstack/react-query";

import { invalidateQueryRoots, type QueryRoot } from "@/api/queries";

/** Smart-list files drive the list index, their detail pages, and Home. */
const SMART_LIST_DATA_ROOTS: readonly QueryRoot[] = [
  "lists",
  "smartList",
  "smartListResults",
  "smartListSuggestions",
  "home",
];

export async function invalidateSmartListData(queryClient: QueryClient) {
  await invalidateQueryRoots(queryClient, SMART_LIST_DATA_ROOTS);
}
