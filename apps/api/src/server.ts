import { serve } from "@hono/node-server";
import {
  loadConfig,
  readLibrary,
  type EntitySummary,
  type HomeSectionConfig,
  type Library,
} from "@kizunashelf/core";
import { readdir, readFile, stat } from "node:fs/promises";
import { basename, extname, join, normalize, relative } from "node:path";
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

app.get("/api/analytics", async (c) => {
  const library = await getLibrary();
  return c.json(buildAnalytics(library));
});

app.get("/api/calendar", async (c) => {
  const library = await getLibrary();
  const now = new Date();
  const year = clampNumber(Number(c.req.query("year") ?? now.getFullYear()), 1970, 2100);
  const month = clampNumber(Number(c.req.query("month") ?? now.getMonth() + 1), 1, 12);
  const type = c.req.query("type");
  const source = c.req.query("source");

  return c.json(
    await buildCalendar(library, {
      year,
      month,
      type: type && type !== "all" ? type : undefined,
      source: source === "taxonomy" || source === "daily-note" ? source : "all",
    }),
  );
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

function buildAnalytics(library: Library) {
  const summaries = library.summaries;
  const outgoing = outgoingRelations(library);
  const unresolved = outgoing.filter((relation) => !relation.targetId);
  const dated = summaries
    .map((entity) => ({ entity, date: parseEntityDate(entity.date) }))
    .filter((item): item is { entity: EntitySummary; date: ParsedEntityDate } =>
      Boolean(item.date),
    );
  const withCover = summaries.filter((entity) => Boolean(entity.image));
  const withRefs = summaries.filter((entity) => Object.keys(entity.externalRefs).length > 0);
  const withSummary = summaries.filter((entity) => Boolean(entity.summary));
  const connected = summaries.filter((entity) => entity.relationCount > 0);

  return {
    generatedAt: library.generatedAt,
    totals: {
      entities: summaries.length,
      relations: outgoing.length,
      unresolvedRelations: unresolved.length,
      datedEntities: dated.length,
      connectedEntities: connected.length,
    },
    distributions: {
      byType: library.config.types.map((type) => ({
        id: type.id,
        label: type.label,
        count: summaries.filter((entity) => entity.type === type.id).length,
      })),
      byStatus: countBy(summaries, (entity) => entity.status ?? "Unknown"),
      byRelationField: countBy(outgoing, (relation) => relation.field).slice(0, 16),
      bySourceTargetType: relationTypePairs(library).slice(0, 16),
    },
    coverage: [
      buildCoverageMetric("Cover", withCover.length, summaries),
      buildCoverageMetric("External refs", withRefs.length, summaries),
      buildCoverageMetric("Summary", withSummary.length, summaries),
      buildCoverageMetric("Relations", connected.length, summaries),
      buildCoverageMetric(
        "Resolved relation targets",
        outgoing.length - unresolved.length,
        outgoing,
      ),
    ],
    timeline: buildTimeline(dated),
    relations: {
      topFields: relationFields(library)
        .map((field) => buildRelationFieldSummary(library, field))
        .filter((field) => field.edgeCount > 0)
        .slice(0, 12),
      topTargets: buildRelationHubs(library).slice(0, 12),
      unresolved: {
        count: unresolved.length,
        examples: unresolved.slice(0, 12),
      },
    },
    dataQuality: {
      missingCover: summaries.filter((entity) => !entity.image).slice(0, 12),
      missingExternalRefs: summaries
        .filter((entity) => Object.keys(entity.externalRefs).length === 0)
        .slice(0, 12),
      missingSummary: summaries.filter((entity) => !entity.summary).slice(0, 12),
      isolated: summaries.filter((entity) => entity.relationCount === 0).slice(0, 12),
    },
  };
}

type ParsedEntityDate = {
  year: number;
  month?: number;
  season?: string;
};

const seasonOrder = new Map([
  ["冬季", 0],
  ["春季", 1],
  ["夏季", 2],
  ["秋季", 3],
]);

function parseEntityDate(value: string | undefined): ParsedEntityDate | undefined {
  if (!value) return undefined;
  const year = value.match(/\b(19|20)\d{2}\b/)?.[0] ?? value.match(/(19|20)\d{2}年/)?.[0];
  if (!year) return undefined;
  const parsedYear = Number(year.slice(0, 4));
  const month = value.match(/\b(19|20)\d{2}[-/.](\d{1,2})/)?.[2];
  const season = value.match(/年(春季|夏季|秋季|冬季)/)?.[1];

  return {
    year: parsedYear,
    month: month ? clampNumber(Number(month), 1, 12) : undefined,
    season,
  };
}

function buildCoverageMetric<T>(name: string, count: number, items: T[]) {
  const total = items.length;
  return {
    name,
    count,
    missing: total - count,
    total,
    percent: total === 0 ? 0 : Math.round((count / total) * 100),
  };
}

function buildTimeline(dated: Array<{ entity: EntitySummary; date: ParsedEntityDate }>) {
  const byYear = new Map<number, EntitySummary[]>();
  const bySeason = new Map<string, { year: number; season: string; entities: EntitySummary[] }>();
  const byMonth = new Map<string, EntitySummary[]>();

  for (const item of dated) {
    const yearItems = byYear.get(item.date.year) ?? [];
    yearItems.push(item.entity);
    byYear.set(item.date.year, yearItems);

    if (item.date.season) {
      const key = `${item.date.year} ${item.date.season}`;
      const seasonItems =
        bySeason.get(key) ?? {
          year: item.date.year,
          season: item.date.season,
          entities: [],
        };
      seasonItems.entities.push(item.entity);
      bySeason.set(key, seasonItems);
    }

    if (item.date.month) {
      const key = `${item.date.year}-${String(item.date.month).padStart(2, "0")}`;
      const monthItems = byMonth.get(key) ?? [];
      monthItems.push(item.entity);
      byMonth.set(key, monthItems);
    }
  }

  const years = [...byYear.entries()]
    .sort((a, b) => b[0] - a[0])
    .map(([year, entities]) => ({
      year,
      count: entities.length,
      byType: countBy(entities, (entity) => entity.typeLabel),
      examples: sortEntities(entities, "date", "desc").slice(0, 6),
    }));

  return {
    totalDated: dated.length,
    years,
    seasons: [...bySeason.values()]
      .map((item) => ({
        name: `${item.year} ${item.season}`,
        count: item.entities.length,
        year: item.year,
        season: item.season,
      }))
      .sort((a, b) => {
        if (a.year !== b.year) return b.year - a.year;
        return (seasonOrder.get(b.season) ?? Number.MIN_SAFE_INTEGER) -
          (seasonOrder.get(a.season) ?? Number.MIN_SAFE_INTEGER);
      })
      .map(({ name, count }) => ({ name, count }))
      .slice(0, 12),
    months: [...byMonth.entries()]
      .map(([name, entities]) => ({ name, count: entities.length }))
      .sort((a, b) => compareString(b.name, a.name))
      .slice(0, 18),
  };
}

type CalendarSource = "all" | "taxonomy" | "daily-note";

type CalendarBuildOptions = {
  year: number;
  month: number;
  type?: string;
  source: CalendarSource;
};

type CalendarSnippet = {
  text: string;
  heading?: string;
  line: number;
};

type CalendarEntry = {
  id: string;
  date: string;
  source: Exclude<CalendarSource, "all">;
  entity: EntitySummary;
  rawDate?: string;
  notePath?: string;
  snippets?: CalendarSnippet[];
};

const dailyNoteDatePattern = /^(?<date>\d{4}-\d{2}-\d{2})\.md$/;
const exactDatePattern =
  /\b((?:19|20)\d{2})[-/.](\d{1,2})[-/.](\d{1,2})\b|((?:19|20)\d{2})年(\d{1,2})月(\d{1,2})日/;
const wikilinkPattern = /!?\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|([^\]]+))?\]\]/g;

