import type { EntitySummary } from "@kizunashelf/core";
import type { Hono } from "hono";

import type { GetLibrary } from "../library-cache";
import {
  buildRelationFieldSummary,
  buildRelationTargetSummary,
  buildRelationTargets,
  outgoingRelations,
  relationFields,
  summaryById,
  targetKey,
} from "../relations";
import { clampNumber, countBy, sortEntities } from "../utils";

export function registerRelationRoutes(app: Hono, getLibrary: GetLibrary) {
  app.get("/api/relations", async (c) => {
    const library = await getLibrary();
    const sourceId = c.req.query("sourceId");
    const field = c.req.query("field");

    let relations = library.relations;
    if (sourceId) relations = relations.filter((relation) => relation.sourceId === sourceId);
    if (field) relations = relations.filter((relation) => relation.field === field);

    return c.json({
      items: relations,
      total: relations.length,
    });
  });

  app.get("/api/relation-groups", async (c) => {
    const library = await getLibrary();
    const groups = relationFields(library).map((field) => buildRelationFieldSummary(library, field));

    return c.json({
      generatedAt: library.generatedAt,
      fields: groups.filter((group) => group.edgeCount > 0),
    });
  });

  app.get("/api/relation-groups/:field", async (c) => {
    const library = await getLibrary();
    const field = c.req.param("field");
    const q = c.req.query("q")?.trim().toLocaleLowerCase();
    const pageSize = clampNumber(Number(c.req.query("pageSize") ?? 40), 1, 100);
    const requestedPage = clampNumber(Number(c.req.query("page") ?? 1), 1, Number.MAX_SAFE_INTEGER);
    const targets = buildRelationTargets(library, field).filter((target) => {
      if (!q) return true;
      return [target.targetTitle, target.targetId, target.targetTypeLabel]
        .filter(Boolean)
        .some((value) => value!.toLocaleLowerCase().includes(q));
    });
    const total = targets.length;
    const totalPages = Math.max(1, Math.ceil(total / pageSize));
    const page = Math.min(requestedPage, totalPages);
    const start = (page - 1) * pageSize;

    return c.json({
      generatedAt: library.generatedAt,
      field,
      edgeCount: outgoingRelations(library, field).length,
      uniqueTargets: buildRelationTargets(library, field).length,
      targets: targets.slice(start, start + pageSize),
      total,
      page,
      pageSize,
      totalPages,
    });
  });

  app.get("/api/relation-groups/:field/:target", async (c) => {
    const library = await getLibrary();
    const field = c.req.param("field");
    const target = c.req.param("target");
    const relations = outgoingRelations(library, field).filter(
      (relation) => relation.targetId === target || relation.targetTitle === target,
    );

    if (relations.length === 0) {
      return c.json({ error: "Relation target not found" }, 404);
    }

    const targetSummary = buildRelationTargetSummary(library, targetKey(relations[0]), relations);
    const entityById = summaryById(library);
    const groups = countBy(
      relations
        .map((relation) => entityById.get(relation.sourceId))
        .filter((entity): entity is EntitySummary => Boolean(entity)),
      (entity) => entity.typeLabel,
    ).map((group) => ({
      typeLabel: group.name,
      count: group.count,
      items: sortEntities(
        relations
          .map((relation) => entityById.get(relation.sourceId))
          .filter(
            (entity): entity is EntitySummary => entity !== undefined && entity.typeLabel === group.name,
          ),
        "title",
        "asc",
      ),
    }));

    return c.json({
      generatedAt: library.generatedAt,
      field,
      target: targetSummary,
      groups,
      total: relations.length,
    });
  });
}
