import type { Hono } from "hono";

import { buildAnalytics } from "../analytics";
import { buildHomeSection } from "../home";
import type { GetLibrary } from "../library-cache";
import { countBy, getStatusTrackedTypeIds } from "../utils";

export function registerSystemRoutes(app: Hono, getLibrary: GetLibrary) {
  app.get("/api/health", async (c) => {
    const library = await getLibrary();
    return c.json({
      ok: true,
      generatedAt: library.generatedAt,
      entityCount: library.entities.length,
      relationCount: library.relations.length,
    });
  });

  app.get("/api/config", async (c) => {
    const library = await getLibrary();
    return c.json({
      taxonomyRoot: library.config.taxonomyRoot,
      home: library.config.home,
      types: library.config.types.map((type) => ({
        id: type.id,
        label: type.label,
        path: type.path,
      })),
    });
  });

  app.get("/api/home", async (c) => {
    const library = await getLibrary();
    const sections = (library.config.home?.sections ?? []).map((section) =>
      buildHomeSection(library, section),
    );

    return c.json({
      generatedAt: library.generatedAt,
      title: library.config.home?.title ?? "Home",
      sections,
    });
  });

  app.get("/api/stats", async (c) => {
    const library = await getLibrary();
    const type = c.req.query("type");
    const summaries =
      type && type !== "all"
        ? library.summaries.filter((entity) => entity.type === type)
        : library.summaries;
    const ids = new Set(summaries.map((entity) => entity.id));
    const statusTrackedTypeIds = getStatusTrackedTypeIds(library);
    const statusSummaries = summaries.filter((entity) => statusTrackedTypeIds.has(entity.type));

    return c.json({
      generatedAt: library.generatedAt,
      total: summaries.length,
      relations: library.relations.filter((relation) => ids.has(relation.sourceId)).length,
      byType: library.config.types.map((type) => ({
        id: type.id,
        label: type.label,
        count: library.entities.filter((entity) => entity.type === type.id).length,
      })),
      dateFields:
        type && type !== "all"
          ? (library.config.types.find((item) => item.id === type)?.fields.date ?? [])
          : [],
      byStatus: countBy(statusSummaries, (entity) => entity.status ?? "Unknown"),
      topRelations: [...summaries]
        .sort((a, b) => b.relationCount - a.relationCount)
        .slice(0, 12),
    });
  });

  app.get("/api/analytics", async (c) => {
    const library = await getLibrary();
    return c.json(buildAnalytics(library));
  });
}
