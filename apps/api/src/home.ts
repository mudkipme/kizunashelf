import type { HomeSectionConfig, Library } from "@kizunashelf/core";

import { clampNumber, sortEntities, type SortDirection } from "./utils";

export function buildHomeSection(library: Library, section: HomeSectionConfig) {
  const type = library.config.types.find((item) => item.id === section.type);
  const statuses = normalizeStatuses(section.status);
  const limit = clampNumber(Number(section.limit ?? 12), 1, 48);
  const direction: SortDirection = section.direction === "desc" ? "desc" : "asc";
  const sort = section.sort ?? "title";

  const filtered = library.summaries.filter((entity) => {
    if (entity.type !== section.type) return false;
    if (statuses.length === 0) return true;
    return statuses.includes(entity.status ?? "Unknown");
  });
  const items = sortEntities(filtered, sort, direction).slice(0, limit);

  return {
    ...section,
    typeLabel: type?.label ?? section.type,
    status: statuses,
    limit,
    sort,
    direction,
    total: filtered.length,
    items,
  };
}

function normalizeStatuses(status: HomeSectionConfig["status"]): string[] {
  if (!status) return [];
  if (Array.isArray(status)) return status.filter(Boolean);
  return [status].filter(Boolean);
}
