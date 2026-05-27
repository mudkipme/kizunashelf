import type { Relation } from "@/types/api";

const relationTypeLabels: Record<string, string> = {
  anime: "Anime",
  games: "Games",
  movie: "Movie",
  drama: "Drama",
  music: "Music",
  cd: "CD",
  event: "Event",
  books: "Books",
  "blu-ray": "Blu-Ray",
  artist: "Artist",
  characters: "Characters",
  franchise: "Franchise",
};

export function groupRelations(relations: Relation[]) {
  const groups = new Map<string, Relation[]>();
  for (const relation of relations) {
    const key =
      relation.direction === "in"
        ? `Backlinks · ${relationTypeLabel(relation.targetType)}`
        : relation.field;
    const items = groups.get(key) ?? [];
    items.push(relation);
    groups.set(key, items);
  }

  return [...groups.entries()].map(([field, items]) => ({ field, items }));
}

export function relationKey(relation: Relation) {
  return `${relation.field}-${relation.targetTitle}-${relation.direction}-${relation.targetId ?? ""}`;
}

export function relationFieldHref(field: string) {
  return `/relations/${encodeURIComponent(field)}`;
}

export function relationTargetHref(
  field: string,
  target: { targetId?: string | null; targetTitle: string },
) {
  return `/relations/${encodeURIComponent(field)}/${encodeURIComponent(
    target.targetId ?? target.targetTitle,
  )}`;
}

function relationTypeLabel(type: string | null | undefined) {
  if (!type) return "Unknown";
  return relationTypeLabels[type] ?? titleCase(type);
}

function titleCase(value: string) {
  return value
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((part) => `${part.slice(0, 1).toUpperCase()}${part.slice(1)}`)
    .join(" ");
}
