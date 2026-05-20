import { readdir, readFile } from "node:fs/promises";
import { basename, join, relative } from "node:path";

import YAML from "yaml";

import type {
  Entity,
  EntitySummary,
  EntityTypeConfig,
  KizunaConfig,
  Library,
  Relation,
} from "./types";

type ParsedMarkdown = {
  frontmatter: Record<string, unknown>;
  body: string;
};

const summaryHeadingPattern = /^##\s+(摘要|概览|简介|Summary)\s*$/im;
const wikilinkPattern = /\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]/g;
const defaultReadConcurrency = 32;

export async function readLibrary(config: KizunaConfig): Promise<Library> {
  const entities = await readEntities(config);
  const relations = buildRelations(config, entities);
  const relationCountById = new Map<string, number>();

  for (const relation of relations) {
    relationCountById.set(
      relation.sourceId,
      (relationCountById.get(relation.sourceId) ?? 0) + 1,
    );
    if (relation.targetId) {
      relationCountById.set(
        relation.targetId,
        (relationCountById.get(relation.targetId) ?? 0) + 1,
      );
    }
  }

  const summaries = entities.map((entity) => ({
    ...toSummary(entity),
    relationCount: relationCountById.get(entity.id) ?? 0,
  }));
  const summaryById = new Map(summaries.map((summary) => [summary.id, summary]));

  return {
    config,
    entities: entities.map((entity) => ({
      ...entity,
      relationCount: summaryById.get(entity.id)?.relationCount ?? 0,
    })),
    summaries,
    relations,
    generatedAt: new Date().toISOString(),
  };
}

function toSummary(entity: Entity): EntitySummary {
  return {
    id: entity.id,
    type: entity.type,
    typeLabel: entity.typeLabel,
    title: entity.title,
    subtitle: entity.subtitle,
    status: entity.status,
    dates: entity.dates,
    image: entity.image,
    summary: entity.summary,
    path: entity.path,
    basename: entity.basename,
    externalRefs: entity.externalRefs,
    relationCount: entity.relationCount,
  };
}

async function readEntities(config: KizunaConfig): Promise<Entity[]> {
  const readLimit = createLimiter(normalizeReadConcurrency(config.readConcurrency));
  const nested = await Promise.all(
    config.types.map((typeConfig) => readEntitiesForType(config, typeConfig, readLimit)),
  );

  return nested.flat().sort((a, b) => {
    if (a.typeLabel !== b.typeLabel) return a.typeLabel.localeCompare(b.typeLabel);
    return a.title.localeCompare(b.title, "zh-Hans-CN");
  });
}

async function readEntitiesForType(
  config: KizunaConfig,
  typeConfig: EntityTypeConfig,
  readLimit: <T>(task: () => Promise<T>) => Promise<T>,
): Promise<Entity[]> {
  const absoluteDir = join(config.vaultRoot, config.taxonomyRoot, typeConfig.path);
  let entries: string[];

  try {
    const dirents = await readdir(absoluteDir, { withFileTypes: true });
    entries = dirents
      .filter((dirent) => dirent.isFile() && dirent.name.endsWith(".md"))
      .map((dirent) => dirent.name);
  } catch {
    return [];
  }

  return Promise.all(
    entries.map((entry) => readLimit(async () => {
      const absolutePath = join(absoluteDir, entry);
      const raw = await readFile(absolutePath, "utf8");
      const parsed = parseMarkdown(raw);
      const noteBasename = basename(entry, ".md");
      const title = firstString(parsed.frontmatter, typeConfig.fields.title) ?? noteBasename;
      const relativePath = relative(config.vaultRoot, absolutePath);

      return {
        id: `${typeConfig.id}:${noteBasename}`,
        type: typeConfig.id,
        typeLabel: typeConfig.label,
        title,
        subtitle: firstString(parsed.frontmatter, typeConfig.fields.subtitle),
        status: firstString(parsed.frontmatter, typeConfig.fields.status),
        dates: dateValues(parsed.frontmatter, typeConfig.fields.date),
        image: firstString(parsed.frontmatter, typeConfig.fields.image),
        summary: extractSummary(parsed.body),
        path: relativePath,
        basename: noteBasename,
        externalRefs: externalRefs(parsed.frontmatter, typeConfig.fields.externalRefs),
        relationCount: 0,
        frontmatter: parsed.frontmatter,
        body: parsed.body,
        raw,
      };
    })),
  );
}

function normalizeReadConcurrency(value: number | undefined) {
  if (value === undefined || !Number.isFinite(value)) return defaultReadConcurrency;
  return Math.floor(Math.min(256, Math.max(1, value)));
}

function createLimiter(concurrency: number) {
  let active = 0;
  const queue: Array<() => void> = [];

  function runNext() {
    const next = queue.shift();
    if (!next || active >= concurrency) return;
    active += 1;
    next();
  }

  return async function limit<T>(task: () => Promise<T>): Promise<T> {
    if (active >= concurrency) {
      await new Promise<void>((resolve) => queue.push(resolve));
    } else {
      active += 1;
    }

    try {
      return await task();
    } finally {
      active -= 1;
      runNext();
    }
  };
}

function parseMarkdown(raw: string): ParsedMarkdown {
  if (!raw.startsWith("---\n")) {
    return { frontmatter: {}, body: raw.trim() };
  }

  const end = raw.indexOf("\n---", 4);
  if (end === -1) {
    return { frontmatter: {}, body: raw.trim() };
  }

  const yamlText = raw.slice(4, end);
  const body = raw.slice(end + 4).trim();

  try {
    const parsed = YAML.parse(yamlText);
    return {
      frontmatter: isRecord(parsed) ? parsed : {},
      body,
    };
  } catch {
    return { frontmatter: {}, body };
  }
}