async function buildCalendar(library: Library, options: CalendarBuildOptions) {
  const entries = [
    ...(options.source === "daily-note" ? [] : taxonomyCalendarEntries(library, options)),
    ...(options.source === "taxonomy" ? [] : await dailyNoteCalendarEntries(library, options)),
  ].sort(compareCalendarEntries);
  const days = calendarDays(options.year, options.month, entries);

  return {
    generatedAt: library.generatedAt,
    year: options.year,
    month: options.month,
    filters: {
      type: options.type,
      source: options.source,
    },
    totals: {
      entries: entries.length,
      taxonomy: entries.filter((entry) => entry.source === "taxonomy").length,
      dailyNotes: entries.filter((entry) => entry.source === "daily-note").length,
      daysWithEntries: days.filter((day) => day.entries.length > 0).length,
    },
    days,
  };
}

function taxonomyCalendarEntries(library: Library, options: CalendarBuildOptions): CalendarEntry[] {
  return library.summaries
    .filter((entity) => !options.type || entity.type === options.type)
    .map((entity) => ({ entity, date: parseExactDate(entity.date) }))
    .filter((item): item is { entity: EntitySummary; date: string } => {
      if (!item.date) return false;
      return isInMonth(item.date, options.year, options.month);
    })
    .map(({ entity, date }) => ({
      id: `taxonomy:${date}:${entity.id}`,
      date,
      source: "taxonomy",
      entity,
      rawDate: entity.date,
    }));
}

