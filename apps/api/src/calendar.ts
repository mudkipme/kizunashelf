import type { EntitySummary, Library } from "@kizunashelf/core";
import { readdir, readFile } from "node:fs/promises";
import { basename, join, relative } from "node:path";

import { isInMonth, normalizeDate, parseExactDate } from "./dates";
import { summaryById } from "./relations";
import { clampNumber, compareString } from "./utils";

export type CalendarSource = "all" | "taxonomy" | "daily-note";

export type CalendarBuildOptions = {
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
  dateField?: string;
  rawDate?: string;
  notePath?: string;
  snippets?: CalendarSnippet[];
};

type EntityDateMetadataEntry = {
  id: string;
  field: string;
  value: string;
  date?: string;
};

type EntityDateDailyNoteEntry = {
  id: string;
  date: string;
  notePath: string;
  snippets: CalendarSnippet[];
};

const dailyNoteDatePattern = /^(?<date>\d{4}-\d{2}-\d{2})\.md$/;
const wikilinkPattern = /!?\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|([^\]]+))?\]\]/g;

export async function buildCalendar(library: Library, options: CalendarBuildOptions) {
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

export async function buildEntityDates(
  library: Library,
  entity: Library["entities"][number],
) {
  const metadata = metadataDateEntries(library, entity);
  const dailyNotes = await entityDailyNoteEntries(library, entity);

  return {
    generatedAt: library.generatedAt,
    entityId: entity.id,
    totals: {
      metadata: metadata.length,
      dailyNotes: dailyNotes.length,
      snippets: dailyNotes.reduce((total, item) => total + item.snippets.length, 0),
    },
    metadata,
    dailyNotes,
  };
}

function taxonomyCalendarEntries(library: Library, options: CalendarBuildOptions): CalendarEntry[] {
  const summaries = summaryById(library);

  return library.entities
    .filter((entity) => !options.type || entity.type === options.type)
    .flatMap((entity) =>
      metadataDateEntries(library, entity)
        .filter((item) => item.date && isInMonth(item.date, options.year, options.month))
        .map((item) => ({
          id: `taxonomy:${item.field}:${item.date}:${entity.id}`,
          date: item.date!,
          source: "taxonomy" as const,
          entity: summaries.get(entity.id) ?? entity,
          dateField: item.field,
          rawDate: item.value,
        })),
    )
    .filter((entry, index, entries) => {
      return entries.findIndex((item) => item.id === entry.id) === index;
    });
}

function metadataDateEntries(
  library: Library,
  entity: Library["entities"][number],
): EntityDateMetadataEntry[] {
  const type = library.config.types.find((item) => item.id === entity.type);
  const fields = type?.fields.date ?? [];
  const seen = new Set<string>();
  const entries: EntityDateMetadataEntry[] = [];

  for (const item of entity.dates) {
    if (fields.length > 0 && !fields.includes(item.field)) continue;
    const key = `${item.field}\u0000${item.value}`;
    if (seen.has(key)) continue;
    seen.add(key);
    entries.push({
      id: `metadata:${item.field}:${entries.length}`,
      field: item.field,
      value: item.value,
      date: parseExactDate(item.value),
    });
  }

  return entries.sort((a, b) => {
    const dateCompare = compareString(b.date ?? b.value, a.date ?? a.value);
    if (dateCompare !== 0) return dateCompare;
    return compareString(a.field, b.field);
  });
}

async function entityDailyNoteEntries(
  library: Library,
  entity: EntitySummary,
): Promise<EntityDateDailyNoteEntry[]> {
  const dailyFiles = await dailyNoteFiles(library);
  const byBasename = entityBasenameIndex(library);
  const grouped = new Map<string, EntityDateDailyNoteEntry>();
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
      const mentionsEntity = links.some((link) => {
        const target = link[1];
        if (!target) return false;
        return findEntityForWikilink(target, library, byBasename)?.id === entity.id;
      });
      if (!mentionsEntity) continue;

      const entry =
        grouped.get(file.date) ??
        ({
          id: `daily-note:${file.date}:${entity.id}`,
          date: file.date,
          notePath: file.relativePath,
          snippets: [],
        } satisfies EntityDateDailyNoteEntry);
      const snippet = {
        text: cleanMentionSnippet(block.text, snippetMaxLength),
        heading: block.heading,
        line: block.line,
      };
      if (!entry.snippets.some((item) => item.text === snippet.text)) {
        entry.snippets.push(snippet);
      }
      grouped.set(file.date, entry);
    }
  }

  return [...grouped.values()]
    .map((entry) => ({
      ...entry,
      snippets: entry.snippets.slice(0, 5),
    }))
    .sort((a, b) => compareString(b.date, a.date));
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

async function dailyNoteFiles(library: Library, year?: number, month?: number) {
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
        if (year === undefined || month === undefined) return true;
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