function externalRefs(
  frontmatter: Record<string, unknown>,
  keys: string[] | undefined,
): Record<string, string> {
  const refs: Record<string, string> = {};

  for (const key of keys ?? []) {
    const value = firstString(frontmatter, [key]);
    if (value) refs[key] = value;
  }

  return refs;
}

function dateValues(
  frontmatter: Record<string, unknown>,
  keys: string[] | undefined,
): Array<{ field: string; value: string }> {
  const dates: Array<{ field: string; value: string }> = [];

  for (const field of keys ?? []) {
    for (const value of normalizeValues(frontmatter[field])) {
      dates.push({ field, value });
    }
  }

  return dates;
}

function firstString(
  frontmatter: Record<string, unknown>,
  keys: string[] | undefined,
): string | undefined {
  for (const key of keys ?? []) {
    const value = normalizeValue(frontmatter[key]);
    if (value) return value;
  }

  return undefined;
}

function normalizeValue(value: unknown): string | undefined {
  if (value === undefined || value === null || value === "") return undefined;
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value === "number") return String(value);
  if (value instanceof Date) return value.toISOString().slice(0, 10);
  if (Array.isArray(value)) {
    const normalized = value.map((item) => normalizeValue(item)).filter(Boolean);
    return normalized.length > 0 ? normalized.join(", ") : undefined;
  }
  if (typeof value === "string") return stripWikilink(value);
  return undefined;
}

function normalizeValues(value: unknown): string[] {
  if (value === undefined || value === null || value === "") return [];
  if (Array.isArray(value)) return value.flatMap((item) => normalizeValues(item));
  const normalized = normalizeValue(value);
  return normalized ? [normalized] : [];
}

function stripWikilink(value: string): string {
  const match = /^\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|([^\]]+))?\]\]$/.exec(value.trim());
  return match?.[2] ?? match?.[1] ?? value;
}

function extractSummary(body: string): string | undefined {
  const heading = summaryHeadingPattern.exec(body);
  const source = heading ? body.slice((heading.index ?? 0) + heading[0].length) : body;
  const cleaned = source
    .split(/\n##\s+/)[0]
    .replace(/```[\s\S]*?```/g, "")
    .replace(/!\[[^\]]*\]\([^)]+\)/g, "")
    .replace(/\[[^\]]+\]\([^)]+\)/g, "")
    .replace(wikilinkPattern, "$1")
    .split("\n")
    .map((line) => line.replace(/^[-*>#\s]+/, "").trim())
    .filter(Boolean)
    .join(" ");

  if (!cleaned) return undefined;
  return cleaned.length > 220 ? `${cleaned.slice(0, 220)}...` : cleaned;
}

function buildRelations(config: KizunaConfig, entities: Entity[]): Relation[] {
  const byBasename = new Map<string, Entity[]>();
  const relations: Relation[] = [];

  for (const entity of entities) {
    const existing = byBasename.get(entity.basename) ?? [];
    existing.push(entity);
    byBasename.set(entity.basename, existing);
  }

  for (const entity of entities) {
    for (const field of relationFields(config, entity.type)) {
      for (const targetTitle of relationValues(entity.frontmatter[field])) {
        const target = findTarget(targetTitle, byBasename);
        relations.push({
          sourceId: entity.id,
          targetId: target?.id,
          targetTitle,
          targetType: target?.type,
          field,
          direction: "out",
        });

        if (target) {
          relations.push({
            sourceId: target.id,
            targetId: entity.id,
            targetTitle: entity.title,
            targetType: entity.type,
            field,
            direction: "in",
          });
        }
      }
    }

    for (const targetTitle of bodyWikilinks(entity.body)) {
      const target = findTarget(targetTitle, byBasename);
      if (!target || target.id === entity.id) continue;
      relations.push({
        sourceId: entity.id,
        targetId: target.id,
        targetTitle,
        targetType: target.type,
        field: "body",
        direction: "out",
      });
    }
  }

  return dedupeRelations(relations);
}

function relationFields(config: KizunaConfig, entityType: string): string[] {
  const typeConfig = config.types.find((type) => type.id === entityType);
  return [...new Set([...(config.relationshipFields ?? []), ...(typeConfig?.fields.relations ?? [])])];
}

function relationValues(value: unknown): string[] {
  if (Array.isArray(value)) return value.flatMap((item) => relationValues(item));
  if (typeof value !== "string") return [];

  const matches = [...value.matchAll(wikilinkPattern)].map((match) => match[1]);
  if (matches.length > 0) return matches.map((match) => match.trim()).filter(Boolean);

  return value.trim() ? [stripWikilink(value.trim())] : [];
}

function bodyWikilinks(body: string): string[] {
  return [...body.matchAll(wikilinkPattern)].map((match) => match[1].trim()).filter(Boolean);
}

function findTarget(targetTitle: string, byBasename: Map<string, Entity[]>): Entity | undefined {
  const candidates = byBasename.get(targetTitle);
  if (!candidates || candidates.length === 0) return undefined;
  return candidates.find((candidate) => candidate.type === "franchise") ?? candidates[0];
}

function dedupeRelations(relations: Relation[]): Relation[] {
  const seen = new Set<string>();
  return relations.filter((relation) => {
    const key = [
      relation.sourceId,
      relation.targetId,
      relation.targetTitle,
      relation.field,
      relation.direction,
    ].join("\u0000");
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