async function dailyNoteCalendarEntries(
  library: Library,
  options: CalendarBuildOptions,
): Promise<CalendarEntry[]> {
  const dailyFiles = await dailyNoteFiles(library, options.year, options.month);
  const byBasename = entityBasenameIndex(library);
  const grouped = new Map<string, CalendarEntry>();
  const snippetMaxLength = clampNumber(
    Number(library.config.dailyNotes?.snippetMaxLength ?? 260),
    80,
    600,
  );

  for (const file of dailyFiles) {
    const raw = await readFile(file.absolutePath, "utf8");
    const blocks = mentionBlocks(stripFrontmatter(raw));

    for (const block of blocks) {
      const links = [...block.text.matchAll(wikilinkPattern)];
      for (const link of links) {
        const target = link[1];
        if (!target) continue;
        const entity = findEntityForWikilink(target, library, byBasename);
        if (!entity || (options.type && entity.type !== options.type)) continue;

        const key = `daily-note:${file.date}:${entity.id}`;
        const entry =
          grouped.get(key) ??
          ({
            id: key,
            date: file.date,
            source: "daily-note",
            entity,
            notePath: file.relativePath,
            snippets: [],
          } satisfies CalendarEntry);
        const snippet = {
          text: cleanMentionSnippet(block.text, snippetMaxLength),
          heading: block.heading,
          line: block.line,
        };
        if (!entry.snippets?.some((item) => item.text === snippet.text)) {
          entry.snippets?.push(snippet);
        }
        grouped.set(key, entry);
      }
    }
  }

  return [...grouped.values()].map((entry) => ({
    ...entry,
    snippets: entry.snippets?.slice(0, 5),
  }));
}

async function dailyNoteFiles(library: Library, year: number, month: number) {
  const paths = library.config.dailyNotes?.paths?.length
    ? library.config.dailyNotes.paths
    : ["Daily Notes"];
  const pattern = safeRegExp(library.config.dailyNotes?.datePattern) ?? dailyNoteDatePattern;
  const files = (
    await Promise.all(paths.map((path) => walkMarkdownFiles(join(library.config.vaultRoot, path))))
  ).flat();

  return files
    .map((absolutePath) => {
      const relativePath = relative(library.config.vaultRoot, absolutePath);
      const date = dailyNoteDate(relativePath, pattern) ?? dailyNoteDate(basename(absolutePath), pattern);
      return date ? { absolutePath, relativePath, date } : undefined;
    })
    .filter(
      (
        item,
      ): item is {
        absolutePath: string;
        relativePath: string;
        date: string;
      } => {
        if (!item) return false;
        return isInMonth(item.date, year, month);
      },
    );
}

