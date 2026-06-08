import { externalFieldOptionsBySource, externalSourceOptions } from "@/lib/external-metadata";
import { isIso639TitleLanguage } from "@/lib/title-language";
import type {
  DailyNotesConfig,
  EntityTypeConfig,
  ExternalFieldMapping,
  FieldConfig,
  FilenameConfig,
  HomeConfig,
  HomeSectionConfig,
  HomeSectionFilterConfig,
  KizunaConfig,
} from "@/types/config";

export function normalizeConfig(config?: KizunaConfig): KizunaConfig {
  if (!config) return defaultConfig();
  return {
    vaultRoot: config.vaultRoot ?? "",
    taxonomyRoot: config.taxonomyRoot ?? "Taxonomy",
    contentWritable: config.contentWritable ?? true,
    readConcurrency: config.readConcurrency ?? null,
    dailyNotes: config.dailyNotes ? normalizeDailyNotes(config.dailyNotes) : null,
    home: config.home ? normalizeHome(config.home) : null,
    types: (config.types ?? []).map(normalizeEntityType),
  };
}

function normalizeDailyNotes(config: DailyNotesConfig): DailyNotesConfig {
  return {
    paths: config.paths ?? [],
    datePattern: config.datePattern ?? "",
    snippetMaxLength: config.snippetMaxLength ?? null,
  };
}

function normalizeHome(config: HomeConfig): HomeConfig {
  return {
    title: config.title ?? "",
    sections: config.sections ?? [],
  };
}

function normalizeEntityType(config: EntityTypeConfig): EntityTypeConfig {
  return {
    id: config.id ?? "",
    label: config.label ?? "",
    icon: config.icon ?? "",
    path: config.path ?? "",
    externalPriority: config.externalPriority ?? [],
    filename: config.filename
      ? {
          titleLanguage: config.filename.titleLanguage ?? "",
          defaultTitle: config.filename.defaultTitle ?? false,
        }
      : null,
    fields: (config.fields ?? []).map(normalizeField),
  };
}

function normalizeField(field: FieldConfig): FieldConfig {
  return {
    field: field.field ?? "",
    fieldType: field.fieldType ?? "text",
    displayName: field.displayName ?? "",
    titleLanguage: field.titleLanguage ?? "",
    titleRole: field.titleRole ?? null,
    externalFields: field.externalFields ?? [],
    defaultTitle: field.defaultTitle ?? false,
    enumOptions: field.enumOptions ?? [],
    totalProgressField: field.totalProgressField ?? "",
    dateRole: field.dateRole ?? null,
    seasonLanguage: field.seasonLanguage ?? "zh",
    externalRef: field.externalRef ?? "",
    externalTypes: field.externalTypes ?? [],
    relationType: field.relationType ?? "",
  };
}

export function cleanConfig(config: KizunaConfig): KizunaConfig {
  return {
    vaultRoot: config.vaultRoot,
    taxonomyRoot: config.taxonomyRoot,
    contentWritable: config.contentWritable ?? undefined,
    readConcurrency: config.readConcurrency ?? undefined,
    dailyNotes: config.dailyNotes
      ? {
          paths: cleanStrings(config.dailyNotes.paths),
          datePattern: emptyToUndefined(config.dailyNotes.datePattern),
          snippetMaxLength: config.dailyNotes.snippetMaxLength ?? undefined,
        }
      : undefined,
    home: config.home
      ? {
          title: emptyToUndefined(config.home.title),
          sections: config.home.sections.map((section) => ({
            id: section.id,
            title: section.title,
            type: section.type,
            filters: cleanHomeSectionFilters(section.filters ?? []),
            limit: section.limit ?? undefined,
            sort: emptyToUndefined(section.sort),
            direction: section.direction ?? undefined,
          })),
        }
      : undefined,
    types: config.types.map((typeConfig) => ({
      id: typeConfig.id,
      label: typeConfig.label,
      icon: emptyToUndefined(typeConfig.icon),
      path: typeConfig.path,
      externalPriority: cleanExternalPriority(typeConfig.externalPriority ?? []),
      filename: cleanFilename(typeConfig.filename),
      fields: typeConfig.fields
        .map(cleanField)
        .filter((field): field is FieldConfig => Boolean(field)),
    })),
  };
}

function cleanHomeSectionFilters(filters: HomeSectionFilterConfig[]) {
  const cleaned = filters
    .map((filter) => ({
      field: filter.field.trim(),
      values: cleanStrings(filter.values ?? []),
    }))
    .filter((filter) => filter.field);
  return cleaned.length > 0 ? cleaned : undefined;
}

