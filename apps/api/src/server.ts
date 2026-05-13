import { serve } from "@hono/node-server";
import {
  loadConfig,
  readLibrary,
  type EntitySummary,
  type HomeSectionConfig,
  type Library,
} from "@kizunashelf/core";
import { readFile, stat } from "node:fs/promises";
import { extname, join, normalize, relative } from "node:path";
import { Hono } from "hono";
import { cors } from "hono/cors";
import { fileURLToPath } from "node:url";

const app = new Hono();
const defaultConfigPath = fileURLToPath(
  new URL("../../../config/kizunashelf.config.json", import.meta.url),
);
const configPath = process.env.KIZUNASHELF_CONFIG ?? defaultConfigPath;
const port = Number(process.env.PORT ?? 8787);
const hostname = process.env.HOST ?? "0.0.0.0";
const cacheTtlMs = Number(process.env.KIZUNASHELF_CACHE_TTL_MS ?? 10_000);
const webDistPath = fileURLToPath(new URL("../../web/dist", import.meta.url));
const serveStaticWeb = process.env.KIZUNASHELF_SERVE_WEB !== "false";

let cachedLibrary: Library | undefined;
let cachedAt = 0;

app.use(
  "*",
  cors({
    origin: ["http://localhost:5173", "http://127.0.0.1:5173"],
  }),
);

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

  return c.json({
    generatedAt: library.generatedAt,
    total: summaries.length,
    relations: library.relations.filter((relation) => ids.has(relation.sourceId)).length,
    byType: library.config.types.map((type) => ({
      id: type.id,
      label: type.label,
      count: library.entities.filter((entity) => entity.type === type.id).length,
    })),
    byStatus: countBy(summaries, (entity) => entity.status ?? "Unknown"),
    topRelations: [...summaries]
      .sort((a, b) => b.relationCount - a.relationCount)
      .slice(0, 12),
  });
});

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

if (serveStaticWeb) {
  app.get("*", async (c, next) => {
    if (c.req.path.startsWith("/api/")) return next();

    const response = await staticResponse(c.req.path);
    if (response) return response;

    return next();
  });
}

async function getLibrary(): Promise<Library> {
  if (cachedLibrary && Date.now() - cachedAt < cacheTtlMs) {
    return cachedLibrary;
  }

  const config = await loadConfig(configPath);
  cachedLibrary = await readLibrary(config);
  cachedAt = Date.now();
  return cachedLibrary;
}

function countBy<T>(items: T[], select: (item: T) => string): Array<{ name: string; count: number }> {
  const counts = new Map<string, number>();
  for (const item of items) {
    const key = select(item);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }

  return [...counts.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => b.count - a.count);
}

function clampNumber(value: number, min: number, max: number) {
  if (!Number.isFinite(value)) return min;
  return Math.floor(Math.min(max, Math.max(min, value)));
}

function sortEntities(
  entities: EntitySummary[],
  sort: string,
  direction: "asc" | "desc",
): EntitySummary[] {
  const multiplier = direction === "asc" ? 1 : -1;
  const sorted = [...entities].sort((a, b) => {
    if (sort === "title") return compareString(a.title, b.title) * multiplier;
    if (sort === "status") return compareString(a.status, b.status) * multiplier;
    if (sort === "date") return compareString(a.date, b.date) * multiplier;
    if (sort === "relations") return (a.relationCount - b.relationCount) * multiplier;
    if (sort === "path") return compareString(a.path, b.path) * multiplier;

    const typeCompare = compareString(a.typeLabel, b.typeLabel);
    if (typeCompare !== 0) return typeCompare * multiplier;
    return compareString(a.title, b.title) * multiplier;
  });

  return sorted;
}

function buildHomeSection(library: Library, section: HomeSectionConfig) {
  const type = library.config.types.find((item) => item.id === section.type);
  const statuses = normalizeStatuses(section.status);
  const limit = clampNumber(Number(section.limit ?? 12), 1, 48);
  const direction = section.direction === "desc" ? "desc" : "asc";
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

function compareString(a: string | undefined, b: string | undefined) {
  if (!a && !b) return 0;
  if (!a) return 1;
  if (!b) return -1;
  return a.localeCompare(b, "zh-Hans-CN", { numeric: true });
}

async function staticResponse(pathname: string): Promise<Response | undefined> {
  const requestedPath = pathname === "/" ? "/index.html" : decodeURIComponent(pathname);
  const filePath = resolveStaticPath(requestedPath);
  const file = filePath ? await readStaticFile(filePath) : undefined;

  if (file) return file;

  const fallback = await readStaticFile(join(webDistPath, "index.html"));
  return fallback;
}

function resolveStaticPath(pathname: string): string | undefined {
  const normalized = normalize(pathname).replace(/^(\.\.(\/|\\|$))+/, "");
  const filePath = join(webDistPath, normalized);
  const relativePath = relative(webDistPath, filePath);

  if (relativePath.startsWith("..") || relativePath === "") return undefined;
  return filePath;
}

async function readStaticFile(filePath: string): Promise<Response | undefined> {
  try {
    const info = await stat(filePath);
    if (!info.isFile()) return undefined;
    const body = await readFile(filePath);
    return new Response(body, {
      headers: {
        "content-type": contentType(filePath),
      },
    });
  } catch {
    return undefined;
  }
}

function contentType(filePath: string) {
  const extension = extname(filePath);
  if (extension === ".html") return "text/html; charset=utf-8";
  if (extension === ".js") return "text/javascript; charset=utf-8";
  if (extension === ".css") return "text/css; charset=utf-8";
  if (extension === ".json") return "application/json; charset=utf-8";
  if (extension === ".svg") return "image/svg+xml";
  if (extension === ".png") return "image/png";
  if (extension === ".jpg" || extension === ".jpeg") return "image/jpeg";
  if (extension === ".webp") return "image/webp";
  if (extension === ".ico") return "image/x-icon";
  return "application/octet-stream";
}

serve(
  {
    fetch: app.fetch,
    hostname,
    port,
  },
  (info) => {
    console.log(`KizunaShelf listening on http://${info.address}:${info.port}`);
  },
);
