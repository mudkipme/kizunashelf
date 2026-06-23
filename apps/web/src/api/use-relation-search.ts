import { useCallback } from "react";
import { getEntities } from "@kizunashelf/api-contract";

import { apiFetch } from "@/api/client";
import type { RelationSuggestionSearch } from "@/components/entities/metadata-types";

/**
 * Shared relation lookup used by the entity create/edit pages to back the
 * relation field autocomplete. Searches entities of the requested relation type,
 * returning nothing when no type is given.
 */
export function useRelationSearch(): RelationSuggestionSearch {
  return useCallback<RelationSuggestionSearch>(async ({ relationType, query, signal }) => {
    const type = relationType?.trim();
    if (!type) return [];
    const result = await getEntities(
      {
        type,
        q: query.trim() || undefined,
        pageSize: 25,
        sort: "title",
        direction: "asc",
      },
      { signal },
      apiFetch,
    );
    return result.items;
  }, []);
}