function cleanFilename(filename: FilenameConfig | null | undefined): FilenameConfig | undefined {
  if (!filename) return undefined;
  const titleLanguage = isIso639TitleLanguage(filename.titleLanguage)
    ? filename.titleLanguage
    : undefined;
  if (!titleLanguage && !filename.defaultTitle) return undefined;
  return {
    titleLanguage,
    defaultTitle: filename.defaultTitle || undefined,
  };
}

function cleanExternalFieldMappings(values: ExternalFieldMapping[]) {
  const cleaned = values
    .map((value) => ({
      source: value.source.trim(),
      field: value.field.trim(),
    }))
    .filter((value) =>
      externalFieldOptionsBySource[value.source]?.some((option) => option.field === value.field),
    );
  return cleaned.length > 0 ? cleaned : undefined;
}

function cleanExternalTypes(source: string | null | undefined, values: string[]) {
  const options = externalTypeOptionsBySource[source ?? ""] ?? [];
  const allowed = new Set(options.map((option) => option.value));
  const cleaned = values.map((value) => value.trim()).filter((value) => allowed.has(value));
  return cleaned.length > 0 ? cleaned : undefined;
}

function cleanField(field: FieldConfig): FieldConfig | undefined {
  const key = field.field.trim();
  if (!key) return undefined;
  return {
    field: key,
    fieldType: field.fieldType,
    displayName: emptyToUndefined(field.displayName),
    titleLanguage:
      field.fieldType === "title" && isIso639TitleLanguage(field.titleLanguage)
        ? field.titleLanguage
        : undefined,
    titleRole: field.fieldType === "title" ? field.titleRole || undefined : undefined,
    externalFields:
      field.fieldType !== "externalRef"
        ? cleanExternalFieldMappings(field.externalFields ?? [])
        : undefined,
    defaultTitle: field.fieldType === "title" && field.defaultTitle ? true : undefined,
    enumOptions:
      field.fieldType === "enum" || field.fieldType === "enumList"
        ? cleanStrings(field.enumOptions ?? [])
        : undefined,
    totalProgressField:
      field.fieldType === "progress" ? emptyToUndefined(field.totalProgressField) : undefined,
    dateRole:
      field.fieldType === "date" || field.fieldType === "season"
        ? field.dateRole || undefined
        : undefined,
    seasonLanguage: field.fieldType === "season" ? field.seasonLanguage || "zh" : undefined,
    externalRef: field.fieldType === "externalRef" ? emptyToUndefined(field.externalRef) : undefined,
    externalTypes:
      field.fieldType === "externalRef"
        ? cleanExternalTypes(field.externalRef, field.externalTypes ?? [])
        : undefined,
    relationType: field.fieldType === "relation" ? emptyToUndefined(field.relationType) : undefined,
  };
}

export function defaultConfig(): KizunaConfig {
  return {
    vaultRoot: "",
    taxonomyRoot: "Taxonomy",
    contentWritable: true,
    readConcurrency: 8,
    dailyNotes: defaultDailyNotes(),
    home: defaultHome(),
    types: [defaultEntityType()],
  };
}

export function defaultDailyNotes(): DailyNotesConfig {
  return {
    paths: ["Daily Notes"],
    datePattern: "^(\\d{4}-\\d{2}-\\d{2})\\.md$",
    snippetMaxLength: 260,
  };
}

export function defaultHome(): HomeConfig {
  return { title: "Home", sections: [] };
}

export function defaultHomeSection(type = ""): HomeSectionConfig {
  return {
    id: "section",
    title: "Section",
    type,
    limit: 12,
    sort: "title",
    direction: "asc",
  };
}

export function defaultEntityType(): EntityTypeConfig {
  return {
    id: "type",
    label: "Type",
    icon: "",
    path: "Type",
    externalPriority: [],
    filename: { defaultTitle: true },
    fields: [
      { field: "id", fieldType: "id", displayName: "ID" },
      {
        field: "state",
        fieldType: "enum",
        displayName: "State",
        enumOptions: ["Backlog", "Active", "Completed", "Paused", "Dropped"],
      },
      { field: "progress", fieldType: "progress", displayName: "Progress" },
    ],
  };
}

export function defaultField(): FieldConfig {
  return { field: "field", fieldType: "text", displayName: "" };
}