async function walkMarkdownFiles(root: string): Promise<string[]> {
  try {
    const dirents = await readdir(root, { withFileTypes: true });
    const nested = await Promise.all(
      dirents.map(async (dirent) => {
        const path = join(root, dirent.name);
        if (dirent.isDirectory()) return walkMarkdownFiles(path);
        if (dirent.isFile() && dirent.name.endsWith(".md")) return [path];
        return [];
      }),
    );

    return nested.flat();
  } catch {
    return [];
  }
}

function dailyNoteDate(path: string, pattern: RegExp) {
  const match = pattern.exec(path);
  if (!match) return undefined;
  const date = match.groups?.date ?? match[1];
  return parseExactDate(date);
}

function safeRegExp(value: string | undefined) {
  if (!value) return undefined;
  try {
    return new RegExp(value);
  } catch {
    return undefined;
  }
}

function parseExactDate(value: string | undefined): string | undefined {
  if (!value) return undefined;
  const match = exactDatePattern.exec(value);
  if (!match) return undefined;

  const year = Number(match[1] ?? match[4]);
  const month = Number(match[2] ?? match[5]);
  const day = Number(match[3] ?? match[6]);
  return normalizeDate(year, month, day);
}

function normalizeDate(year: number, month: number, day: number) {
  if (!Number.isInteger(year) || !Number.isInteger(month) || !Number.isInteger(day)) {
    return undefined;
  }
  const date = new Date(Date.UTC(year, month - 1, day));
  if (
    date.getUTCFullYear() !== year ||
    date.getUTCMonth() !== month - 1 ||
    date.getUTCDate() !== day
  ) {
    return undefined;
  }

  return `${year}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

function isInMonth(date: string, year: number, month: number) {
  return date.startsWith(`${year}-${String(month).padStart(2, "0")}-`);
}

function calendarDays(year: number, month: number, entries: CalendarEntry[]) {
  const count = new Date(Date.UTC(year, month, 0)).getUTCDate();
  return Array.from({ length: count }, (_, index) => {
    const date = normalizeDate(year, month, index + 1)!;
    const dayEntries = entries.filter((entry) => entry.date === date);
    return {
      date,
      entries: dayEntries,
      counts: {
        total: dayEntries.length,
        taxonomy: dayEntries.filter((entry) => entry.source === "taxonomy").length,
        dailyNotes: dayEntries.filter((entry) => entry.source === "daily-note").length,
      },
    };
  });
}

function compareCalendarEntries(a: CalendarEntry, b: CalendarEntry) {
  const dateCompare = compareString(a.date, b.date);
  if (dateCompare !== 0) return dateCompare;
  if (a.source !== b.source) return a.source === "taxonomy" ? -1 : 1;
  const typeCompare = compareString(a.entity.typeLabel, b.entity.typeLabel);
  if (typeCompare !== 0) return typeCompare;
  return compareString(a.entity.title, b.entity.title);
}

function entityBasenameIndex(library: Library) {
  const byBasename = new Map<string, EntitySummary[]>();
  for (const entity of library.summaries) {
    const key = normalizeWikilinkTarget(entity.basename);
    const items = byBasename.get(key) ?? [];
    items.push(entity);
    byBasename.set(key, items);
  }
  return byBasename;
}

function findEntityForWikilink(
  target: string,
  library: Library,
  byBasename: Map<string, EntitySummary[]>,
) {
  const normalized = normalizeWikilinkTarget(target);
  const candidates = byBasename.get(normalized);
  if (!candidates || candidates.length === 0) return undefined;

  const pathParts = target.split("/").map((part) => part.trim()).filter(Boolean);
  const typePath = pathParts.length > 1 ? pathParts.at(-2) : undefined;
  const type = library.config.types.find((item) => item.path === typePath);
  if (type) {
    const typed = candidates.find((candidate) => candidate.type === type.id);
    if (typed) return typed;
  }

  return candidates.find((candidate) => candidate.type === "franchise") ?? candidates[0];
}

function normalizeWikilinkTarget(target: string) {
  return target.split("/").at(-1)?.trim().toLocaleLowerCase() ?? target.trim().toLocaleLowerCase();
}

type MarkdownMentionBlock = {
  text: string;
  heading?: string;
  line: number;
};

function stripFrontmatter(raw: string) {
  if (!raw.startsWith("---\n")) return raw;
  const end = raw.indexOf("\n---", 4);
  return end === -1 ? raw : raw.slice(end + 4);
}

function mentionBlocks(markdown: string): MarkdownMentionBlock[] {
  const lines = markdown.split(/\r?\n/);
  const blocks: MarkdownMentionBlock[] = [];
  let heading: string | undefined;
  let paragraph: Array<{ text: string; line: number }> = [];
  let inFence = false;

  function pushBlock(text: string, line: number) {
    if (wikilinkPattern.test(text)) {
      blocks.push({ text, heading, line });
    }
    wikilinkPattern.lastIndex = 0;
  }

  function flushParagraph() {
    if (paragraph.length === 0) return;
    pushBlock(
      paragraph.map((item) => item.text).join(" ").trim(),
      paragraph[0].line,
    );
    paragraph = [];
  }

  lines.forEach((line, index) => {
    const lineNumber = index + 1;
    const trimmed = line.trim();

    if (/^(```|~~~)/.test(trimmed)) {
      flushParagraph();
      inFence = !inFence;
      return;
    }
    if (inFence) return;

    const headingMatch = /^(#{1,6})\s+(.+)$/.exec(trimmed);
    if (headingMatch) {
      flushParagraph();
      heading = cleanMentionSnippet(headingMatch[2], 120);
      pushBlock(trimmed, lineNumber);
      return;
    }

    if (!trimmed) {
      flushParagraph();
      return;
    }

    if (/^([-*+]|\d+\.)\s+/.test(trimmed) || /^>\s+/.test(trimmed) || trimmed.includes("|")) {
      flushParagraph();
      pushBlock(trimmed, lineNumber);
      return;
    }

    paragraph.push({ text: trimmed, line: lineNumber });
  });

  flushParagraph();
  return blocks;
}

