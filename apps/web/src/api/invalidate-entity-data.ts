import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";

import { invalidateQueryRoots, type QueryRoot } from "@/api/queries";

// Query keys whose data can change when a single entity is created, edited,
// renamed, or deleted. Anything derived from the entity set (home shelves,
// upcoming, lists — a rename rewrites wikilinks inside list files — smart-list
// results and previews (the library browser is one), calendar, analytics,
// cleanup queues, stats, tags, diagnostics, external-search membership) is
// refetched; config, capabilities, and provider availability are left untouched.
const ENTITY_DATA_ROOTS: readonly QueryRoot[] = [
  "entity",
  "entityDates",
  "entities",
  "home",
  "upcoming",
  "lists",
  "list",
  "smartListResults",
  "smartListPreview",
  "calendar",
  "activity",
  "analytics",
  "cleanupQueues",
  "stats",
  "tags",
  "health",
  "externalSearch",
];

/**
 * Returns a stable callback that invalidates exactly the entity-derived
 * queries, rather than the entire cache. Shared by every entity mutation flow
 * (view, edit, create) so they stay in sync.
 */
export function useInvalidateEntityData() {
  const queryClient = useQueryClient();
  return useCallback(async () => {
    await invalidateQueryRoots(queryClient, ENTITY_DATA_ROOTS);
  }, [queryClient]);
}