export function vaultTemplates(): Array<{ id: string; label: string; config: KizunaConfig }> {
  return [
    {
      id: "media",
      label: "Media Library",
      config: {
        ...defaultConfig(),
        types: [
          mediaType("anime", "Anime", "📺", "Anime", ["bangumi_url"], ["season", "release_date"], ["complete_date"]),
          mediaType("drama", "Drama", "🎭", "Drama", ["thetvdb_url"], ["season", "release_date"], ["complete_date"]),
          mediaType("movie", "Movie", "🎬", "Movie", ["bangumi_url", "thetvdb_url"], ["release_date"], ["complete_date"]),
          mediaType("games", "Games", "🎮", "Games", ["igdb_url"], ["release_date"], ["complete_date"]),
        ],
        home: {
          title: "Home",
          sections: [
            { id: "recent-anime", title: "Recent Anime", type: "anime", limit: 12, sort: "date:season", direction: "desc" },
            { id: "games", title: "Games", type: "games", limit: 12, sort: "title", direction: "asc" },
          ],
        },
      },
    },
    {
      id: "watching",
      label: "Anime + Drama + Movies",
      config: {
        ...defaultConfig(),
        types: [
          mediaType("anime", "Anime", "📺", "Anime", ["bangumi_url"], ["season", "release_date"], ["complete_date"]),
          mediaType("drama", "Drama", "🎭", "Drama", ["thetvdb_url"], ["season", "release_date"], ["complete_date"]),
          mediaType("movie", "Movie", "🎬", "Movie", ["bangumi_url", "thetvdb_url"], ["release_date"], ["complete_date"]),
        ],
      },
    },
    {
      id: "games",
      label: "Games",
      config: {
        ...defaultConfig(),
        types: [mediaType("games", "Games", "🎮", "Games", ["igdb_url"], ["release_date"], ["complete_date"])],
      },
    },
    {
      id: "books",
      label: "Books",
      config: {
        ...defaultConfig(),
        types: [mediaType("books", "Books", "📚", "Books", ["openlibrary_url", "isbn"], ["release_date"], ["complete_date"])],
      },
    },
    {
      id: "blank",
      label: "Custom Blank",
      config: {
        ...defaultConfig(),
        home: { title: "Home", sections: [] },
        types: [defaultEntityType()],
      },
    },
  ];
}

function mediaType(
  id: string,
  label: string,
  icon: string,
  path: string,
  externalRefs: string[],
  planningDates: string[],
  completedDates: string[],
): EntityTypeConfig {
  const stateOptions = ["Backlog", "Watching", "Playing", "Reading", "Completed", "Paused", "Dropped"];
  return {
    id,
    label,
    icon,
    path,
    externalPriority: defaultExternalPriority(externalRefs),
    filename: { titleLanguage: "zh", defaultTitle: true },
    fields: [
      { field: "uid", fieldType: "id", displayName: "UID" },
      { field: "id", fieldType: "id", displayName: "ID" },
      { field: "title", fieldType: "title", displayName: "Title", titleLanguage: "zh", externalFields: defaultExternalMappings(id, "title") },
      { field: "title_original", fieldType: "title", displayName: "Title (Original)", titleRole: "original", externalFields: defaultExternalMappings(id, "originalTitle") },
      { field: "title_en", fieldType: "title", displayName: "Title (English)", titleLanguage: "en", externalFields: defaultExternalMappings(id, "titleEn") },
      { field: "title_ja", fieldType: "title", displayName: "Title (Japanese)", titleLanguage: "ja", externalFields: defaultExternalMappings(id, "titleJa") },
      { field: "cover_url", fieldType: "image", displayName: "Cover", externalFields: defaultExternalMappings(id, "cover") },
      { field: "state", fieldType: "enum", displayName: "State", enumOptions: stateOptions },
      { field: "progress", fieldType: "progress", displayName: "Progress", totalProgressField: "episodes" },
      { field: "episodes", fieldType: "totalProgress", displayName: "Episodes" },
      { field: "rating", fieldType: "rating", displayName: "Rating" },
      ...planningDates.map((field) =>
        field === "season"
          ? ({
              field,
              fieldType: "season",
              displayName: "Season",
              dateRole: "planning",
              seasonLanguage: "zh",
            } satisfies FieldConfig)
          : ({
              field,
              fieldType: "date",
              displayName: field === "release_date" ? "Release date" : field,
              dateRole: "planning",
              externalFields: field === "release_date" ? defaultExternalMappings(id, "releaseDate") : [],
            } satisfies FieldConfig),
      ),
      ...completedDates.map((field) => ({
        field,
        fieldType: "date",
        displayName: field === "complete_date" ? "Completed date" : field,
        dateRole: "completed",
      }) satisfies FieldConfig),
      ...externalRefs.map((field) => ({
        field,
        fieldType: "externalRef",
        displayName: field,
        externalRef: externalSourceForField(field),
        externalTypes: externalTypesForSource(externalSourceForField(field), id),
      }) satisfies FieldConfig),
      { field: "franchise", fieldType: "relation", displayName: "Franchise", relationType: "franchise" },
      { field: "studio", fieldType: "relation", displayName: "Studio", relationType: "studio" },
      { field: "developer", fieldType: "relation", displayName: "Developer", relationType: "developer" },
    ],
  };
}

