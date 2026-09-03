import { getEntities } from "@kizunashelf/api-contract";
import { useCallback } from "react";

import { apiFetch } from "@/api/client";
import type { RelationSuggestionSearch } from "@/components/entities/metadata-types";
import { relevanceSort } from "@/lib/constants";
import { useTitleLanguage } from "@/lib/language";

/**
 * Shared relation lookup used by the entity create/edit pages to back the
 * relation field autocomplete. Searches entities of the requested relation type,
 * returning nothing when no type is given. A query ranks matches by relevance
 * (exact/prefix first); browsing with no query falls back to title order (the
 * core treats `relevance` without a query as a title sort).
 */
export function useRelationSearch(): RelationSuggestionSearch {
  const titleLanguage = useTitleLanguage();
  return useCallback<RelationSuggestionSearch>(
    async ({ relationType, query, signal }) => {
      const type = relationType?.trim();
      if (!type) return [];
      const trimmed = query.trim();
      const result = await getEntities(
        {
          type,
          q: trimmed || undefined,
          pageSize: 25,
          sort: trimmed ? relevanceSort : "title",
          direction: "asc",
          titleLanguage,
        },
        { signal },
        apiFetch,
      );
      return result.items;
    },
    [titleLanguage],
  );
}
