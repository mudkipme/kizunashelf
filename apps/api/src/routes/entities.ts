import type { Hono } from "hono";

import { buildEntityDates } from "../calendar";
import type { GetLibrary } from "../library-cache";
import { clampNumber, sortEntities } from "../utils";

export function registerEntityRoutes(app: Hono, getLibrary: GetLibrary) {
  app.get("/api/entities", async (c) => {
    const library = await getLibrary();
    const type = c.req.query("type");
    const status = c.req.query("status");
    const refs = c.req.query("refs");
    const cover = c.req.query("cover");
    const sort = c.req.query("sort") ?? "type";
    const direction = c.req.query("direction") === "desc" ? "desc" : "asc";
    const q = c.req.query("q")?.trim().toLocaleLowerCase();
    const relation = c.req.query("relation")?.trim();
    const pageSize = clampNumber(Number(c.req.query("pageSize") ?? 40), 1, 100);
    const requestedPage = clampNumber(Number(c.req.query("page") ?? 1), 1, Number.MAX_SAFE_INTEGER);

    let entities = library.summaries;
    if (type && type !== "all") entities = entities.filter((entity) => entity.type === type);
    if (status && status !== "all") {
      entities = entities.filter((entity) =>
        status === "Unknown" ? !entity.status : entity.status === status,
      );
    }
    if (refs === "with") {
      entities = entities.filter((entity) => Object.keys(entity.externalRefs).length > 0);
    }
    if (refs === "without") {
      entities = entities.filter((entity) => Object.keys(entity.externalRefs).length === 0);
    }
    if (cover === "with") entities = entities.filter((entity) => Boolean(entity.image));
    if (cover === "without") entities = entities.filter((entity) => !entity.image);
    if (q) {
      entities = entities.filter((entity) =>
        [entity.title, entity.subtitle, entity.summary, entity.basename, entity.path]
          .filter(Boolean)
          .some((value) => value!.toLocaleLowerCase().includes(q)),
      );
    }
    if (relation) {
      const ids = new Set(
        library.relations
          .filter((item) => item.targetTitle === relation || item.targetId === relation)
          .map((item) => item.sourceId),
      );
      entities = entities.filter((entity) => ids.has(entity.id));
    }

    entities = sortEntities(entities, sort, direction);

    const total = entities.length;
    const totalPages = Math.max(1, Math.ceil(total / pageSize));
    const page = Math.min(requestedPage, totalPages);
    const start = (page - 1) * pageSize;

    return c.json({
      items: entities.slice(start, start + pageSize),
      total,
      page,
      pageSize,
      totalPages,
    });
  });

  app.get("/api/entities/:id/dates", async (c) => {
    const library = await getLibrary();
    const id = c.req.param("id");
    const entity = library.entities.find((item) => item.id === id);

    if (!entity) {
      return c.json({ error: "Entity not found" }, 404);
    }

    return c.json(await buildEntityDates(library, entity));
  });

  app.get("/api/entities/:id", async (c) => {
    const library = await getLibrary();
    const id = c.req.param("id");
    const entity = library.entities.find((item) => item.id === id);

    if (!entity) {
      return c.json({ error: "Entity not found" }, 404);
    }

    return c.json({
      entity,
      relations: library.relations.filter((relation) => relation.sourceId === entity.id),
    });
  });
}
