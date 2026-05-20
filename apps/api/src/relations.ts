import type { EntitySummary, Library, Relation } from "@kizunashelf/core";

import { compareString, countBy, sortEntities } from "./utils";

export function buildRelationHubs(library: Library) {
  const grouped = new Map<string, Relation[]>();
  for (const relation of outgoingRelations(library)) {
    const key = targetKey(relation);
    const items = grouped.get(key) ?? [];
    items.push(relation);
    grouped.set(key, items);
  }

  return [...grouped.entries()]
    .map(([key, relations]) => ({
      ...buildRelationTargetSummary(library, key, relations),
      fields: countBy(relations, (relation) => relation.field),
    }))
    .sort((a, b) => {
      if (a.count !== b.count) return b.count - a.count;
      return compareString(a.targetTitle, b.targetTitle);
    });
}

export function relationTypePairs(library: Library) {
  const byId = summaryById(library);
  return countBy(outgoingRelations(library), (relation) => {
    const source = byId.get(relation.sourceId);
    const targetLabel =
      relation.targetId && byId.get(relation.targetId)
        ? byId.get(relation.targetId)?.typeLabel
        : relation.targetType
          ? typeLabel(library, relation.targetType)
          : "Unresolved";
    return `${source?.typeLabel ?? "Unknown"} -> ${targetLabel ?? "Unknown"}`;
  });
}

export function relationFields(library: Library): string[] {
  const fields = new Set(library.config.relationshipFields);
  for (const relation of library.relations) {
    if (relation.direction === "out") fields.add(relation.field);
  }
  return [...fields].sort((a, b) => a.localeCompare(b, "zh-Hans-CN", { numeric: true }));
}

export function outgoingRelations(library: Library, field?: string) {
  return library.relations.filter(
    (relation) => relation.direction === "out" && (!field || relation.field === field),
  );
}

export function buildRelationFieldSummary(library: Library, field: string) {
  const relations = outgoingRelations(library, field);
  const targets = buildRelationTargets(library, field);
  const sources = new Set(relations.map((relation) => relation.sourceId));

  return {
    field,
    edgeCount: relations.length,
    sourceCount: sources.size,
    uniqueTargets: targets.length,
    resolvedTargets: targets.filter((target) => Boolean(target.targetId)).length,
    topTargets: targets.slice(0, 8),
  };
}

export function buildRelationTargets(library: Library, field: string) {
  const grouped = new Map<string, Relation[]>();
  for (const relation of outgoingRelations(library, field)) {
    const key = targetKey(relation);
    const items = grouped.get(key) ?? [];
    items.push(relation);
    grouped.set(key, items);
  }

  return [...grouped.entries()]
    .map(([key, relations]) => buildRelationTargetSummary(library, key, relations))
    .sort((a, b) => {
      if (a.count !== b.count) return b.count - a.count;
      return compareString(a.targetTitle, b.targetTitle);
    });
}

export function buildRelationTargetSummary(
  library: Library,
  key: string,
  relations: Relation[],
) {
  const entityById = summaryById(library);
  const first = relations[0];
  const targetEntity = first.targetId ? entityById.get(first.targetId) : undefined;
  const sources = relations
    .map((relation) => entityById.get(relation.sourceId))
    .filter((entity): entity is EntitySummary => Boolean(entity));

  return {
    key,
    targetTitle: targetEntity?.title ?? first.targetTitle,
    targetId: first.targetId,
    targetType: targetEntity?.type ?? first.targetType,
    targetTypeLabel: targetEntity?.typeLabel ?? typeLabel(library, first.targetType),
    count: relations.length,
    sourceTypes: countBy(sources, (entity) => entity.typeLabel),
    examples: sortEntities(sources, "title", "asc").slice(0, 5),
  };
}

export function targetKey(relation: { targetId?: string; targetTitle: string }) {
  return relation.targetId ?? relation.targetTitle;
}

export function summaryById(library: Library) {
  return new Map(library.summaries.map((entity) => [entity.id, entity]));
}

export function typeLabel(library: Library, type: string | undefined) {
  if (!type) return undefined;
  return library.config.types.find((item) => item.id === type)?.label ?? type;
}
