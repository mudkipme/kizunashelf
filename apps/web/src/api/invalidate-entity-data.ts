import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";

// Query keys whose data can change when a single entity is created, edited,
// renamed, or deleted. Anything derived from the entity set (home shelves,
// upcoming, lists — a rename rewrites wikilinks inside list files — smart-list
// results and previews (the library browser is one), calendar, analytics,
// cleanup queues, stats) is refetched; unrelated caches (config, capabilities,
// provider catalog) are left untouched.
const ENTITY_DATA_KEYS = [
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
] as const;

/**
 * Returns a stable callback that invalidates exactly the entity-derived
 * queries, rather than the entire cache. Shared by every entity mutation flow
 * (view, edit, create) so they stay in sync.
 */
export function useInvalidateEntityData() {
  const queryClient = useQueryClient();
  return useCallback(async () => {
    await Promise.all(
      ENTITY_DATA_KEYS.map((key) => queryClient.invalidateQueries({ queryKey: [key] })),
    );
  }, [queryClient]);
}
