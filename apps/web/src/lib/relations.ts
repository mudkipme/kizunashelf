import type { Relation } from "@/types/api";

export function groupRelations(relations: Relation[]) {
  const groups = new Map<string, Relation[]>();
  for (const relation of relations) {
    const key = relation.field;
    const items = groups.get(key) ?? [];
    items.push(relation);
    groups.set(key, items);
  }

  return [...groups.entries()].map(([field, items]) => ({ field, items }));
}

export function relationKey(relation: Relation) {
  return `${relation.sourceId}-${relation.field}-${relation.targetTitle}-${relation.direction}-${relation.targetId ?? ""}`;
}