function cleanMentionSnippet(text: string, maxLength: number) {
  const cleaned = text
    .replace(wikilinkPattern, (_, target: string, alias: string | undefined) => alias ?? target)
    .replace(/!\[[^\]]*]\([^)]+\)/g, "")
    .replace(/\[([^\]]+)]\([^)]+\)/g, "$1")
    .replace(/^#{1,6}\s+/, "")
    .replace(/^>\s+/, "")
    .replace(/^([-*+]|\d+\.)\s+/, "")
    .replace(/\s+/g, " ")
    .trim();

  if (cleaned.length <= maxLength) return cleaned;
  return `${cleaned.slice(0, maxLength - 3).trim()}...`;
}

function buildRelationHubs(library: Library) {
  const grouped = new Map<string, typeof library.relations>();
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

function relationTypePairs(library: Library) {
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

function relationFields(library: Library): string[] {
  const fields = new Set(library.config.relationshipFields);
  for (const relation of library.relations) {
    if (relation.direction === "out") fields.add(relation.field);
  }
  return [...fields].sort((a, b) => a.localeCompare(b, "zh-Hans-CN", { numeric: true }));
}

function outgoingRelations(library: Library, field?: string) {
  return library.relations.filter(
    (relation) => relation.direction === "out" && (!field || relation.field === field),
  );
}

function buildRelationFieldSummary(library: Library, field: string) {
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

function buildRelationTargets(library: Library, field: string) {
  const grouped = new Map<string, typeof library.relations>();
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

function buildRelationTargetSummary(
  library: Library,
  key: string,
  relations: typeof library.relations,
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

function targetKey(relation: { targetId?: string; targetTitle: string }) {
  return relation.targetId ?? relation.targetTitle;
}

function summaryById(library: Library) {
  return new Map(library.summaries.map((entity) => [entity.id, entity]));
}

function typeLabel(library: Library, type: string | undefined) {
  if (!type) return undefined;
  return library.config.types.find((item) => item.id === type)?.label ?? type;
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
