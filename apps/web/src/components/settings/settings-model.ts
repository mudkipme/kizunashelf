import {
  defaultExternalBodyMappings,
  defaultExternalMappings,
  defaultExternalPriority,
  externalFieldOptionsForSource,
  externalTypeOptionsForSource,
  externalTypesForSource,
  externalSourceOptions,
} from "@/lib/external-metadata";
import { isIso639TitleLanguage } from "@/lib/title-language";
import type { ExternalProviderCatalog } from "@/types/api";
import type {
  AppConfig,
  DailyNotesConfig,
  EntityTypeConfig,
  ExternalBodyMapping,
  ExternalFieldMapping,
  FieldConfig,
  FilenameConfig,
  HomeConfig,
  HomeSectionConfig,
  HomeSectionFilterConfig,
  MergedConfig,
  SaveSettingsRequest,
  VaultConfig,
} from "@/types/config";

export function normalizeConfig(app?: AppConfig, vault?: VaultConfig): MergedConfig {
  const base = defaultConfig();
  return {
    vaultRoot: app?.vaultRoot ?? base.vaultRoot,
    contentWritable: app?.contentWritable ?? base.contentWritable,
    // When the vault config is missing entirely (e.g. a fresh vault), seed the
    // defaults so the editor has something to fill in; when it exists, respect
    // its values including disabled (null) daily notes / home.
    taxonomyRoot: vault?.taxonomyRoot ?? base.taxonomyRoot,
    assetRoot: vault?.assetRoot ?? base.assetRoot,
    dailyNotes: vault
      ? vault.dailyNotes
        ? normalizeDailyNotes(vault.dailyNotes)
        : null
      : base.dailyNotes,
    home: vault ? (vault.home ? normalizeHome(vault.home) : null) : base.home,
    types: vault ? (vault.types ?? []).map(normalizeEntityType) : base.types,
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
    bodyMappings: config.bodyMappings ?? [],
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

export function cleanConfig(
  config: MergedConfig,
  providerCatalog?: ExternalProviderCatalog,
): SaveSettingsRequest {
  return {
    app: {
      vaultRoot: config.vaultRoot,
      contentWritable: config.contentWritable ?? undefined,
    },
    vault: {
      taxonomyRoot: config.taxonomyRoot,
      assetRoot: emptyToUndefined(config.assetRoot),
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
        externalPriority: cleanExternalPriority(providerCatalog, typeConfig.externalPriority ?? []),
        filename: cleanFilename(typeConfig.filename),
        bodyMappings: cleanExternalBodyMappings(typeConfig.bodyMappings ?? [], providerCatalog),
        fields: typeConfig.fields
          .map((field) => cleanField(field, providerCatalog))
          .filter((field): field is FieldConfig => Boolean(field)),
      })),
    },
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

function cleanExternalFieldMappings(values: ExternalFieldMapping[], providerCatalog?: ExternalProviderCatalog) {
  const cleaned = values
    .map((value) => ({
      source: value.source.trim(),
      field: value.field.trim(),
    }))
    .filter((value) =>
      externalFieldOptionsForSource(providerCatalog, value.source).some((option) => option.field === value.field),
    );
  return cleaned.length > 0 ? cleaned : undefined;
}

function cleanExternalBodyMappings(values: ExternalBodyMapping[], providerCatalog?: ExternalProviderCatalog) {
  const seen = new Set<string>();
  const cleaned = values
    .map((value) => ({
      source: value.source.trim(),
      field: value.field.trim(),
      heading: value.heading.trim(),
    }))
    .filter((value) => {
      if (!value.heading) return false;
      if (!externalFieldOptionsForSource(providerCatalog, value.source).some((option) => option.field === value.field)) {
        return false;
      }
      const key = `${value.source}:${value.field}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  return cleaned.length > 0 ? cleaned : undefined;
}

function cleanExternalTypes(providerCatalog: ExternalProviderCatalog | undefined, source: string | null | undefined, values: string[]) {
  const options = externalTypeOptionsForSource(providerCatalog, source ?? "");
  const allowed = new Set(options.map((option) => option.value));
  const cleaned = values.map((value) => value.trim()).filter((value) => allowed.has(value));
  return cleaned.length > 0 ? cleaned : undefined;
}

function cleanField(field: FieldConfig, providerCatalog?: ExternalProviderCatalog): FieldConfig | undefined {
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
        ? cleanExternalFieldMappings(field.externalFields ?? [], providerCatalog)
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
        ? cleanExternalTypes(providerCatalog, field.externalRef, field.externalTypes ?? [])
        : undefined,
    relationType: field.fieldType === "relation" ? emptyToUndefined(field.relationType) : undefined,
  };
}

export function defaultConfig(): MergedConfig {
  return {
    vaultRoot: "",
    taxonomyRoot: "Taxonomy",
    assetRoot: "Assets",
    contentWritable: true,
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
    bodyMappings: [],
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

export function vaultTemplates(providerCatalog?: ExternalProviderCatalog): Array<{ id: string; label: string; config: MergedConfig }> {
  return [
    {
      id: "media",
      label: "Media Library",
      config: {
        ...defaultConfig(),
        types: [
          mediaType(providerCatalog, "anime", "Anime", "📺", "Anime", [externalRef("bangumi_url", "bangumi")], ["season", "release_date"], ["complete_date"]),
          mediaType(providerCatalog, "drama", "Drama", "🎭", "Drama", [externalRef("thetvdb_url", "thetvdb")], ["season", "release_date"], ["complete_date"]),
          mediaType(providerCatalog, "movie", "Movie", "🎬", "Movie", [externalRef("bangumi_url", "bangumi"), externalRef("thetvdb_url", "thetvdb")], ["release_date"], ["complete_date"]),
          mediaType(providerCatalog, "games", "Games", "🎮", "Games", [externalRef("igdb_url", "igdb")], ["release_date"], ["complete_date"]),
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
          mediaType(providerCatalog, "anime", "Anime", "📺", "Anime", [externalRef("bangumi_url", "bangumi")], ["season", "release_date"], ["complete_date"]),
          mediaType(providerCatalog, "drama", "Drama", "🎭", "Drama", [externalRef("thetvdb_url", "thetvdb")], ["season", "release_date"], ["complete_date"]),
          mediaType(providerCatalog, "movie", "Movie", "🎬", "Movie", [externalRef("bangumi_url", "bangumi"), externalRef("thetvdb_url", "thetvdb")], ["release_date"], ["complete_date"]),
        ],
      },
    },
    {
      id: "games",
      label: "Games",
      config: {
        ...defaultConfig(),
        types: [mediaType(providerCatalog, "games", "Games", "🎮", "Games", [externalRef("igdb_url", "igdb")], ["release_date"], ["complete_date"])],
      },
    },
    {
      id: "books",
      label: "Books",
      config: {
        ...defaultConfig(),
        types: [mediaType(providerCatalog, "books", "Books", "📚", "Books", [], ["release_date"], ["complete_date"])],
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

type ExternalRefTemplate = {
  field: string;
  source: string;
};

function externalRef(field: string, source: string): ExternalRefTemplate {
  return { field, source };
}

function mediaType(
  providerCatalog: ExternalProviderCatalog | undefined,
  id: string,
  label: string,
  icon: string,
  path: string,
  externalRefs: ExternalRefTemplate[],
  planningDates: string[],
  completedDates: string[],
): EntityTypeConfig {
  const stateOptions = ["Backlog", "Watching", "Playing", "Reading", "Completed", "Paused", "Dropped"];
  return {
    id,
    label,
    icon,
    path,
    externalPriority: defaultExternalPriority(providerCatalog, externalRefs.map((ref) => ref.source)),
    filename: { titleLanguage: "zh", defaultTitle: true },
    bodyMappings: defaultExternalBodyMappings(providerCatalog, defaultExternalSource(externalRefs), "summary", "Summary"),
    fields: [
      { field: "uid", fieldType: "id", displayName: "UID" },
      { field: "id", fieldType: "id", displayName: "ID" },
      { field: "title", fieldType: "title", displayName: "Title", titleLanguage: "zh", externalFields: defaultExternalMappings(providerCatalog, defaultExternalSource(externalRefs), "title") },
      { field: "title_original", fieldType: "title", displayName: "Title (Original)", titleRole: "original", externalFields: defaultExternalMappings(providerCatalog, defaultExternalSource(externalRefs), "originalTitle") },
      { field: "title_en", fieldType: "title", displayName: "Title (English)", titleLanguage: "en", externalFields: defaultExternalMappings(providerCatalog, defaultExternalSource(externalRefs), "titleEn") },
      { field: "title_ja", fieldType: "title", displayName: "Title (Japanese)", titleLanguage: "ja", externalFields: defaultExternalMappings(providerCatalog, defaultExternalSource(externalRefs), "titleJa") },
      { field: "cover_url", fieldType: "image", displayName: "Cover", externalFields: defaultExternalMappings(providerCatalog, defaultExternalSource(externalRefs), "cover") },
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
              externalFields: field === "release_date" ? defaultExternalMappings(providerCatalog, defaultExternalSource(externalRefs), "releaseDate") : [],
            } satisfies FieldConfig),
      ),
      ...completedDates.map((field) => ({
        field,
        fieldType: "date",
        displayName: field === "complete_date" ? "Completed date" : field,
        dateRole: "completed",
      }) satisfies FieldConfig),
      ...externalRefs.map((ref) => ({
        field: ref.field,
        fieldType: "externalRef",
        displayName: ref.field,
        externalRef: ref.source,
        externalTypes: externalTypesForSource(providerCatalog, ref.source),
      }) satisfies FieldConfig),
      { field: "franchise", fieldType: "relation", displayName: "Franchise", relationType: "franchise" },
    ],
  };
}

function defaultExternalSource(externalRefs: ExternalRefTemplate[]) {
  return externalRefs[0]?.source ?? "";
}

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

function cleanExternalPriority(providerCatalog: ExternalProviderCatalog | undefined, values: string[]) {
  const allowed = new Set(externalSourceOptions(providerCatalog).map((option) => option.source));
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
