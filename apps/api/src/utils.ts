import type { EntitySummary, Library } from "@kizunashelf/core";

import { dateSortKey } from "./dates";

export type SortDirection = "asc" | "desc";

export function countBy<T>(
  items: T[],
  select: (item: T) => string,
): Array<{ name: string; count: number }> {
  const counts = new Map<string, number>();
  for (const item of items) {
    const key = select(item);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }

  return [...counts.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => b.count - a.count);
}

export function clampNumber(value: number, min: number, max: number) {
  if (!Number.isFinite(value)) return min;
  return Math.floor(Math.min(max, Math.max(min, value)));
}

export function sortEntities(
  entities: EntitySummary[],
  sort: string,
  direction: SortDirection,
): EntitySummary[] {
  const multiplier = direction === "asc" ? 1 : -1;
  const sorted = [...entities].sort((a, b) => {
    if (sort === "title") return compareString(a.title, b.title) * multiplier;
    if (sort === "status") return compareString(a.status, b.status) * multiplier;
    if (sort.startsWith("date:")) {
      const field = sort.slice("date:".length);
      return compareOptionalString(
        entityDateSortValue(a, field),
        entityDateSortValue(b, field),
        direction,
      );
    }
    if (sort === "relations") return (a.relationCount - b.relationCount) * multiplier;
    if (sort === "path") return compareString(a.path, b.path) * multiplier;

    const typeCompare = compareString(a.typeLabel, b.typeLabel);
    if (typeCompare !== 0) return typeCompare * multiplier;
    return compareString(a.title, b.title) * multiplier;
  });

  return sorted;
}

export function getStatusTrackedTypeIds(library: Library) {
  return new Set(
    library.config.types
      .filter((type) => Boolean(type.fields.status?.length))
      .map((type) => type.id),
  );
}

export function compareString(a: string | undefined, b: string | undefined) {
  if (!a && !b) return 0;
  if (!a) return 1;
  if (!b) return -1;
  return a.localeCompare(b, "zh-Hans-CN", { numeric: true });
}

function entityDateSortValue(entity: EntitySummary, field: string) {
  return dateSortKey(entity.dates.find((item) => item.field === field)?.value);
}

function compareOptionalString(
  a: string | undefined,
  b: string | undefined,
  direction: SortDirection,
) {
  if (!a && !b) return 0;
  if (!a) return 1;
  if (!b) return -1;
  return compareString(a, b) * (direction === "asc" ? 1 : -1);
}