export function externalTypesForSource(source: string, typeId: string) {
  if (source === "igdb") return ["game"];
  if (source === "thetvdb") return typeId === "movie" ? ["movie"] : ["series"];
  if (source === "bangumi") {
    if (typeId === "games") return ["4"];
    if (typeId === "music" || typeId === "cd") return ["3"];
    if (typeId === "books" || typeId === "book") return ["1"];
    if (typeId === "drama") return ["6"];
    if (typeId === "movie") return ["2", "6"];
    return ["2"];
  }
  return [];
}

function externalSourceForField(field: string) {
  return field.replace(/_url$/, "");
}

function defaultExternalPriority(externalRefs: string[]) {
  return cleanExternalPriority(externalRefs.map(externalSourceForField)) ?? [];
}

function defaultExternalMappings(typeId: string, role: string): ExternalFieldMapping[] {
  const source = typeId === "games" ? "igdb" : typeId === "drama" ? "thetvdb" : "bangumi";
  const field = defaultExternalField(source, role);
  return field ? [{ source, field }] : [];
}

function defaultExternalField(source: string, role: string) {
  if (source === "bangumi") {
    if (role === "title") return "name_cn";
    if (role === "originalTitle" || role === "titleJa") return "name";
    if (role === "cover") return "cover_url";
    if (role === "releaseDate") return "date";
  }
  if (source === "igdb") {
    if (role === "title" || role === "originalTitle") return "name";
    if (role === "cover") return "cover_url";
    if (role === "releaseDate") return "first_release_date";
  }
  if (source === "thetvdb") {
    if (role === "title" || role === "originalTitle") return "name";
    if (role === "cover") return "cover_url";
    if (role === "releaseDate") return "first_air_time";
  }
  return undefined;
}

export const externalTypeOptionsBySource: Record<string, Array<{ value: string; label: string }>> = {
  bangumi: [
    { value: "1", label: "Book (1)" },
    { value: "2", label: "Anime (2)" },
    { value: "3", label: "Music (3)" },
    { value: "4", label: "Game (4)" },
    { value: "6", label: "Real (6)" },
  ],
  igdb: [{ value: "game", label: "Game" }],
  thetvdb: [
    { value: "series", label: "Series" },
    { value: "movie", label: "Movie" },
  ],
};

export function replaceAt<T, K extends keyof T>(
  object: T,
  key: K,
  index: number,
  value: T[K] extends Array<infer U> ? U : never,
): T {
  const current = object[key];
  if (!Array.isArray(current)) return object;
  return { ...object, [key]: replaceArray(current, index, value) };
}

export function replaceArray<T>(items: T[], index: number, value: T) {
  return items.map((item, itemIndex) => (itemIndex === index ? value : item));
}

function cleanStrings(values: string[]) {
  return values.map((value) => value.trim()).filter(Boolean);
}

function cleanExternalPriority(values: string[]) {
  const allowed = new Set(externalSourceOptions.map((option) => option.source));
  const cleaned = values
    .map((value) => value.trim().toLowerCase())
    .filter((value, index, items) => allowed.has(value) && items.indexOf(value) === index);
  return cleaned.length > 0 ? cleaned : undefined;
}

function emptyToUndefined(value?: string | null) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}

export function joinPath(base: string, path: string) {
  if (!base) return path;
  if (!path) return base;
  return `${base.replace(/\/+$/, "")}/${path.replace(/^\/+/, "")}`;
}

export function relativeToBase(path: string, base: string) {
  const normalizedBase = base.replace(/\/+$/, "");
  if (path === normalizedBase) return "";
  if (path.startsWith(`${normalizedBase}/`)) return path.slice(normalizedBase.length + 1);
  return path;
}
